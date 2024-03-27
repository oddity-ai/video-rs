extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::decoder::Video as AvDecoder;
use ffmpeg::codec::Context as AvContext;
use ffmpeg::format::pixel::Pixel as AvPixel;
use ffmpeg::software::scaling::{context::Context as AvScaler, flag::Flags as AvScalerFlags};
use ffmpeg::util::error::EAGAIN;
use ffmpeg::{Error as AvError, Rational as AvRational};

use crate::error::Error;
use crate::ffi;
use crate::ffi_hwaccel;
#[cfg(feature = "ndarray")]
use crate::frame::Frame;
use crate::frame::{RawFrame, FRAME_PIXEL_FORMAT};
use crate::hwaccel::{HardwareAccelerationContext, HardwareAccelerationDeviceType};
use crate::io::{Reader, ReaderBuilder};
use crate::location::Location;
use crate::options::Options;
use crate::packet::Packet;
use crate::resize::Resize;
use crate::time::Time;

type Result<T> = std::result::Result<T, Error>;

/// Always use NV12 pixel format with hardware acceleration, then rescale later.
static HWACCEL_PIXEL_FORMAT: AvPixel = AvPixel::NV12;

/// Builds a [`Decoder`].
pub struct DecoderBuilder<'a> {
    source: Location,
    options: Option<&'a Options>,
    resize: Option<Resize>,
    hardware_acceleration_device_type: Option<HardwareAccelerationDeviceType>,
}

impl<'a> DecoderBuilder<'a> {
    /// Create a decoder with the specified source.
    ///
    /// * `source` - Source to decode.
    pub fn new(source: impl Into<Location>) -> Self {
        Self {
            source: source.into(),
            options: None,
            resize: None,
            hardware_acceleration_device_type: None,
        }
    }

    /// Set custom options. Options are applied to the input.
    ///
    /// * `options` - Custom options.
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Set resizing to apply to frames.
    ///
    /// * `resize` - Resizing to apply.
    pub fn with_resize(mut self, resize: Resize) -> Self {
        self.resize = Some(resize);
        self
    }

    /// Enable hardware acceleration with the specified device type.
    ///
    /// * `device_type` - Device to use for hardware acceleration.
    pub fn with_hardware_acceleration(
        mut self,
        device_type: HardwareAccelerationDeviceType,
    ) -> Self {
        self.hardware_acceleration_device_type = Some(device_type);
        self
    }

    /// Build [`Decoder`].
    pub fn build(self) -> Result<Decoder> {
        let mut reader_builder = ReaderBuilder::new(self.source);
        if let Some(options) = self.options {
            reader_builder = reader_builder.with_options(options);
        }
        let reader = reader_builder.build()?;
        let reader_stream_index = reader.best_video_stream_index()?;
        Ok(Decoder {
            decoder: DecoderSplit::new(
                &reader,
                reader_stream_index,
                self.resize,
                self.hardware_acceleration_device_type,
            )?,
            reader,
            reader_stream_index,
        })
    }
}

/// Decode video files and streams.
///
/// # Example
///
/// ```ignore
/// let decoder = Decoder::new(Path::new("video.mp4")).unwrap();
/// decoder
///     .decode_iter()
///     .take_while(Result::is_ok)
///     .for_each(|frame| println!("Got frame!"),
/// );
/// ```
pub struct Decoder {
    decoder: DecoderSplit,
    reader: Reader,
    reader_stream_index: usize,
}

impl Decoder {
    /// Create a decoder to decode the specified source.
    ///
    /// # Arguments
    ///
    /// * `source` - Source to decode.
    #[inline]
    pub fn new(source: impl Into<Location>) -> Result<Self> {
        DecoderBuilder::new(source).build()
    }

    /// Get decoder time base.
    #[inline]
    pub fn time_base(&self) -> AvRational {
        self.decoder.time_base()
    }

    /// Duration of the decoder stream.
    #[inline]
    pub fn duration(&self) -> Result<Time> {
        let reader_stream = self
            .reader
            .input
            .stream(self.reader_stream_index)
            .ok_or(AvError::StreamNotFound)?;
        Ok(Time::new(
            Some(reader_stream.duration()),
            reader_stream.time_base(),
        ))
    }

    /// Number of frames in the decoder stream.
    #[inline]
    pub fn frames(&self) -> Result<u64> {
        Ok(self
            .reader
            .input
            .stream(self.reader_stream_index)
            .ok_or(AvError::StreamNotFound)?
            .frames()
            .max(0) as u64)
    }

    /// Decode frames through iterator interface. This is similar to `decode` but it returns frames
    /// through an infinite iterator.
    ///
    /// # Example
    ///
    /// ```ignore
    /// decoder
    ///     .decode_iter()
    ///     .take_while(Result::is_ok)
    ///     .map(Result::unwrap)
    ///     .for_each(|(ts, frame)| {
    ///         // Do something with frame...
    ///     });
    /// ```
    #[cfg(feature = "ndarray")]
    pub fn decode_iter(&mut self) -> impl Iterator<Item = Result<(Time, Frame)>> + '_ {
        std::iter::from_fn(move || Some(self.decode()))
    }

    /// Decode a single frame.
    ///
    /// # Return value
    ///
    /// A tuple of the frame timestamp (relative to the stream) and the frame itself.
    ///
    /// # Example
    ///
    /// ```ignore
    /// loop {
    ///     let (ts, frame) = decoder.decode()?;
    ///     // Do something with frame...
    /// }
    /// ```
    #[cfg(feature = "ndarray")]
    pub fn decode(&mut self) -> Result<(Time, Frame)> {
        Ok(loop {
            let packet = self.reader.read(self.reader_stream_index)?;
            if let Some(frame) = self.decoder.decode(packet)? {
                break frame;
            }
        })
    }

    /// Decode frames through iterator interface. This is similar to `decode_raw` but it returns
    /// frames through an infinite iterator.
    pub fn decode_raw_iter(&mut self) -> impl Iterator<Item = Result<RawFrame>> + '_ {
        std::iter::from_fn(move || Some(self.decode_raw()))
    }

    /// Decode a single frame and return the raw ffmpeg `AvFrame`.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`].
    pub fn decode_raw(&mut self) -> Result<RawFrame> {
        Ok(loop {
            let packet = self.reader.read(self.reader_stream_index)?;
            if let Some(frame) = self.decoder.decode_raw(packet)? {
                break frame;
            }
        })
    }

    /// Split the decoder into a decoder (of type [`DecoderSplit`]) and a [`Reader`].
    ///
    /// This allows the caller to detach stream reading from decoding, which is useful for advanced
    /// use cases.
    ///
    /// # Return value
    ///
    /// Tuple of the [`DecoderSplit`], [`Reader`] and the reader stream index.
    #[inline]
    pub fn into_parts(self) -> (DecoderSplit, Reader, usize) {
        (self.decoder, self.reader, self.reader_stream_index)
    }

    /// Get the decoders input size (resolution dimensions): width and height.
    #[inline(always)]
    pub fn size(&self) -> (u32, u32) {
        self.decoder.size
    }

    /// Get the decoders output size after resizing is applied (resolution dimensions): width and
    /// height.
    #[inline(always)]
    pub fn size_out(&self) -> (u32, u32) {
        self.decoder.size_out
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
}

/// Decoder part of a split [`Decoder`] and [`Reader`].
pub struct DecoderSplit {
    decoder: AvDecoder,
    decoder_time_base: AvRational,
    hwaccel_context: Option<HardwareAccelerationContext>,
    scaler: Option<AvScaler>,
    size: (u32, u32),
    size_out: (u32, u32),
}

impl DecoderSplit {
    /// Create a new [`DecoderSplit`].
    ///
    /// # Arguments
    ///
    /// * `reader` - [`Reader`] to initialize decoder from.
    /// * `resize` - Optional resize strategy to apply to frames.
    pub fn new(
        reader: &Reader,
        reader_stream_index: usize,
        resize: Option<Resize>,
        hwaccel_device_type: Option<HardwareAccelerationDeviceType>,
    ) -> Result<Self> {
        let reader_stream = reader
            .input
            .stream(reader_stream_index)
            .ok_or(AvError::StreamNotFound)?;

        let mut decoder = AvContext::new();
        ffi::set_decoder_context_time_base(&mut decoder, reader_stream.time_base());
        decoder.set_parameters(reader_stream.parameters())?;

        let hwaccel_context = match hwaccel_device_type {
            Some(device_type) => Some(HardwareAccelerationContext::new(&mut decoder, device_type)?),
            None => None,
        };

        let decoder = decoder.decoder().video()?;
        let decoder_time_base = decoder.time_base();

        if decoder.format() == AvPixel::None || decoder.width() == 0 || decoder.height() == 0 {
            return Err(Error::MissingCodecParameters);
        }

        let (resize_width, resize_height) = match resize {
            Some(resize) => resize
                .compute_for((decoder.width(), decoder.height()))
                .ok_or(Error::InvalidResizeParameters)?,
            None => (decoder.width(), decoder.height()),
        };

        let scaler_input_format = if hwaccel_context.is_some() {
            HWACCEL_PIXEL_FORMAT
        } else {
            decoder.format()
        };

        let is_scaler_needed = !(scaler_input_format == FRAME_PIXEL_FORMAT
            && decoder.width() == resize_width
            && decoder.height() == resize_height);

        let scaler = if is_scaler_needed {
            Some(AvScaler::get(
                scaler_input_format,
                decoder.width(),
                decoder.height(),
                FRAME_PIXEL_FORMAT,
                resize_width,
                resize_height,
                AvScalerFlags::AREA,
            )?)
        } else {
            None
        };

        let size = (decoder.width(), decoder.height());
        let size_out = (resize_width, resize_height);

        Ok(Self {
            decoder,
            decoder_time_base,
            hwaccel_context,
            scaler,
            size,
            size_out,
        })
    }

    /// Get decoder time base.
    #[inline]
    pub fn time_base(&self) -> AvRational {
        self.decoder_time_base
    }

    /// Decode a [`Packet`].
    ///
    /// Feeds the packet to the decoder and returns a frame if there is one available. The caller
    /// should keep feeding packets until the decoder returns a frame.
    ///
    /// # Return value
    ///
    /// A tuple of the [`Frame`] and timestamp (relative to the stream) and the frame itself if the
    /// decoder has a frame available, [`None`] if not.
    #[cfg(feature = "ndarray")]
    pub fn decode(&mut self, packet: Packet) -> Result<Option<(Time, Frame)>> {
        match self.decode_raw(packet)? {
            Some(mut frame) => {
                // We use the packet DTS here (which is `frame->pkt_dts`) because that is what the
                // encoder will use when encoding for the `PTS` field.
                let timestamp = Time::new(Some(frame.packet().dts), self.decoder_time_base);
                let frame =
                    ffi::convert_frame_to_ndarray_rgb24(&mut frame).map_err(Error::BackendError)?;

                Ok(Some((timestamp, frame)))
            }
            None => Ok(None),
        }
    }

    /// Decode a [`Packet`].
    ///
    /// Feeds the packet to the decoder and returns a frame if there is one available. The caller
    /// should keep feeding packets until the decoder returns a frame.
    ///
    /// # Return value
    ///
    /// The decoded raw frame as [`RawFrame`] if the decoder has a frame available, [`None`] if not.
    pub fn decode_raw(&mut self, packet: Packet) -> Result<Option<RawFrame>> {
        let (mut packet, packet_time_base) = packet.into_inner_parts();
        packet.rescale_ts(packet_time_base, self.decoder_time_base);

        self.decoder
            .send_packet(&packet)
            .map_err(Error::BackendError)?;

        match self.decoder_receive_frame()? {
            Some(frame) => {
                let frame = match self.hwaccel_context.as_ref() {
                    Some(hwaccel_context) if hwaccel_context.format() == frame.format() => {
                        let mut frame_downloaded = RawFrame::empty();
                        frame_downloaded.set_format(HWACCEL_PIXEL_FORMAT);
                        ffi_hwaccel::hwdevice_transfer_frame(&mut frame_downloaded, &frame)?;
                        ffi::copy_frame_props(&frame, &mut frame_downloaded);
                        frame_downloaded
                    }
                    _ => frame,
                };

                let frame = match self.scaler.as_mut() {
                    Some(scaler) => {
                        let mut frame_scaled = RawFrame::empty();
                        scaler
                            .run(&frame, &mut frame_scaled)
                            .map_err(Error::BackendError)?;
                        ffi::copy_frame_props(&frame, &mut frame_scaled);
                        frame_scaled
                    }
                    _ => frame,
                };

                Ok(Some(frame))
            }
            None => Ok(None),
        }
    }

    /// Get the decoders input size (resolution dimensions): width and height.
    #[inline(always)]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Get the decoders output size after resizing is applied (resolution dimensions): width and
    /// height.
    #[inline(always)]
    pub fn size_out(&self) -> (u32, u32) {
        self.size_out
    }

    /// Pull a decoded frame from the decoder. This function also implements retry mechanism in case
    /// the decoder signals `EAGAIN`.
    fn decoder_receive_frame(&mut self) -> Result<Option<RawFrame>> {
        let mut frame = RawFrame::empty();
        let decode_result = self.decoder.receive_frame(&mut frame);
        match decode_result {
            Ok(()) => Ok(Some(frame)),
            Err(AvError::Other { errno }) if errno == EAGAIN => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

impl Drop for DecoderSplit {
    fn drop(&mut self) {
        // Maximum number of invocations to `decoder_receive_frame` to drain the items still on the
        // queue before giving up.
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

unsafe impl Send for DecoderSplit {}
unsafe impl Sync for DecoderSplit {}
