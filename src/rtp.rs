use super::{
  Reader,
  PacketizedBufMuxer,
  StreamInfo,
  Packet,
  Error,
  Sps,
  Pps,
  io::Buf,
  ffi::{
    sdp,
    rtp_h264_mode_0,
    rtp_seq_and_timestamp,
  },
};

type Result<T> = std::result::Result<T, Error>;

/// Represents a muxer that muxes into the RTP format and streams the
/// output over RTP.
pub struct RtpMuxer(PacketizedBufMuxer);

impl RtpMuxer {

  /// Create a new muxer that produces an RTP stream to a buffer.
  pub fn new() -> Result<Self> {
    PacketizedBufMuxer::new_to_packetized_buf("rtp")
      .map(RtpMuxer)
  }

  /// Add an output stream to the muxer based on an input stream from
  /// a reader. Any packets provided to `mux` from the given input
  /// stream will be muxed to the corresponding output stream.
  /// 
  /// At least one stream must be added before any muxing can take place.
  /// 
  /// # Arguments
  /// 
  /// * `stream_info` - Stream information. Usually this information is
  ///   retrieved by calling `reader.stream_info(index)`.
  pub fn with_stream(
    self,
    stream_info: StreamInfo,
  ) -> Result<Self> {
    self.0.with_stream(stream_info)
      .map(RtpMuxer)
  }

  /// Add output streams from reader to muxer. This will add all streams
  /// in the reader and duplicate them in the muxer. After calling this,
  /// it is safe to mux all packets from the provided reader.
  /// 
  /// # Arguments
  /// 
  /// * `reader` - Reader to add streams from.
  pub fn with_streams(
    self,
    reader: &Reader,
  ) -> Result<Self> {
    self.0.with_streams(reader)
      .map(RtpMuxer)
  }

  /// Get the RTP packetization mode used by the muxer.
  pub fn packetization_mode(&self) -> usize {
    let is_packetization_mode_0 =
      rtp_h264_mode_0(&self.0.writer.output);

    if !is_packetization_mode_0 {
      1
    } else {
      0
    }
  }

  /// Get parameter sets corresponding to each internal stream. The
  /// parameter set contains one SPS (Sequence Parameter Set) and
  /// zero or more PPSs (Picture Parameter Sets).
  /// 
  /// Note that this function only supports extracting parameter
  /// sets for streams with the H.264 codec and will return
  /// `Error::UnsupportedCodecParameterSets` for streams with another
  /// type of codec.
  pub fn parameter_sets_h264<'param>(
    &'param self
  ) -> Vec<Result<(Sps<'param>, Pps<'param>)>> {
    self.0.parameter_sets_h264()
  }

  /// Get the current RTP sequence number and timestamp.
  pub fn seq_and_timestamp(&self) -> (u16, u32) {
    rtp_seq_and_timestamp(&self.0.writer.output)
  }

  /// Produce SDP (Session Description Protocol) file contents for this
  /// stream using the `libavcodec` backend.
  /// 
  /// # Returns
  /// 
  /// An SDP file string, for example:
  /// 
  /// ```text
  /// v=0
  /// o=- 0 0 IN IP4 127.0.0.1
  /// s=No Name
  /// c=IN IP4 127.0.0.1
  /// t=0 0
  /// a=tool:libavformat 55.2.100
  /// m=video 1235 RTP/AVP 96
  /// a=rtpmap:96 H264/90000
  /// a=fmtp:96 packetization-mode=1
  /// ```
  pub fn sdp(&self) -> Result<String> {
    sdp(&self.0.writer.output)
      .map_err(Error::BackendError)
  }

  /// Mux a single packet. This will cause the muxer to try and read
  /// packets from the preferred stream, mux it and return one or more
  /// RTP buffers.
  pub fn mux(
    &mut self,
    packet: Packet,
  ) -> Result<Vec<RtpBuf>> {
    self.0.mux(packet)
      .map(|bufs| bufs
        .into_iter()
        .map(|buf| buf.into())
        .collect())
  }

  /// Signal to the muxer that writing has finished. This will cause
  /// trailing packets to be returned if the container format has one.
  pub fn finish(&mut self) -> Result<Vec<RtpBuf>> {
    self.0.finish()
      .map(|bufs| bufs
        .into_iter()
        .map(|buf| buf.into())
        .collect())
  }

}

/// Buffer-form RTP packet, can be either a normal RTP payload or an
/// RTCP packet (a sender report).
pub enum RtpBuf {
  Rtp(Buf),
  Rtcp(Buf),
}

impl From<Buf> for RtpBuf {

  /// Convert a buffer to an `RtpBuf`. An `RtpBuf` can either be a
  /// normal RTP buf or an RTCP buf.
  fn from(buf: Buf) -> Self {
    const RTCP_SR_MARKER: u8 = 200;

    if buf.len() >= 2 {
      if buf[1] == RTCP_SR_MARKER {
        RtpBuf::Rtcp(buf)
      } else {
        RtpBuf::Rtp(buf)
      }
    } else {
      RtpBuf::Rtp(buf)
    }
  }

}

impl From<RtpBuf> for Buf {

  /// Convert from `RtpBuf` to normal `Buf`, without metadata about
  /// the type of payload.
  fn from(rtp_buf: RtpBuf) -> Self {
    match rtp_buf {
      RtpBuf::Rtp(buf)  => buf,
      RtpBuf::Rtcp(buf) => buf,
    }
  }

}