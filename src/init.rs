extern crate ffmpeg_next as ffmpeg;

use std::error::Error;

use super::ffi::init_logging;

/// Initialize global ffmpeg settings. This also intializes the
/// logging capability and redirect it to `tracing`.
pub fn init() -> Result<(), Box<dyn Error>> {
  ffmpeg::init()?;

  // Redirect logging to the Rust `tracing` crate.
  init_logging();

  Ok(())
}