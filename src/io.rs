extern crate ffmpeg_next as ffmpeg;

use std::path::{PathBuf, Path};
use std::mem;

use ffmpeg::{
  ffi::AV_TIME_BASE_Q,
  format::context::Input as AvInput,
  format::context::Output as AvOutput,
  codec::packet::Packet as AvPacket,
  media::Type as AvMediaType,
  Error as AvError,
};

use super::StreamInfo;
use super::Packet;
use super::Error;
use super::options::Options;
use super::ffi;

type Result<T> = std::result::Result<T, Error>;

/// Re-export `url::Url` since it is an input type for callers of the API.
pub use url::Url;

/// Video reader that can read from files.
pub struct Reader {
  pub source: Locator,
  pub input: AvInput,
}

impl Reader {

  /// Create a new video file reader on a given source (path, URL, etc.).
  /// 
  /// # Arguments
  /// 
  /// * `source` - Source to read from.
  pub fn new(source: &Locator) -> Result<Self> {
    let input = ffmpeg::format::input(&source.resolve())?;

    Ok(Self {
      source: source.clone(),
      input,
    })
  }

  /// Create a new video file reader with options for the backend.
  /// 
  /// # Arguments
  /// 
  /// * `source` - Source to read from.
  /// * `options` - Options to pass on.
  /// 
  /// # Examples
  /// 
  /// ```
  /// let mut options = HashMap::new();
  /// options.insert(
  ///   "rtsp_transport".to_string(),
  ///   "tcp".to_string());
  /// 
  /// let mut reader = Reader::new(
  ///     &PathBuf::from("my_file.mp4").into(), 
  ///     &options.into())
  ///   .unwrap();
  /// ```
  pub fn new_with_options(source: &Locator, options: &Options) -> Result<Self> {
    let input = ffmpeg::format::input_with_dictionary(
      &source.resolve(),
      options.to_dict())?;

    Ok(Self {
      source: source.clone(),
      input,
    })
  }

  /// Read a single packet from the source video file.
  /// 
  /// # Arguments
  /// 
  /// * `stream_index` - Index of stream to read from.
  /// 
  /// # Examples
  /// 
  /// Read a single packet.
  /// 
  /// ```
  /// let mut reader = Reader(&PathBuf::from("my_video.mp4").into()).unwrap();
  /// let stream = reader.best_video_stream_index().unwrap();
  /// let mut packet = reader.read(stream).unwrap();
  /// ```
  pub fn read(&mut self, stream_index: usize) -> Result<Packet> {
    let mut error_count = 0;
    loop {
      match self.input.packets().next() {
        Some((stream, packet)) => {
          if stream.index() == stream_index {
            return Ok(Packet::new(
              packet,
              stream.time_base(),
            ))
          }
        },
        None => {
          error_count += 1;
          if error_count > 3 {
            return Err(Error::ReadExhausted)
          }
        }
      }
    }
  }

  /// Retrieve stream information for a stream. Stream information can be
  /// used to set up a corresponding stream for transmuxing or transcoding.
  /// 
  /// # Arguments
  /// 
  /// * `stream_index` - Index of stream to produce information for.
  pub fn stream_info(
    &self,
    stream_index: usize,
  ) -> Result<StreamInfo> {
    StreamInfo::from_reader(
      &self,
      stream_index,
    )
  }

  /// Seek in reader. This will change the reader head so that it points to
  /// a location within one second of the target timestamp or it will return
  /// an error.
  /// 
  /// # Arguments
  /// 
  /// * `timestamp_milliseconds` - Number of millisecond from start of video
  ///   to seek to.
  pub fn seek(&mut self, timestamp_milliseconds: i64) -> Result<()> {
    // Conversion factor from timestamp in milliseconds to `TIME_BASE` units.
    const CONVERSION_FACTOR: i64 = (AV_TIME_BASE_Q.den / 1000) as i64;
    // One second left and right leeway when seeking.
    const LEEWAY: i64 = AV_TIME_BASE_Q.den as i64;

    let timestamp = CONVERSION_FACTOR * timestamp_milliseconds;
    let range = timestamp - LEEWAY..timestamp + LEEWAY;

    self
      .input
      .seek(timestamp, range)
      .map_err(Error::BackendError)
  }

  /// Seek to start of reader. This function performs best effort seeking to
  /// the start of the file.
  pub fn seek_to_start(&mut self) -> Result<()> {
    self
      .input
      .seek(i64::min_value(), ..)
      .map_err(Error::BackendError)
  }

  /// Find the best video stream and return the index.
  pub fn best_video_stream_index(&self) -> Result<usize> {
    Ok(self.input
      .streams()
      .best(AvMediaType::Video)
      .ok_or(AvError::StreamNotFound)?
      .index())
  }

}

/// Any type that implements this can write video packets.
pub trait Write:
  private::Write + private::Output {}

/// File writer for video files.
pub struct Writer {
  pub dest: Locator,
  pub(crate) output: AvOutput,
}

impl Writer {

  /// Create a new file writer for video files.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Where to write to.
  pub fn new(dest: &Locator) -> Result<Self> {
    let output = ffmpeg::format::output(&dest.resolve())?;

    Ok(Self {
      dest: dest.clone(),
      output,
    })
  }

  /// Create a new file writer for video files with a custom format
  /// specifier.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Where to write to.
  /// * `format` - Container format to use.
  pub fn new_with_format(dest: &Locator, format: &str) -> Result<Self> {
    let output = ffmpeg::format::output_as(
      &dest.resolve(),
      format)?;

    Ok(Self {
      dest: dest.clone(),
      output,
    })
  }

  /// Create a new file writer for video files with custom options
  /// for the ffmpeg backend.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Where to write to.
  /// * `options` - Options to pass on.
  /// 
  /// # Examples
  /// 
  /// Create a video writer that produces fragmented MP4.
  /// 
  /// ```
  /// let mut options = HashMap::new();
  /// options.insert(
  ///   "movflags".to_string(),
  ///   "frag_keyframe+empty_moov".to_string());
  /// 
  /// let mut writer = FileWriter::new(
  ///     &PathBuf::from("my_file.mp4").into(), 
  ///     &options.into())
  ///   .unwrap();
  /// ```
  pub fn new_with_options(dest: &Locator, options: &Options) -> Result<Self> {
    let output = ffmpeg::format::output_with(
      &dest.resolve(),
      options.to_dict())?;

    Ok(Self {
      dest: dest.clone(),
      output,
    })
  }

  /// Create a new file writer for video files with a custom format
  /// specifier and custom options for the ffmpeg backend.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Where to write to.
  /// * `format` - Container format to use.
  /// * `options` - Options to pass on.
  pub fn new_with_format_and_options(
    dest: &Locator,
    format: &str,
    options: &Options,
  ) -> Result<Self> {
    let output = ffmpeg::format::output_as_with(
      &dest.resolve(),
      format,
      options.to_dict())?;

    Ok(Self {
      dest: dest.clone(),
      output,
    })
  }

}

impl Write for Writer {}

/// Type alias for a byte buffer.
pub type Buf = Vec<u8>;

/// Type alias for multiple buffers.
pub type Bufs = Vec<Buf>;

/// Video writer that writes to a buffer.
pub struct BufWriter {
  pub(crate) output: AvOutput,
  options: Options<'static>,
}

impl BufWriter {

  /// Create a video writer that writes to a buffer and returns
  /// the resulting bytes.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Container format to use.
  /// 
  /// # Examples
  /// 
  /// ```
  /// let mut writer = BufWriter::new("mp4").unwrap();
  /// let bytes = writer.write_header()?;
  /// ```
  pub fn new(format: &str) -> Result<Self> {
    Self::new_with(format, Default::default())
  }

  /// Create a video writer that writes to a buffer and returns
  /// the resulting bytes. This constructor also allows for passing
  /// options for the ffmpeg backend.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Container format to use.
  /// * `options` - Options to pass on to ffmpeg.
  pub fn new_with(format: &str, options: Options<'static>) -> Result<Self> {
    let output = ffi::output_raw(format)?;
    
    Ok(Self {
      output,
      options,
    })
  }

  fn begin_write(&mut self) {
    ffi::output_raw_buf_start(&mut self.output);
  }

  fn end_write(&mut self) -> Buf {
    ffi::output_raw_buf_end(&mut self.output)
  }

}

impl Write for BufWriter {}

impl Drop for BufWriter {

  fn drop(&mut self) {
    // Make sure to close the buffer properly before dropping the
    // object or `avio_close` will get confused and double free.
    // We can simply ignore the resulting buffer.
    let _ = ffi::output_raw_buf_end(&mut self.output);
  }

}

/// Video writer that writes to a packetized buffer.
pub struct PacketizedBufWriter {
  pub(crate) output: AvOutput,
  options: Options<'static>,
  buffers: Bufs,
}

impl PacketizedBufWriter {
  /// Actual packet size. Value should be below MTU.
  const PACKET_SIZE: usize = 1024;

  /// Create a video writer that writes multiple packets to a buffer
  /// and returns the resulting bytes for each packet.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Container format to use.
  /// 
  /// # Examples
  /// 
  /// ```
  /// let mut writer = BufPacketizedWriter::new("rtp").unwrap();
  /// let bytes = writer.write_header()?;
  /// ```
  pub fn new(format: &str) -> Result<Self> {
    Self::new_with(format, Default::default())
  }

  /// Create a video writer that writes multiple packets to a buffer
  /// and returns the resulting bytes for each packet. This constructor
  /// also allows for passing options for the ffmpeg backend.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Container format to use.
  /// * `options` - Options to pass on to ffmpeg.
  pub fn new_with(format: &str, options: Options<'static>) -> Result<Self> {
    let output = ffi::output_raw(format)?;
    
    Ok(Self {
      output,
      options,
      buffers: Vec::new(),
    })
  }

  fn begin_write(&mut self) {
    ffi::output_raw_packetized_buf_start(
      &mut self.output,
      // Note: `ffi::output_raw_packetized_bug_start` requires that this
      // value lives until `ffi::output_raw_packetized_buf_end`. This is
      // guaranteed by the fact that `begin_write` is always followed by
      // an invocation of `end_write` in the same function (see the
      // implementation) of `Write` for `PacketizedBufWriter`.
      &mut self.buffers,
      Self::PACKET_SIZE,
    );
  }

  fn end_write(&mut self) {
    ffi::output_raw_packetized_buf_end(&mut self.output);
  }

  #[inline]
  fn take_buffers(&mut self) -> Bufs {
    // We take the buffers here and replace them with an empty `Vec`.
    mem::take(&mut self.buffers)
  }

}

impl Write for PacketizedBufWriter {}

/// Wrapper type for any valid video source. Currently, this could be
/// a URI, file path or any other input the backend will accept. Later,
/// we might add some scaffolding to have stricter typing.
#[derive(Clone)]
pub enum Locator {
  Path(PathBuf),
  Url(Url)
}

impl Locator {

  /// Resolves the locator into a `PathBuf` for usage with `ffmpeg-next`.
  fn resolve(&self) -> &Path {
    match self {
      Locator::Path(path) => path.as_path(),
      Locator::Url(url) => Path::new(url.as_str())
    }
  }

}

/// Allow conversion from path to `Locator`.
impl From<PathBuf> for Locator {

  fn from(path: PathBuf) -> Locator {
    Locator::Path(path)
  }

}

/// Allow conversion from `Url` to `Locator`.
impl From<Url> for Locator {

  fn from(url: Url) -> Locator {
    Locator::Url(url)
  }

}

/// Allow conversion to string and display for locator types.
impl std::fmt::Display for Locator {

  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Locator::Path(ref path) =>
        write!(f, "{}", path.display()),
      Locator::Url(ref url) =>
        write!(f, "{}", url),
    }
  }

}

pub(crate) mod private {

  use super::{
    AvOutput,
    AvPacket,
    Writer,
    BufWriter,
    PacketizedBufWriter,
    Buf,
    Bufs,
    Error,
    ffi,
  };

  type Result<T> = std::result::Result<T, Error>;

  pub trait Write {
    type Out;

    /// Write the container header.
    fn write_header(&mut self) -> Result<Self::Out>;

    /// Write a packet into the container.
    /// 
    /// # Arguments
    /// 
    /// * `packet` - Packet to write.
    fn write(&mut self, packet: &mut AvPacket) -> Result<Self::Out>;

    /// Write a packet into the container and take care of interleaving.
    /// 
    /// # Arguments
    /// 
    /// * `packet` - Packet to write.
    fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<Self::Out>;

    /// Write the container trailer.
    fn write_trailer(&mut self) -> Result<Self::Out>;
  }

  impl Write for Writer {
    type Out = ();

    fn write_header(&mut self) -> Result<()> {
      Ok(self.output.write_header()?)
    }

    fn write(&mut self, packet: &mut AvPacket) -> Result<()> {
      packet.write(&mut self.output)?;
      Ok(())
    }

    fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<()> {
      packet.write_interleaved(&mut self.output)?;
      Ok(())
    }

    fn write_trailer(&mut self) -> Result<()> {
      Ok(self.output.write_trailer()?)
    }

  }

  impl Write for BufWriter {
    type Out = Buf;

    fn write_header(&mut self) -> Result<Buf> {
      self.begin_write();
      self.output.write_header_with(self.options.to_dict())?;
      Ok(self.end_write())
    }

    fn write(&mut self, packet: &mut AvPacket) -> Result<Buf> {
      self.begin_write();
      packet.write(&mut self.output)?;
      ffi::flush_output(&mut self.output)?;
      Ok(self.end_write())
    }

    fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<Buf> {
      self.begin_write();
      packet.write_interleaved(&mut self.output)?;
      ffi::flush_output(&mut self.output)?;
      Ok(self.end_write())
    }

    fn write_trailer(&mut self) -> Result<Buf> {
      self.begin_write();
      self.output.write_trailer()?;
      Ok(self.end_write())
    }

  }

  impl Write for PacketizedBufWriter {
    type Out = Bufs;

    fn write_header(&mut self) -> Result<Bufs> {
      self.begin_write();
      self.output.write_header_with(self.options.to_dict())?;
      self.end_write();
      Ok(self.take_buffers())
    }

    fn write(&mut self, packet: &mut AvPacket) -> Result<Bufs> {
      self.begin_write();
      packet.write(&mut self.output)?;
      ffi::flush_output(&mut self.output)?;
      self.end_write();
      Ok(self.take_buffers())
    }

    fn write_interleaved(&mut self, packet: &mut AvPacket) -> Result<Bufs> {
      self.begin_write();
      packet.write_interleaved(&mut self.output)?;
      ffi::flush_output(&mut self.output)?;
      self.end_write();
      Ok(self.take_buffers())
    }

    fn write_trailer(&mut self) -> Result<Bufs> {
      self.begin_write();
      self.output.write_trailer()?;
      self.end_write();
      Ok(self.take_buffers())
    }

  }

  pub trait Output {

    /// Obtain reference to output context.
    fn output(&self) -> &AvOutput;

    /// Obtain mutable reference to output context.
    fn output_mut(&mut self) -> &mut AvOutput;

  }

  impl Output for Writer {

    fn output(&self) -> &AvOutput  {
      &self.output
    }

    fn output_mut(&mut self) -> &mut AvOutput  {
      &mut self.output
    }

  }

  impl Output for BufWriter {

    fn output(&self) -> &AvOutput  {
      &self.output
    }

    fn output_mut(&mut self) -> &mut AvOutput  {
      &mut self.output
    }

  }

  impl Output for PacketizedBufWriter {

    fn output(&self) -> &AvOutput  {
      &self.output
    }

    fn output_mut(&mut self) -> &mut AvOutput  {
      &mut self.output
    }

  }

}
