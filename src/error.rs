extern crate ffmpeg_next as ffmpeg;

use std::fmt;
use std::error;

use ffmpeg::Error as FfmpegError;

/// Represents video I/O Errors. Some errors are generated
/// by the ffmpeg backend, and are wrapped in `BackendError`.
#[derive(Debug, Clone)]
pub enum Error {
  ReadExhausted,
  WriteRetryLimitReached,
  InvalidFrameFormat,
  InvalidExtraData,
  MissingCodecParameters,
  UnsupporedCodecParameterSets,
  BackendError(FfmpegError),
}

impl error::Error for Error {
  
  fn source(&self) -> Option<&(dyn error::Error + 'static)> {
    match *self {
      Error::ReadExhausted => None,
      Error::WriteRetryLimitReached => None,
      Error::InvalidFrameFormat => None,
      Error::InvalidExtraData => None,
      Error::MissingCodecParameters => None,
      Error::UnsupporedCodecParameterSets => None,
      Error::BackendError(ref internal) =>
        Some(internal),
    }
  }

}

impl fmt::Display for Error {

  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    match *self {
      Error::ReadExhausted =>
        write!(f, "stream exhausted"),
      Error::WriteRetryLimitReached =>
        write!(f, "cannot write to video stream, even after multiple tries"),
      Error::InvalidFrameFormat =>
        write!(f, "provided frame does not match expected dimensions and/or pixel format"),
      Error::InvalidExtraData =>
        write!(f, "codec parameters extradata is corrupted"),
      Error::MissingCodecParameters =>
        write!(f, "codec parameters missing"),
      Error::UnsupporedCodecParameterSets =>
        write!(f, "extracting parameter sets for this codec is not suppored"),
      Error::BackendError(ref internal) =>
        internal.fmt(f),
    }
  }

}

impl From<FfmpegError> for Error {

  fn from(internal: FfmpegError) -> Error {
    Error::BackendError(internal)
  }

}
