extern crate ffmpeg_next as ffmpeg;

use std::time::Duration;

use ffmpeg::{
  Rational as AvRational,
  util::mathematics::rescale::Rescale,
};

/// Represents a frame timestamp relative to the start of original
/// stream. This can be either a presentation timestamp (PTS), decoder
/// timestamp (DTS) or even a duration.
#[derive(Clone, Debug)]
pub struct Time {
  time: Option<i64>,
  time_base: AvRational,
}

impl Time {

  /// Create a new zero-valued timestamp.
  pub fn zero() -> Self {
    Time {
      time: Some(0),
      time_base: (1, 90000).into(),
    }
  }

  /// Create a new timestamp by its time value and time base in
  /// which the time is expressed. These two components are enough
  /// to rescale to a new time base.
  /// 
  /// # Arguments
  /// 
  /// * `time` - Relative time in `time_base` units.
  /// * `time_base` - Time base of source.
  pub(crate) fn new(
    time: Option<i64>,
    time_base: AvRational,
  ) -> Time {
    Self {
      time,
      time_base,
    }
  }

  /// Whether or not the `Time` has a time at all.
  pub fn has_value(&self) -> bool {
    self.time.is_some()
  }

  /// Align the timestamp with another timestamp, which will convert
  /// the `rhs` timestamp to the same time base, such that operations
  /// can be performed upon the aligned timestamps.
  /// 
  /// # Arguments
  /// 
  /// * `rhs` - Right-hand side timestamp.
  /// 
  /// # Returns
  /// 
  /// Two timestamps that are aligned.
  pub fn aligned_with(&self, rhs: &Time) -> Aligned {
    Aligned {
      lhs: self.time.clone(),
      rhs: rhs
        .time
        .map(|rhs_time| rhs_time.rescale(rhs.time_base, self.time_base)),
      time_base: self.time_base,
    }
  }

  /// Align the timestamp along another `time_base`.
  /// 
  /// # Arguments
  /// 
  /// * `time_base` - Target time base.
  pub(crate) fn aligned(&self, time_base: AvRational) -> Time {
    Time {
      time: self
        .time
        .map(|time| time.rescale(self.time_base, time_base)),
      time_base,
    }
  }

  /// Convert to underlying time in `i64`. Assumes that the user knows
  /// the time base and applies it correctly.
  pub(crate) fn into_value(self) -> Option<i64> {
    self.time
  }

}

impl From<Duration> for Time {

  /// Convert from a `Duration` to `Time`.
  fn from(duration: Duration) -> Self {
    const TIMEBASE: (i32, i32) = (1, 90000);

    let time = duration.as_secs_f64() * (TIMEBASE.1 as f64);
    let time = time.round() as i64;

    Self {
      time: Some(time),
      time_base: TIMEBASE.into(),
    }
  }

}

impl From<Time> for Duration {

  /// Convert from a `Time` to a Rust-native `Duration`.
  fn from(timestamp: Time) -> Self {
    if let Some(offset) = timestamp.time {
      let micros = offset
        // By rescaling into 1/1 million, we're essentially acquiring
        // the number of microseconds that is the offset of this timestamp.
        .rescale(timestamp.time_base, AvRational::new(1, 1_000_000))
        // Make sure it is a positive number.
        .max(0) as u64;

      Duration::from_micros(micros)
    } else {
      Duration::ZERO
    }
  }

}

/// Represents two timestampts that are aligned.
pub struct Aligned {
  lhs: Option<i64>,
  rhs: Option<i64>,
  time_base: AvRational,
}

impl Aligned {

  /// Add two timestamps together.
  pub fn add(self) -> Time {
    self.apply(|lhs, rhs| lhs + rhs)
  }

  /// Substract the right-hand side timestamp from the left-hand
  /// side timestamp.
  pub fn substract(self) -> Time {
    self.apply(|lhs, rhs| lhs - rhs)
  }

  /// Apply operation `f` on aligned timestamps.
  fn apply<F>(self, f: F) -> Time
  where
    F: FnOnce(i64, i64) -> i64
  {
    match (self.lhs, self.rhs) {
      (Some(lhs_time), Some(rhs_time)) => {
        Time {
          time: Some(f(lhs_time, rhs_time)),
          time_base: self.time_base,
        }
      },
      _ => {
        Time {
          time: None,
          time_base: self.time_base,
        }
      }
    }
  }

}