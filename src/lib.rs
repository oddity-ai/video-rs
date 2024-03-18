pub mod decode;
pub mod encode;
pub mod error;
pub mod extradata;
pub mod frame;
pub mod hwaccel;
pub mod init;
pub mod io;
pub mod location;
pub mod mux;
pub mod options;
pub mod packet;
pub mod resize;
pub mod rtp;
pub mod stream;
pub mod time;

mod ffi;
mod ffi_hwaccel;

pub use decode::{Decoder, DecoderBuilder};
pub use encode::{Encoder, EncoderBuilder};
pub use error::Error;
#[cfg(feature = "ndarray")]
pub use frame::Frame;
pub use init::init;
pub use io::{Reader, ReaderBuilder, Writer, WriterBuilder};
pub use location::{Location, Url};
pub use mux::{Muxer, MuxerBuilder};
pub use options::Options;
pub use packet::Packet;
pub use resize::Resize;
pub use time::Time;

/// Re-export backend `ffmpeg` library.
pub use ffmpeg_next as ffmpeg;
