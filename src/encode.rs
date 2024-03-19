extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::codec::Codec as AvCodec;
use ffmpeg::codec::encoder::video::Encoder as AvEncoder;
use ffmpeg::codec::encoder::video::Video as AvVideo;
use ffmpeg::codec::flag::Flags as AvCodecFlags;
use ffmpeg::codec::packet::Packet as AvPacket;
use ffmpeg::codec::{Context as AvContext, Id as AvCodecId};
use ffmpeg::format::flag::Flags as AvFormatFlags;
use ffmpeg::software::scaling::context::Context as AvScaler;
use ffmpeg::software::scaling::flag::Flags as AvScalerFlags;
use ffmpeg::util::error::EAGAIN;
use ffmpeg::util::format::Pixel as AvPixel;
use ffmpeg::util::mathematics::rescale::TIME_BASE;
use ffmpeg::util::picture::Type as AvFrameType;
use ffmpeg::Error as AvError;
use ffmpeg::Rational as AvRational;

use crate::error::Error;
use crate::ffi;
#[cfg(feature = "ndarray")]
use crate::frame::Frame;
use crate::frame::{PixelFormat, RawFrame, FRAME_PIXEL_FORMAT};
use crate::io::private::Write;
use crate::io::{Writer, WriterBuilder};
use crate::location::Location;
use crate::options::Options;
#[cfg(feature = "ndarray")]
use crate::time::Time;

type Result<T> = std::result::Result<T, Error>;

/// Builds an [`Encoder`].
pub struct EncoderBuilder<'a> {
    destination: Location,
    settings: Settings,
    options: Option<&'a Options>,
    format: Option<&'a str>,
    interleaved: bool,
}

impl<'a> EncoderBuilder<'a> {
    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `settings` - Encoding settings.
    pub fn new(destination: impl Into<Location>, settings: Settings) -> Self {
        Self {
            destination: destination.into(),
            settings,
            options: None,
            format: None,
            interleaved: false,
        }
    }

    /// Set the output options for the encoder.
    ///
    /// # Arguments
    ///
    /// * `options` - The output options.
    pub fn with_options(mut self, options: &'a Options) -> Self {
        self.options = Some(options);
        self
    }

    /// Set the container format for the encoder.
    ///
    /// # Arguments
    ///
    /// * `format` - Container format to use.
    pub fn with_format(mut self, format: &'a str) -> Self {
        self.format = Some(format);
        self
    }

    /// Set interleaved. This will cause the encoder to use interleaved write instead of normal
    /// write.
    pub fn interleaved(mut self) -> Self {
        self.interleaved = true;
        self
    }

    /// Build an [`Encoder`].
    pub fn build(self) -> Result<Encoder> {
        let mut writer_builder = WriterBuilder::new(self.destination);
        if let Some(options) = self.options {
            writer_builder = writer_builder.with_options(options);
        }
        if let Some(format) = self.format {
            writer_builder = writer_builder.with_format(format);
        }
        Encoder::from_writer(writer_builder.build()?, self.interleaved, self.settings)
    }
}

/// Encodes frames into a video stream.
///
/// # Example
///
/// ```ignore
/// let encoder = Encoder::new(
///     Path::new("video_in.mp4"),
///     Settings::for_h264_yuv420p(800, 600, 30.0)
/// )
/// .unwrap();
///
/// let decoder = Decoder::new(Path::new("video_out.mkv")).unwrap();
/// decoder
///     .decode_iter()
///     .take_while(Result::is_ok)
///     .map(|frame| encoder
///         .encode(frame.unwrap())
///         .expect("Failed to encode frame."),
///     );
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
    have_written_trailer: bool,
}

impl Encoder {
    const KEY_FRAME_INTERVAL: u64 = 12;

    /// Create an encoder with the specified destination and settings.
    ///
    /// * `destination` - Where to encode to.
    /// * `settings` - Encoding settings.
    #[inline]
    pub fn new(destination: impl Into<Location>, settings: Settings) -> Result<Self> {
        EncoderBuilder::new(destination, settings).build()
    }

    /// Encode a single `ndarray` frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode in `HWC` format and standard layout.
    /// * `source_timestamp` - Frame timestamp of original source. This is necessary to make sure
    ///   the output will be timed correctly.
    #[cfg(feature = "ndarray")]
    pub fn encode(&mut self, frame: &Frame, source_timestamp: &Time) -> Result<()> {
        let (height, width, channels) = frame.dim();
        if height != self.scaler_height as usize
            || width != self.scaler_width as usize
            || channels != 3
        {
            return Err(Error::InvalidFrameFormat);
        }

        let mut frame = ffi::convert_ndarray_to_frame_rgb24(frame).map_err(Error::BackendError)?;

        frame.set_pts(
            source_timestamp
                .aligned_with_rational(self.encoder_time_base)
                .into_value(),
        );

        self.encode_raw(frame)
    }

    /// Encode a single raw frame.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to encode.
    pub fn encode_raw(&mut self, frame: RawFrame) -> Result<()> {
        if frame.width() != self.scaler_width
            || frame.height() != self.scaler_height
            || frame.format() != FRAME_PIXEL_FORMAT
        {
            return Err(Error::InvalidFrameFormat);
        }

        // Write file header if we hadn't done that yet.
        if !self.have_written_header {
            self.writer.write_header()?;
            self.have_written_header = true;
        }

        // Reformat frame to target pixel format.
        let mut frame = self.scale(frame)?;
        // Producer key frame every once in a while
        if self.frame_count % Self::KEY_FRAME_INTERVAL == 0 {
            frame.set_kind(AvFrameType::I);
        }

        self.encoder
            .send_frame(&frame)
            .map_err(Error::BackendError)?;

        if let Some(packet) = self.encoder_receive_packet()? {
            self.write(packet)?;
        }

        Ok(())
    }

    /// Signal to the encoder that writing has finished. This will cause any packets in the encoder
    /// to be flushed and a trailer to be written if the container format has one.
    ///
    /// Note: If you don't call this function before dropping the encoder, it will be called
    /// automatically. This will block the caller thread. Any errors cannot be propagated in this
    /// case.
    pub fn finish(&mut self) -> Result<()> {
        if self.have_written_header && !self.have_written_trailer {
            self.have_written_trailer = true;
            self.flush()?;
            self.writer.write_trailer()?;
        }

        Ok(())
    }

    /// Get encoder time base.
    #[inline]
    pub fn time_base(&self) -> AvRational {
        self.encoder_time_base
    }

    /// Create an encoder from a `FileWriter` instance.
    ///
    /// # Arguments
    ///
    /// * `writer` - [`Writer`] to create encoder from.
    /// * `interleaved` - Whether or not to use interleaved write.
    /// * `settings` - Encoder settings to use.
    fn from_writer(mut writer: Writer, interleaved: bool, settings: Settings) -> Result<Self> {
        let global_header = writer
            .output
            .format()
            .flags()
            .contains(AvFormatFlags::GLOBAL_HEADER);

        let mut writer_stream = writer.output.add_stream(settings.codec())?;
        let writer_stream_index = writer_stream.index();

        let mut encoder_context = match settings.codec() {
            Some(codec) => ffi::codec_context_as(&codec)?,
            None => AvContext::new(),
        };

        // Some formats require this flag to be set or the output will
        // not be playable by dumb players.
        if global_header {
            encoder_context.set_flags(AvCodecFlags::GLOBAL_HEADER);
        }

        let mut encoder = encoder_context.encoder().video()?;
        settings.apply_to(&mut encoder);

        // Just use the ffmpeg global time base which is precise enough
        // that we should never get in trouble.
        encoder.set_time_base(TIME_BASE);

        let encoder = encoder.open_with(settings.options().to_dict())?;
        let encoder_time_base = ffi::get_encoder_time_base(&encoder);

        writer_stream.set_parameters(&encoder);

        let scaler_width = encoder.width();
        let scaler_height = encoder.height();
        let scaler = AvScaler::get(
            FRAME_PIXEL_FORMAT,
            scaler_width,
            scaler_height,
            encoder.format(),
            scaler_width,
            scaler_height,
            AvScalerFlags::empty(),
        )?;

        Ok(Self {
            writer,
            writer_stream_index,
            encoder,
            encoder_time_base,
            interleaved,
            scaler,
            scaler_width,
            scaler_height,
            frame_count: 0,
            have_written_header: false,
            have_written_trailer: false,
        })
    }

    /// Apply scaling (or pixel reformatting in this case) on the frame with the scaler we
    /// initialized earlier.
    ///
    /// # Arguments
    ///
    /// * `frame` - Frame to rescale.
    fn scale(&mut self, frame: RawFrame) -> Result<RawFrame> {
        let mut frame_scaled = RawFrame::empty();
        self.scaler
            .run(&frame, &mut frame_scaled)
            .map_err(Error::BackendError)?;
        // Copy over PTS from old frame.
        frame_scaled.set_pts(frame.pts());

        Ok(frame_scaled)
    }

    /// Pull an encoded packet from the decoder. This function also handles the possible `EAGAIN`
    /// result, in which case we just need to go again.
    fn encoder_receive_packet(&mut self) -> Result<Option<AvPacket>> {
        let mut packet = AvPacket::empty();
        let encode_result = self.encoder.receive_packet(&mut packet);
        match encode_result {
            Ok(()) => Ok(Some(packet)),
            Err(AvError::Other { errno }) if errno == EAGAIN => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    /// Acquire the time base of the output stream.
    fn stream_time_base(&mut self) -> AvRational {
        self.writer
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
                Ok(Some(packet)) => self.write(packet)?,
                Ok(None) => continue,
                Err(_) => break,
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
#[derive(Debug, Clone)]
pub struct Settings {
    width: u32,
    height: u32,
    pixel_format: AvPixel,
    options: Options,
}

impl Settings {
    /// This is the assumed FPS for the encoder to use. Note that this does not need to be correct
    /// exactly.
    const FRAME_RATE: i32 = 30;

    /// Create encoder settings for an H264 stream with YUV420p pixel format. This will encode to
    /// arguably the most widely compatible video file since H264 is a common codec and YUV420p is
    /// the most commonly used pixel format.
    pub fn preset_h264_yuv420p(width: usize, height: usize, realtime: bool) -> Settings {
        let options = if realtime {
            Options::preset_h264_realtime()
        } else {
            Options::preset_h264()
        };

        Self {
            width: width as u32,
            height: height as u32,
            pixel_format: AvPixel::YUV420P,
            options,
        }
    }

    /// Create encoder settings for an H264 stream with a custom pixel format and options.
    /// This allows for greater flexibility in encoding settings, enabling specific requirements
    /// or optimizations to be set depending on the use case.
    ///
    /// # Arguments
    ///
    /// * `width` - The width of the video stream.
    /// * `height` - The height of the video stream.
    /// * `pixel_format` - The desired pixel format for the video stream.
    /// * `options` - Custom H264 encoding options.
    ///
    /// # Return value
    ///
    /// A `Settings` instance with the specified configuration.+
    pub fn preset_h264_custom(
        width: usize,
        height: usize,
        pixel_format: PixelFormat,
        options: Options,
    ) -> Settings {
        Self {
            width: width as u32,
            height: height as u32,
            pixel_format,
            options,
        }
    }

    /// Apply the settings to an encoder.
    ///
    /// # Arguments
    ///
    /// * `encoder` - Encoder to apply settings to.
    ///
    /// # Return value
    ///
    /// New encoder with settings applied.
    fn apply_to(&self, encoder: &mut AvVideo) {
        encoder.set_width(self.width);
        encoder.set_height(self.height);
        encoder.set_format(self.pixel_format);
        encoder.set_frame_rate(Some((Self::FRAME_RATE, 1)));
    }

    /// Get codec.
    fn codec(&self) -> Option<AvCodec> {
        // Try to use the libx264 decoder. If it is not available, then use use whatever default
        // h264 decoder we have.
        Some(
            ffmpeg::encoder::find_by_name("libx264")
                .unwrap_or(ffmpeg::encoder::find(AvCodecId::H264)?),
        )
    }

    /// Get encoder options.
    fn options(&self) -> &Options {
        &self.options
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
