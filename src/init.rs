extern crate ffmpeg_next as ffmpeg;

use ffmpeg::{
  error::register_all as register_all_errors,
};

use super::ffi::init_logging;

/// Initialize global ffmpeg settings. This also intializes the
/// logging capability and redirect it to `tracing`.
pub fn init() {
  // Or error messages will be empty.
  register_all_errors();

  // Redirect logging to the Rust `tracing` crate.
  init_logging();
}