extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::Parameters as AvCodecParameters;
use ffmpeg::{Error as AvError, Rational as AvRational};

use crate::error::Error;
use crate::io::Reader;

type Result<T> = std::result::Result<T, Error>;

/// Holds transferable stream information. This can be used to duplicate stream settings for the
/// purpose of transmuxing or transcoding.
#[derive(Clone)]
pub struct StreamInfo {
    pub index: usize,
    codec_parameters: AvCodecParameters,
    time_base: AvRational,
}

impl StreamInfo {
    /// Fetch stream information from a reader by stream index.
    ///
    /// # Arguments
    ///
    /// * `reader` - Reader to find stream information from.
    /// * `stream_index` - Index of stream in reader.
    pub(crate) fn from_reader(reader: &Reader, stream_index: usize) -> Result<Self> {
        let stream = reader
            .input
            .stream(stream_index)
            .ok_or(AvError::StreamNotFound)?;

        Ok(Self {
            index: stream_index,
            codec_parameters: stream.parameters(),
            time_base: stream.time_base(),
        })
    }

    /// Turn information back into parts for usage.
    ///
    /// Note: Consumes stream information object.
    ///
    /// # Return value
    ///
    /// A tuple consisting of:
    /// * The stream index.
    /// * Codec parameters.
    /// * Original stream time base.
    pub(crate) fn into_parts(self) -> (usize, AvCodecParameters, AvRational) {
        (self.index, self.codec_parameters, self.time_base)
    }
}

unsafe impl Send for StreamInfo {}
unsafe impl Sync for StreamInfo {}
