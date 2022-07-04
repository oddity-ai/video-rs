extern crate ffmpeg_next as ffmpeg;

use ffmpeg::{
  format::{
    version,
    register_all,
  },
  error::register_all as register_all_errors,
};

use super::ffi::init_logging;

/// The version of `libavformat` that deprecated `register_all`.
const AVFORMAT_MAX_VERSION_REQUIRES_REGISTER_ALL: u32 = 3803492;

/// Initialize global ffmpeg settings. This also intializes the
/// logging capability and redirect it to `tracing`.
/// 
/// In older versions, this will invoke `register_all`.
pub fn init() {
  // Older versions of ffmpeg require this.
  if version() < AVFORMAT_MAX_VERSION_REQUIRES_REGISTER_ALL {
    register_all();
  }

  // Or error messages will be empty.
  register_all_errors();

  // Redirect logging to the Rust `tracing` crate.
  init_logging();
}