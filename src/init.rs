extern crate ffmpeg_next as ffmpeg;

use crate::ffi::init_logging;

/// Initialize global ffmpeg settings. This also intializes the
/// logging capability and redirect it to `tracing`.
pub fn init() -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;

    // Redirect logging to the Rust `tracing` crate.
    init_logging();

    Ok(())
}
