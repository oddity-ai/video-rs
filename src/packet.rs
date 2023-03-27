extern crate ffmpeg_next as ffmpeg;

use ffmpeg::Rational as AvRational;
use ffmpeg::codec::packet::Packet as AvPacket;

use crate::time::Time;

/// Represents a stream packet.
#[derive(Clone)]
pub struct Packet {
  inner: AvPacket,
  time_base: AvRational,
}

impl Packet {

  /// Get packet PTS (presentation timestamp).
  pub fn pts(&self) -> Time {
    Time::new(self.inner.pts(), self.time_base)
  }

  /// Get packet DTS (decoder timestamp).
  pub fn dts(&self) -> Time {
    Time::new(self.inner.dts(), self.time_base)
  }

  /// Get packet duration.
  pub fn duration(&self) -> Time {
    Time::new(Some(self.inner.duration()), self.time_base)
  }

  /// Set packet PTS (presentation timestamp).
  pub fn set_pts(&mut self, timestamp: &Time) {
    self.inner.set_pts(timestamp.aligned(self.time_base).into_value());
  }

  /// Set packet DTS (decoder timestamp).
  pub fn set_dts(&mut self, timestamp: &Time) {
    self.inner.set_dts(timestamp.aligned(self.time_base).into_value());
  }

  /// Set duration.
  pub fn set_duration(&mut self, timestamp: &Time) {
    if let Some(duration) = timestamp.aligned(self.time_base).into_value() {
      self.inner.set_duration(duration);
    }
  }

  /// Create a new packet.
  /// 
  /// # Arguments
  /// 
  /// * `inner` - Inner `AvPacket`.
  /// * `time_base` - Source time base.
  pub(crate) fn new(
    inner: AvPacket,
    time_base: AvRational,
  ) -> Self {
    Self {
      inner,
      time_base,
    }
  }

  /// Downcast to native inner type.
  pub(crate) fn into_inner(self) -> AvPacket {
    self.inner
  }

}

unsafe impl Send for Packet {}
unsafe impl Sync for Packet {}