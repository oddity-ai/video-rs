extern crate ffmpeg_next as ffmpeg;

use ffmpeg::util::{
  frame::Video as AvFrame,
  format::Pixel as AvPixel,
};

/// Re-export internal `AvFrame` for caller to use.
pub type RawFrame = AvFrame;

/// Re-export frame type as ndarray.
#[cfg(feature = "ndarray")]
pub type Frame = super::ffi::FrameArray;

/// Default frame pixel format.
pub(crate) const FRAME_PIXEL_FORMAT: AvPixel = AvPixel::RGB24;