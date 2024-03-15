mod decode;
mod encode;
mod error;
mod extradata;
mod ffi;
mod ffi_hwaccel;
mod frame;
mod hwaccel;
mod init;
mod io;
mod mux;
mod options;
mod packet;
mod resize;
mod rtp;
mod stream;
mod time;

pub use decode::{Decoder, DecoderBuilder, DecoderSplit};
pub use encode::{Encoder, EncoderBuilder, Settings as EncoderSettings};
pub use error::Error;
pub use extradata::{Pps, Sps};
pub use frame::PixelFormat;
pub use frame::RawFrame;
pub use hwaccel::HardwareAccelerationDeviceType;
pub use init::init;
pub use io::{
    Buf, BufWriter, BufWriterBuilder, PacketizedBufWriter, PacketizedBufWriterBuilder, Reader,
    ReaderBuilder, Write, Writer, WriterBuilder,
};
pub use io::{Locator, Url};
pub use mux::{BufMuxer, FileMuxer, PacketizedBufMuxer};
pub use options::Options;
pub use packet::Packet;
pub use resize::Resize;
pub use rtp::{RtpBuf, RtpMuxer};
pub use stream::StreamInfo;
pub use time::{Aligned, Time};

#[cfg(feature = "ndarray")]
pub use frame::Frame;

/// Re-export inner `ffmpeg` library.
pub use ffmpeg_next as ffmpeg;
