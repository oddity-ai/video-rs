extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::Id as AvCodecId;
use ffmpeg::{Error as AvError, Rational as AvRational};

use crate::error::Error;
use crate::extradata::{extract_parameter_sets_h264, Pps, Sps};
use crate::ffi::extradata;
use crate::io::{Reader, Write};
use crate::packet::Packet;
use crate::stream::StreamInfo;

type Result<T> = std::result::Result<T, Error>;

/// Builds a [`Muxer`].
pub struct MuxerBuilder<W: Write> {
    writer: W,
    interleaved: bool,
    mapping: std::collections::HashMap<usize, StreamDescription>,
}

impl<W: Write> MuxerBuilder<W> {
    /// Create a new [`MuxerBuilder`].
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            interleaved: false,
            mapping: std::collections::HashMap::new(),
        }
    }

    /// Add an output stream to the muxer based on an input stream from a reader. Any packets
    /// provided to [`Muxer::mux()`] from the given input stream will be muxed to the corresponding
    /// output stream.
    ///
    /// At least one stream must be added before any muxing can take place.
    ///
    /// # Arguments
    ///
    /// * `stream_info` - Stream information. Usually this information is retrieved by calling
    ///   [`Reader::stream_info()`].
    pub fn with_stream(mut self, stream_info: StreamInfo) -> Result<Self> {
        let (index, codec_parameters, reader_stream_time_base) = stream_info.into_parts();
        let mut writer_stream = self
            .writer
            .output_mut()
            .add_stream(ffmpeg::encoder::find(codec_parameters.id()))?;
        writer_stream.set_parameters(codec_parameters);
        let stream_description = StreamDescription {
            index: writer_stream.index(),
            source_time_base: reader_stream_time_base,
        };
        self.mapping.insert(index, stream_description);
        Ok(self)
    }

    /// Add output streams from reader to muxer. This will add all streams in the reader and
    /// duplicate them in the muxer. After calling this, it is safe to mux all packets from the
    /// provided reader.
    ///
    /// # Arguments
    ///
    /// * `reader` - Reader to add streams from.
    pub fn with_streams(mut self, reader: &Reader) -> Result<Self> {
        for stream in reader.input.streams() {
            self = self.with_stream(reader.stream_info(stream.index())?)?;
        }
        Ok(self)
    }

    /// Set interleaved. This will cause the muxer to use interleaved write instead of normal
    /// write.
    pub fn interleaved(mut self) -> Self {
        self.interleaved = true;
        self
    }

    /// Build [`Muxer`].
    pub fn build(self) -> Muxer<W> {
        Muxer {
            writer: self.writer,
            mapping: self.mapping,
            interleaved: self.interleaved,
            have_written_header: false,
            have_written_trailer: false,
        }
    }
}

/// Represents a muxer. A muxer allows muxing media packets into a new container format. Muxing does
/// not require encoding and/or decoding.
///
/// # Examples
///
/// Mux to an MKV file:
///
/// ```ignore
/// let reader = Reader::new(Path::new("from_file.mp4")).unwrap();
/// let writer = Writer::new(Path::new("to_file.mkv")).unwrap();
/// let muxer = MuxerBuilder::new(writer)
///     .with_streams(&reader)
///     .build()
///     .unwrap();
/// while let Ok(packet) = reader.read() {
///     muxer.mux(packet).unwrap();
/// }
/// muxer.finish().unwrap();
/// ```
///
/// Mux from file to MP4 and print length of first 100 buffer segments:
///
/// ```ignore
/// let reader = Reader::new(Path::new("my_file.mp4")).unwrap();
/// let writer = BufWriter::new("mp4").unwrap();
/// let mut muxer = MuxerBuilder::new(writer)
///     .with_streams(&reader)
///     .unwrap();
/// for _ in 0..100 {
///     println!("len: {}", muxer.mux().unwrap().len());
/// }
/// muxer.finish()?;
/// ```
pub struct Muxer<W: Write> {
    pub(crate) writer: W,
    mapping: std::collections::HashMap<usize, StreamDescription>,
    interleaved: bool,
    have_written_header: bool,
    have_written_trailer: bool,
}

impl<W: Write> Muxer<W> {
    /// Mux a single packet. This will mux a single packet.
    ///
    /// # Arguments
    ///
    /// * `packet` - [`Packet`] to mux.
    pub fn mux(&mut self, packet: Packet) -> Result<W::Out> {
        if self.have_written_header {
            let mut packet = packet.into_inner();
            let stream_description = self
                .mapping
                .get(&packet.stream())
                .ok_or(AvError::StreamNotFound)?;

            let destination_stream = self
                .writer
                .output()
                .stream(stream_description.index)
                .ok_or(AvError::StreamNotFound)?;

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

    /// Signal to the muxer that writing has finished. This will cause a trailer to be written if
    /// the container format has one.
    pub fn finish(&mut self) -> Result<Option<W::Out>> {
        if self.have_written_header && !self.have_written_trailer {
            self.have_written_trailer = true;
            self.writer.write_trailer().map(Some)
        } else {
            Ok(None)
        }
    }

    /// Get parameter sets corresponding to each internal stream. The parameter set contains one SPS
    /// (Sequence Parameter Set) and zero or more PPSs (Picture Parameter Sets).
    ///
    /// Note that this function only supports extracting parameter sets for streams with the H.264
    /// codec and will return `Error::UnsupportedCodecParameterSets` for streams with another type
    /// of codec.
    pub fn parameter_sets_h264(&self) -> Vec<Result<(Sps<'_>, Pps<'_>)>> {
        self.writer
            .output()
            .streams()
            .map(|stream| {
                if stream.parameters().id() == AvCodecId::H264 {
                    extract_parameter_sets_h264(extradata(self.writer.output(), stream.index())?)
                } else {
                    Err(Error::UnsupportedCodecParameterSets)
                }
            })
            .collect::<Vec<_>>()
    }
}

unsafe impl<W: Write> Send for Muxer<W> {}
unsafe impl<W: Write> Sync for Muxer<W> {}

/// Internal structure that holds the stream index and the time base of the source packet for
/// rescaling.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamDescription {
    index: usize,
    source_time_base: AvRational,
}
