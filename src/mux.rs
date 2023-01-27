extern crate ffmpeg_next as ffmpeg;

use std::collections::HashMap;

use ffmpeg::{
  codec::Id as AvCodecId,
  Rational as AvRational,
  Error as AvError,
};

use super::{
  Error,
  Locator,
  Sps,
  Pps,
  Packet,
  StreamInfo,
};
use super::io::{
  Reader,
  Write,
  Writer,
  BufWriter,
  PacketizedBufWriter,
};
use super::extradata::extract_parameter_sets_h264;
use super::ffi::extradata;
use super::options::Options;

type Result<T> = std::result::Result<T, Error>;

/// Represents a muxer. A muxer allows muxing media packets into a
/// new container format. Muxing does not require encoding and/or
/// decoding.
pub struct Muxer<W: Write> {
  pub(crate) writer: W,
  mapping: HashMap<usize, StreamDescription>,
  interleaved: bool,
  have_written_header: bool,
}

/// Represents a muxer that writes to a file.
pub type FileMuxer = Muxer<Writer>;

/// Represents a muxer that writes to a buffer.
pub type BufMuxer = Muxer<BufWriter>;

/// Represents a muxer that writes to a packetized buffer.
pub type PacketizedBufMuxer = Muxer<PacketizedBufWriter>;

impl Muxer<Writer> {

  /// Create a muxer that writes to a file.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Locator to mux to, like a file or URL.
  /// 
  /// # Examples
  /// 
  /// Mux to an MKV file.
  /// 
  /// ```
  /// let reader = Reader::new(
  ///     &PathBuf::from("from_file.mp4").into())
  ///   .unwrap();
  ///   
  /// let muxer = Muxer::new_to_file(
  ///     &PathBuf::from("to_file.mkv").into())
  ///   .unwrap()
  ///   .with_streams(&reader)
  ///   .unwrap();
  /// 
  /// while let Ok(packet) = reader.read() {
  ///   muxer.mux(packet).unwrap();
  /// }
  /// 
  /// muxer.finish().unwrap();
  /// ```
  pub fn new_to_file(dest: &Locator) -> Result<Self> {
    Self::new(Writer::new(dest)?)
  }

  /// Create a muxer that writes to a file and allows for specifying
  /// ffmpeg options for the destination writer.
  /// 
  /// # Arguments
  /// 
  /// * `dest` - Locator to mux to, like a file or URL.
  /// * `format` - Format to mux into.
  pub fn new_to_file_with_format(
    dest: &Locator,
    format: &str,
  ) -> Result<Self> {
    Self::new(Writer::new_with_format(dest, format)?)
  }

}

impl Muxer<PacketizedBufWriter> {

  /// Create a muxer that writes to a packetized buffer. This is the
  /// packetized variant of `new_to_buf`.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Format to mux into.
  pub fn new_to_packetized_buf(
    format: &str,
  ) -> Result<Self> {
    Self::new(PacketizedBufWriter::new(format)?)
  }

  /// Create a muxer that writes to a packetized buffer and allows for
  /// specifying ffmpeg options for the destination writer.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Format to mux into.
  /// * `options` - Options for the writer.
  pub fn new_to_packetized_buf_with_options(
    format: &str,
    options: Options<'static>,
  ) -> Result<Self> {
    Self::new(PacketizedBufWriter::new_with(format, options)?)
  }

}

impl Muxer<BufWriter> {

  /// Create a muxer that writes to a buffer.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Format to mux into.
  /// 
  /// # Examples
  /// 
  /// Mux from file to mp4 and print length of first 100 buffer segments.
  /// 
  /// ```
  /// let reader = Reader::new(&PathBuf::from("my_file.mp4").into())
  ///   .unwrap();
  /// let mut muxer = Muxer::new_to_buf("mp4")
  ///   .unwrap()
  ///   .with_streams(&reader)
  ///   .unwrap();
  ///
  /// for _ in 0..100 {
  ///   println!("len: {}", muxer.mux().unwrap().len());
  /// }
  /// 
  /// muxer.finish()?;
  /// ```
  pub fn new_to_buf(
    format: &str,
  ) -> Result<Self> {
    Self::new(BufWriter::new(format)?)
  }

  /// Create a muxer that writes to a buffer and allows for specifying
  /// ffmpeg options for the destination writer.
  /// 
  /// # Arguments
  /// 
  /// * `format` - Format to mux into.
  /// * `options` - Options for the writer.
  pub fn new_to_buf_with_options(
    format: &str,
    options: Options<'static>,
  ) -> Result<Self> {
    Self::new(BufWriter::new_with(format, options)?)
  }

}

impl<W: Write> Muxer<W> {

  /// Create a muxer.
  /// 
  /// # Arguments
  /// 
  /// * `writer` - Video writer that implements the `Writer` trait.
  /// 
  /// # Examples
  /// 
  /// ```
  /// let muxer = Muxer::new(BufWriter::new("mp4").unwrap())
  ///   .unwrap());
  /// ```
  fn new(writer: W) -> Result<Self> {
    Ok(Self {
      writer,
      mapping: HashMap::new(),
      interleaved: false,
      have_written_header: false,
    })
  }

  /// Turn the muxer into an interleaved version, that automatically
  /// reorders packets when necessary.
  pub fn interleaved(mut self) -> Self {
    self.interleaved = true;
    self
  }

  /// Add an output stream to the muxer based on an input stream from
  /// a reader. Any packets provided to `mux` from the given input
  /// stream will be muxed to the corresponding output stream.
  /// 
  /// At least one stream must be added before any muxing can take place.
  /// 
  /// # Arguments
  /// 
  /// * `stream_info` - Stream information. Usually this information is
  ///   retrieved by calling `reader.stream_info(index)`.
  pub fn with_stream(
    mut self,
    stream_info: StreamInfo,
  ) -> Result<Self> {
    let (index, codec_parameters, reader_stream_time_base) =
      stream_info.into_parts();

    let mut writer_stream = self
      .writer
      .output_mut()
      .add_stream(ffmpeg::encoder::find(codec_parameters.id()))?;
    writer_stream.set_parameters(codec_parameters);

    let stream_description = StreamDescription {
      index: writer_stream.index(),
      source_time_base: reader_stream_time_base,
    };

    self
      .mapping
      .insert(index, stream_description);

    Ok(self)
  }

  /// Add output streams from reader to muxer. This will add all streams
  /// in the reader and duplicate them in the muxer. After calling this,
  /// it is safe to mux all packets from the provided reader.
  /// 
  /// # Arguments
  /// 
  /// * `reader` - Reader to add streams from.
  pub fn with_streams(
    mut self,
    reader: &Reader,
  ) -> Result<Self> {
    for stream in reader.input.streams() {
      self = self.with_stream(reader.stream_info(stream.index())?)?;
    }

    Ok(self)
  }

  /// Mux a single packet. This will mux a single packet.
  /// 
  /// # Arguments
  /// 
  /// * `packet` - Packet to mux.
  pub fn mux(
    &mut self,
    packet: Packet,
  ) -> Result<W::Out> {
    if self.have_written_header {
      let mut packet = packet.into_inner();
      let stream_description = self
        .mapping
        .get(&packet.stream())
        .ok_or_else(|| AvError::StreamNotFound)?;

      let destination_stream = self
        .writer
        .output()
        .stream(stream_description.index)
        .ok_or_else(|| AvError::StreamNotFound)?;

      packet.set_stream(destination_stream.index());
      packet.set_position(-1);
      packet.rescale_ts(
        stream_description.source_time_base,
        destination_stream.time_base(),
      );

      Ok({
        if self.interleaved {
          self.writer.write_interleaved(&mut packet)?
        } else {
          self.writer.write(&mut packet)?
        }
      })
    } else {
      self.have_written_header = true;
      self.writer.write_header()?;
      self.mux(packet)
    }
  }

  /// Get parameter sets corresponding to each internal stream. The
  /// parameter set contains one SPS (Sequence Parameter Set) and
  /// zero or more PPSs (Picture Parameter Sets).
  /// 
  /// Note that this function only supports extracting parameter
  /// sets for streams with the H.264 codec and will return
  /// `Error::UnsupportedCodecParameterSets` for streams with another
  /// type of codec.
  pub fn parameter_sets_h264<'param>(
    &'param self,
  ) -> Vec<Result<(Sps<'param>, Pps<'param>)>> {
    self
      .writer
      .output()
      .streams()
      .map(|stream| {
        if stream.codec().id() == AvCodecId::H264 {
          extract_parameter_sets_h264(
            extradata(
              &self.writer.output(),
              stream.index()
            )?
          )
        } else {
          Err(Error::UnsupportedCodecParameterSets)
        }
      })
      .collect::<Vec<_>>()
  }

  /// Signal to the muxer that writing has finished. This will cause a
  /// trailer to be written if the container format has one.
  pub fn finish(&mut self) -> Result<W::Out> {
    self.writer.write_trailer()
  }

}

/// Internal structure that holds the stream index and the time base of the
/// source packet for rescaling.
struct StreamDescription {
  index: usize,
  source_time_base: AvRational,
}