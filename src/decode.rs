extern crate ffmpeg_next as ffmpeg;

use ffmpeg::{
  codec::{
    Context as AvContext,
    decoder::Video as AvDecoder,
  },
  software::scaling::{
    context::Context as AvScaler,
    flag::Flags as AvScalerFlags,
  },
  util::{
    format::pixel::Pixel as AvPixel,
    error::EAGAIN,
  },
  Error as AvError,
  Rational as AvRational,
};

use super::{
  RawFrame,
  io::Reader,
  options::Options,
  frame::FRAME_PIXEL_FORMAT,
  ffi::{
    set_decoder_context_time_base,
    copy_frame_props,
  },
  Resize,
  Locator,
  Error,
};

#[cfg(feature = "ndarray")]
use super::{
  Frame,
  Time,
  ffi::convert_frame_to_ndarray_rgb24,
};

type Result<T> = std::result::Result<T, Error>;

/// Decodes video streams and provides the caller with decoded RGB frames.
/// 
/// # Example
/// 
/// ```
/// let decoder = Decoder::new(&PathBuf::from("video.mp4")).unwrap();
/// decoder
///   .decode_iter()
///   .take_while(Result::is_ok)
///   .for_each(|frame| println!("Got frame!"));
/// ```
pub struct Decoder {
  reader: Reader,
  reader_stream_index: usize,
  decoder: AvDecoder,
  decoder_time_base: AvRational,
  scaler: AvScaler,
  size: (u32, u32),
  size_out: (u32, u32),
}

impl Decoder {

  /// Create a new decoder for the specified file.
  /// 
  /// # Arguments
  /// 
  /// * `source` - Locator to file to decode.
  pub fn new(
    source: &Locator,
  ) -> Result<Self> {
    Self::from_reader(
      Reader::new(source)?,
      None,
    )
  }

  /// Create a new decoder for the specified file with input options.
  /// 
  /// # Arguments
  /// 
  /// * `source` - Locator to file to decode.
  /// * `options` - The input options.
  pub fn new_with_options(
    source: &Locator,
    options: &Options,
  ) -> Result<Self> {
    Self::from_reader(
      Reader::new_with_options(source, options)?,
      None,
    )
  }

  /// Create a new decoder for the specified file with input options and
  /// custom dimensions. Each frame will be resized to the given dimensions.
  /// 
  /// # Arguments
  /// 
  /// * `source` - Locator to file to decode.
  /// * `options` - The input options.
  /// * `resize` - How to resize frames.
  /// 
  /// # Example
  /// 
  /// ```
  /// let decoder = Decoder::new_with_options_and_resize(
  ///     &PathBuf::from("from_file.mp4").into(),
  ///     Options::new_with_rtsp_transport_tcp(),
  ///     Resize::Exact(800, 600))
  ///  .unwrap();
  /// ```
  pub fn new_with_options_and_resize(
    source: &Locator,
    options: &Options,
    resize: Resize,
  ) -> Result<Self> {
    Self::from_reader(
      Reader::new_with_options(source, options)?,
      Some(resize),
    )
  }

  /// Decode frames through iterator interface. This is similar to `decode`
  /// but it returns frames through an infinite iterator.
  /// 
  /// # Example
  /// 
  /// ```
  /// decoder
  ///   .decode_iter()
  ///   .take_while(Result::is_ok)
  ///   .map(Result::unwrap)
  ///   .for_each(|(ts, frame)| {
  ///     // Do something with frame...
  ///   });
  /// ```
  #[cfg(feature = "ndarray")]
  pub fn decode_iter(
    &mut self,
  ) -> impl Iterator<Item=Result<(Time, Frame)>> + '_ {
    std::iter::from_fn(move || {
      Some(self.decode())
    })
  }

  /// Decode a single frame.
  /// 
  /// # Returns
  /// 
  /// A tuple of the frame timestamp (relative to the stream) and the
  /// frame itself.
  /// 
  /// # Example
  /// 
  /// ```
  /// loop {
  ///   let (ts, frame) = decoder.decode()?;
  ///   // Do something with frame...
  /// }
  /// ```
  #[cfg(feature = "ndarray")]
  pub fn decode(&mut self) -> Result<(Time, Frame)> {
    let frame = &mut self.decode_raw()?;
    // We use the packet DTS here (which is `frame->pkt_dts`) because that is
    // what the encoder will use when encoding for the `PTS` field.
    let timestamp = Time::new(Some(frame.packet().dts), self.decoder_time_base);
    let frame = convert_frame_to_ndarray_rgb24(frame)
      .map_err(Error::BackendError)?;

    Ok((timestamp, frame))
  }

  /// Decode frames through iterator interface. This is similar to `decode_raw`
  /// but it returns frames through an infinite iterator.
  pub fn decode_raw_iter(
    &mut self,
  ) -> impl Iterator<Item=Result<RawFrame>> + '_ {
    std::iter::from_fn(move || {
      Some(self.decode_raw())
    })
  }

  /// Decode a single frame and return the raw ffmpeg `AvFrame`.
  pub fn decode_raw(&mut self) -> Result<RawFrame> {
    let mut frame: Option<RawFrame> = None;
    while frame.is_none() {
      let mut packet = self
        .reader
        .read(self.reader_stream_index)?
        .into_inner();
      packet.rescale_ts(self.stream_time_base(), self.decoder_time_base);

      self.decoder.send_packet(&packet)
        .map_err(Error::BackendError)?;

      frame = self.decoder_receive_frame()?;
    }

    let frame = frame.unwrap();
    let mut frame_scaled = RawFrame::empty();
    self
      .scaler
      .run(&frame, &mut frame_scaled)
      .map_err(Error::BackendError)?;

    copy_frame_props(&frame, &mut frame_scaled);

    Ok(frame_scaled)
  }

  /// Get the decoders input size (resolution dimensions): width and height.
  pub fn size(&self) -> (u32, u32) {
    self.size
  }

  /// Get the decoders output size after resizing is applied (resolution
  /// dimensions): width and height.
  pub fn size_out(&self) -> (u32, u32) {
    self.size_out
  }

  /// Get the decoders input frame rate as floating-point value.
  pub fn frame_rate(&self) -> f32 {
    let frame_rate = self
      .reader
      .input
      .stream(self.reader_stream_index)
      .map(|stream| stream.rate());
    
    if let Some(frame_rate) = frame_rate {
      if frame_rate.denominator() > 0 {
        (frame_rate.numerator() as f32) / (frame_rate.denominator() as f32)
      } else {
        0.0
      }
    } else {
      0.0
    }
  }

  /// Create a decoder from a `Reader` instance. Optionally provide
  /// dimensions to resize frames to.
  /// 
  /// # Arguments
  /// 
  /// * `reader` - `Reader` to create decoder from.
  /// * `resize` - Optional resize strategy to apply to frames.
  fn from_reader(
    reader: Reader,
    resize: Option<Resize>,
  ) -> Result<Self> {
    let reader_stream_index = reader.best_video_stream_index()?;
    let reader_stream = reader
      .input
      .stream(reader_stream_index)
      .ok_or(AvError::StreamNotFound)?;

    let mut decoder = AvContext::new();
    set_decoder_context_time_base(&mut decoder, reader_stream.time_base());
    decoder.set_parameters(reader_stream.parameters())?;
    let decoder = decoder
      .decoder()
      .video()?;
    let decoder_time_base = decoder.time_base();

    let (resize_width, resize_height) = match resize {
      Some(resize) => {
        resize
          .compute_for((decoder.width(), decoder.height()))
          .ok_or_else(|| Error::InvalidResizeParameters)?
      },
      None => (decoder.width(), decoder.height()),
    };

    if decoder.format() == AvPixel::None ||
       decoder.width() == 0 || decoder.height() == 0 {
      return Err(Error::MissingCodecParameters);
    }

    let scaler = AvScaler::get(
      decoder.format(),
      decoder.width(),
      decoder.height(),
      FRAME_PIXEL_FORMAT,
      resize_width,
      resize_height,
      AvScalerFlags::AREA)?;

    let size = (decoder.width(), decoder.height());
    let size_out = (resize_width, resize_height);

    Ok(Self {
      reader,
      reader_stream_index,
      decoder,
      decoder_time_base,
      scaler,
      size,
      size_out,
      frame_rate,
    })
  }
  
  /// Pull a decoded frame from the decoder. This function also implements
  /// retry mechanism in case the decoder signals `EAGAIN`.
  fn decoder_receive_frame(&mut self) -> Result<Option<RawFrame>> {
    let mut frame = RawFrame::empty();
    let decode_result = self.decoder.receive_frame(&mut frame);
    match decode_result {
      Ok(())
        => Ok(Some(frame)),
      Err(AvError::Other { errno }) if errno == EAGAIN
        => Ok(None),
      Err(err)
        => Err(err.into()),
    }
  }

  // Acquire the time base of the input stream.
  fn stream_time_base(&self) -> AvRational {
    self
      .reader
      .input
      .stream(self.reader_stream_index)
      .unwrap()
      .time_base()
  }

}

impl Drop for Decoder {

  fn drop(&mut self) {
    // Maximum number of invocations to `decoder_receive_frame`
    // to drain the items still on the queue before giving up.
    const MAX_DRAIN_ITERATIONS: u32 = 100;

    // We need to drain the items still in the decoders queue.
    if let Ok(()) = self.decoder.send_eof() {
      for _ in 0..MAX_DRAIN_ITERATIONS {
        if self.decoder_receive_frame().is_err() {
          break;
        }
      }
    }
  }

}
