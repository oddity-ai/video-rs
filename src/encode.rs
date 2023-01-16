extern crate ffmpeg_next as ffmpeg;

use ffmpeg::{
  format::flag::Flags as AvFormatFlags,
  codec::{
    Id as AvCodecId,
    Context as AvContext,
    codec::Codec as AvCodec,
    packet::Packet as AvPacket,
    encoder::video::Video as AvEncoder,
    flag::Flags as AvCodecFlags,
  },
  software::scaling::{
    context::Context as AvScaler,
    flag::Flags as AvScalerFlags,
  },
  util::{
    format::Pixel as AvPixel,
    picture::Type as AvFrameType,
    mathematics::rescale::TIME_BASE,
    error::EAGAIN,
  },
  StreamMut,
  Rational as AvRational,
  Error as AvError,
};

use super::{
  Error,
  Locator,
  RawFrame,
  io::{
    Writer,
    private::Write,
  },
  options::Options,
  frame::FRAME_PIXEL_FORMAT,
  ffi::get_encoder_time_base,
};

#[cfg(feature = "ndarray")]
use super::{
  Frame,
  Time,
  ffi::convert_ndarray_to_frame_rgb24,
};

type Result<T> = std::result::Result<T, Error>;

/// Encodes frames into a video stream.
/// 
/// # Example
/// 
/// ```
/// let encoder = Encoder::new(
///     &PathBuf::from("video_in.mp4"),
///     Settings::for_h264_yuv420p(800, 600, 30.0))
///   .unwrap();
/// let decoder = Decoder::new(&PathBuf::from("video_out.mkv")).unwrap();
/// decoder
///   .decode_iter()
///   .take_while(Result::is_ok)
///   .map(|frame| encoder
///     .encode(frame.unwrap())
///     .expect("Failed to encode frame."));
/// ```
pub struct Encoder {
  writer: Writer,
  writer_stream_index: usize,
  encoder: AvEncoder,
  encoder_time_base: AvRational,
  interleaved: bool,
  scaler: AvScaler,
  scaler_width: u32,
  scaler_height: u32,
  frame_count: u64,
  have_written_header: bool,
  have_finished: bool,
}

impl Encoder {
  const KEY_FRAME_INTERVAL: u64 = 12;

  /// Create a new encoder that writes to the specified file.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Locator to file to encode to.
  /// * `settings` - Encoder settings to use.
  pub fn new(
    dest: &Locator,
    settings: Settings,
  ) -> Result<Self> {
    Self::from_writer(
      Writer::new(dest)?,
      settings,
    )
  }

  /// Create a new encoder that writes to the specified file with the
  /// given output options.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Locator to file to encode to.
  /// * `settings` - Encoder settings to use.
  /// * `options` - The output options.
  pub fn new_with_options(
    dest: &Locator,
    settings: Settings,
    options: &Options,
  ) -> Result<Self> {
    Self::from_writer(
      Writer::new_with_options(dest, options)?,
      settings,
    )
  }

  /// Create a new encoder that writes to the specified file with the
  /// given format.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Locator to file to encode to.
  /// * `settings` - Encoder settings to use.
  /// * `format` - Container format to use.
  pub fn new_with_format(
    dest: &Locator,
    settings: Settings,
    format: &str,
  ) -> Result<Self> {
    Self::from_writer(
      Writer::new_with_format(dest, format)?,
      settings,
    )
  }

  /// Create a new encoder that writes to the specified file with the
  /// given format and output options.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Locator to file to encode to.
  /// * `settings` - Encoder settings to use.
  /// * `format` - Container format to use.
  /// * `options` - The output options.
  pub fn new_with_format_and_options(
    dest: &Locator,
    settings: Settings,
    format: &str,
    options: &Options,
  ) -> Result<Self> {
    Self::from_writer(
      Writer::new_with_format_and_options(dest, format, options)?,
      settings,
    )
  }

  /// Turn the encoder into an interleaved version, that automatically
  /// reorders packets when necessary.
  pub fn interleaved(mut self) -> Self {
    self.interleaved = true;
    self
  }

  /// Encode a single `ndarray` frame.
  /// 
  /// # Arguments
  /// 
  /// * `frame` - Frame to encode in `HWC` format and standard layout.
  /// * `source_timestamp` - Frame timestamp of original source. This is
  ///   necessary to make sure the output will be timed correctly.
  #[cfg(feature = "ndarray")]
  pub fn encode(
    &mut self,
    frame: &Frame,
    source_timestamp: &Time,
  ) -> Result<()> {
    let (height, width, channels) = frame.dim();
    if height != self.scaler_height as usize ||
       width != self.scaler_width as usize ||
       channels != 3 {
      return Err(Error::InvalidFrameFormat);
    }

    let mut frame = convert_ndarray_to_frame_rgb24(frame)
      .map_err(Error::BackendError)?;

    frame.set_pts(
      source_timestamp
        .aligned(self.encoder_time_base)
        .into_value());

    self.encode_raw(frame)
  }

  /// Encode a single raw frame.
  /// 
  /// # Arguments
  /// 
  /// * `frame` - Frame to encode.
  pub fn encode_raw(&mut self, frame: RawFrame) -> Result<()> {
    if frame.width() != self.scaler_width ||
       frame.height() != self.scaler_height ||
       frame.format() != FRAME_PIXEL_FORMAT {
      return Err(Error::InvalidFrameFormat);
    }

    // Write file header if we hadn't done that yet.
    if !self.have_written_header {
      let _ = self.writer.write_header()?;
      self.have_written_header = true;
    }

    // Reformat frame to target pixel format.
    let mut frame = self.scale(frame)?;
    // Producer key frame every once in a while
    if self.frame_count % Self::KEY_FRAME_INTERVAL == 0 {
      frame.set_kind(AvFrameType::I);
    }

    self
      .encoder
      .send_frame(&*frame)
      .map_err(Error::BackendError)?;

    if let Some(packet) = self.encoder_receive_packet()? {
      self.write(packet)?;
    }

    Ok(())
  }

  /// Signal to the encoder that writing has finished. This will cause any
  /// packets in the encoder to be flushed and a trailer to be written if
  /// the container format has one.
  /// 
  /// Note: If you don't call this function before dropping the encoder, it
  /// will be called automatically. This will block the caller thread. Any
  /// errors cannot be propagated in this case.
  pub fn finish(&mut self) -> Result<()> {
    if !self.have_finished {
      self.have_finished = true;
      self.flush()?;
      self.writer.write_trailer()?;
    }

    Ok(())
  }

  /// Create an encoder from a `FileWriter` instance.
  /// 
  /// # Arguments
  /// 
  /// * `writer` - `FileWriter` to create encoder from.
  /// * `settings` - Encoder settings to use.
  fn from_writer(
    mut writer: Writer,
    settings: Settings,
  ) -> Result<Self> {
    let global_header = writer
      .output
      .format()
      .flags()
      .contains(AvFormatFlags::GLOBAL_HEADER);

    let mut writer_stream = writer
      .output
      .add_stream(settings.codec())?;
    let writer_stream_index = writer_stream.index();

    let mut encoder = Self::encoder(&writer_stream)?;
    // Some formats require this flag to be set or the output will
    // not be playable by dumb players.
    if global_header {
      encoder.set_flags(AvCodecFlags::GLOBAL_HEADER);
    }

    let mut encoder = settings.apply_to(encoder);
    // Just use the ffmpeg global time base which is precise enough
    // that we should never get in trouble
    encoder.set_time_base(TIME_BASE);

    let _ = encoder
      .open_with(settings.options().to_dict())?;

    // FIXME: Not sure why we need to reinitialize the encoder every
    // time (or at least it seems like that) but the official examples
    // do the same thing.
    let encoder = Self::encoder(&writer_stream)?;
    writer_stream.set_parameters(encoder);

    let encoder = Self::encoder(&writer_stream)?;
    let encoder_time_base = get_encoder_time_base(&encoder);

    let scaler_width = encoder.width();
    let scaler_height = encoder.height();
    let scaler = AvScaler::get(
      FRAME_PIXEL_FORMAT,
      scaler_width,
      scaler_height,
      encoder.format(),
      scaler_width,
      scaler_height,
      AvScalerFlags::empty())?;

    Ok(Self {
      writer,
      writer_stream_index,
      encoder,
      encoder_time_base,
      interleaved: false,
      scaler,
      scaler_width,
      scaler_height,
      frame_count: 0,
      have_written_header: false,
      have_finished: false,
    })
  }

  /// Apply scaling (or pixel reformatting in this case) on the frame with the
  /// scaler we initialized earlier.
  /// 
  /// # Arguments
  /// 
  /// * `frame` - Frame to rescale.
  fn scale(&mut self, frame: RawFrame) -> Result<RawFrame> {
    let mut frame_scaled = RawFrame::empty();
    self
      .scaler
      .run(&frame, &mut frame_scaled)
      .map_err(Error::BackendError)?;
    // Copy over PTS from old frame.
    frame_scaled.set_pts(frame.pts());

    Ok(frame_scaled)
  }

  /// Pull an encoded packet from the decoder. This function also handles
  /// the possible `EAGAIN` result, in which case we just need to go
  /// again.
  fn encoder_receive_packet(&mut self) -> Result<Option<AvPacket>> {
    let mut packet = AvPacket::empty();
    let encode_result = self.encoder.receive_packet(&mut packet);
    match encode_result {
      Ok(())
        => Ok(Some(packet)),
      Err(AvError::Other { errno }) if errno == EAGAIN
        => Ok(None),
      Err(err)
        => Err(err.into()),
    }
  }

  /// Helper function to extract encoder from stream.
  /// 
  /// # Arguments
  /// 
  /// * `writer_stream` - Stream to get encoder of.
  /// 
  /// # Returns
  /// 
  /// Raw ffmpeg encoder belonging to given stream.
  fn encoder(writer_stream: &StreamMut) -> Result<AvEncoder> {
    AvContext::from_parameters(writer_stream.parameters())?
      .encoder()
      .video()
      .map_err(Error::BackendError)
  }

  /// Acquire the time base of the output stream.
  fn stream_time_base(&mut self) -> AvRational {
    self
      .writer
      .output
      .stream(self.writer_stream_index)
      .unwrap()
      .time_base()
  }

  /// Write encoded packet to output stream.
  /// 
  /// # Arguments
  /// 
  /// * `packet` - Encoded packet.
  fn write(&mut self, mut packet: AvPacket) -> Result<()> {
    packet.set_stream(self.writer_stream_index);
    packet.set_position(-1);
    packet.rescale_ts(self.encoder_time_base, self.stream_time_base());
    let _ =
      if self.interleaved {
        self.writer.write_interleaved(&mut packet)?;
      } else {
        self.writer.write(&mut packet)?;
      };
    
    self.frame_count += 1;
    Ok(())
  }

  /// Flush the encoder, drain any packets that still need processing.
  fn flush(&mut self) -> Result<()> {
    // Maximum number of invocations to `encoder_receive_packet`
    // to drain the items still on the queue before giving up.
    const MAX_DRAIN_ITERATIONS: u32 = 100;

    // Notify the encoder that the last frame has been sent.
    self.encoder.send_eof()?;

    // We need to drain the items still in the encoders queue.
    for _ in 0..MAX_DRAIN_ITERATIONS {
      match self.encoder_receive_packet() {
        Ok(Some(packet))
          => self.write(packet)?,
        Ok(None)
          => continue,
        Err(_)
          => break,
      }
    }

    Ok(())
  }

}

impl Drop for Encoder {

  fn drop(&mut self) {
    let _ = self.finish();
  }

}

/// Holds a logical combination of encoder settings.
pub struct Settings<'o> {
  width: u32,
  height: u32,
  pixel_format: AvPixel,
  options: Options<'o>,
}

impl<'o> Settings<'o> {
  /// This is the assumed FPS for the encoder to use. Note that this does not
  /// need to be correct exactly.
  const FRAME_RATE: i32 = 30;

  /// Create encoder settings for an H264 stream with YUV420p pixel format.
  /// This will encode to arguably the most widely compatible video file since
  /// H264 is a common codec and YUV420p is the most commonly used pixel format.
  pub fn for_h264_yuv420p(
    width: usize,
    height: usize,
    realtime: bool,
  ) -> Settings<'o> {
    let options = if realtime {
      Options::new_h264_realtime()
    } else {
      Options::new_h264()
    };

    Self {
      width: width as u32,
      height: height as u32,
      pixel_format: AvPixel::YUV420P,
      options,
    }
  }

  /// Apply the settings to an encoder.
  /// 
  /// # Arguments
  /// 
  /// * `encoder` - Encoder to apply settings to.
  /// 
  /// # Returns
  /// 
  /// New encoder with settings applied.
  fn apply_to(&self, mut encoder: AvEncoder) -> AvEncoder {
    encoder.set_width(self.width);
    encoder.set_height(self.height);
    encoder.set_format(self.pixel_format);
    encoder.set_frame_rate(Some((Self::FRAME_RATE, 1)));
    encoder
  }

  /// Get codec.
  fn codec(&self) -> Option<AvCodec> {
    // Try to use the libx264 decoder. If it is not available, then use
    // use whatever default h264 decoder we have.
    Some(ffmpeg::encoder::find_by_name("libx264")
      .unwrap_or(ffmpeg::encoder::find(AvCodecId::H264)?))
  }

  /// Get encoder options.
  fn options(&self) -> &Options<'o> {
    &self.options
  }

}