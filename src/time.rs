extern crate ffmpeg_next as ffmpeg;

use std::time::Duration;

use ffmpeg::util::mathematics::rescale::{Rescale, TIME_BASE};
use ffmpeg::Rational as AvRational;

/// Represents a time or duration.
///
/// [`Time`] may represent a PTS (presentation timestamp), DTS (decoder timestamp) or a duration,
/// depending on the function that returns it.
///
/// [`Time`] may represent a non-existing time, in which case [`Time::has_value`] will return
/// `false`, and conversions to seconds will return `0.0`.
///
/// A [`Time`] object may be aligned with another [`Time`] object, which produces an [`Aligned`]
/// object, on which arithmetic operations can be performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Time {
    time: Option<i64>,
    time_base: AvRational,
}

impl Time {
    /// Create a new time by its time value and time base in which the time is expressed.
    ///
    /// # Arguments
    ///
    /// * `time` - Relative time in `time_base` units.
    /// * `time_base` - Time base of source.
    pub fn new(time: Option<i64>, time_base: AvRational) -> Time {
        Self { time, time_base }
    }

    /// Align the timestamp with a different time base.
    ///
    /// # Arguments
    ///
    /// # Return value
    ///
    /// The same timestamp, with the time base changed.
    #[inline]
    pub fn with_time_base(&self, time_base: AvRational) -> Self {
        self.aligned_with_rational(time_base)
    }

    /// Creates a new timestamp that reprsents `nth` of a second.
    ///
    /// # Arguments
    ///
    /// * `nth` - Denominator of the time in seconds as in `1 / nth`.
    pub fn from_nth_of_a_second(nth: usize) -> Self {
        Self {
            time: Some(1),
            time_base: AvRational::new(1, nth as i32),
        }
    }

    /// Creates a new timestamp from a number of seconds.
    ///
    /// # Arguments
    ///
    /// * `secs` - Number of seconds.
    pub fn from_secs(secs: f32) -> Self {
        Self {
            time: Some((secs * TIME_BASE.denominator() as f32).round() as i64),
            time_base: TIME_BASE,
        }
    }

    /// Creates a new timestamp from a number of seconds.
    ///
    /// # Arguments
    ///
    /// * `secs` - Number of seconds.
    pub fn from_secs_f64(secs: f64) -> Self {
        Self {
            time: Some((secs * TIME_BASE.denominator() as f64).round() as i64),
            time_base: TIME_BASE,
        }
    }

    /// Creates a new timestamp with `time` time units, each represents one / `base_den` seconds.
    ///
    /// # Arguments
    ///
    /// * `time` - Relative time in `time_base` units.
    /// * `base_den` - Time base denominator i.e. time base is `1 / base_den`.
    pub fn from_units(time: usize, base_den: usize) -> Self {
        Self {
            time: Some(time as i64),
            time_base: AvRational::new(1, base_den as i32),
        }
    }

    /// Create a new zero-valued timestamp.
    pub fn zero() -> Self {
        Time {
            time: Some(0),
            time_base: (1, 90000).into(),
        }
    }

    /// Whether or not the [`Time`] has a time at all.
    pub fn has_value(&self) -> bool {
        self.time.is_some()
    }

    /// Align the timestamp with another timestamp, which will convert the `rhs` timestamp to the
    /// same time base, such that operations can be performed upon the aligned timestamps.
    ///
    /// # Arguments
    ///
    /// * `rhs` - Right-hand side timestamp.
    ///
    /// # Return value
    ///
    /// Two timestamps that are aligned.
    pub fn aligned_with(&self, rhs: &Time) -> Aligned {
        Aligned {
            lhs: self.time,
            rhs: rhs
                .time
                .map(|rhs_time| rhs_time.rescale(rhs.time_base, self.time_base)),
            time_base: self.time_base,
        }
    }

    /// Get number of seconds as floating point value.
    pub fn as_secs(&self) -> f32 {
        if let Some(time) = self.time {
            (time as f32)
                * (self.time_base.numerator() as f32 / self.time_base.denominator() as f32)
        } else {
            0.0
        }
    }

    /// Get number of seconds as floating point value.
    pub fn as_secs_f64(&self) -> f64 {
        if let Some(time) = self.time {
            (time as f64)
                * (self.time_base.numerator() as f64 / self.time_base.denominator() as f64)
        } else {
            0.0
        }
    }

    /// Convert to underlying parts: the `time` and `time_base`.
    pub fn into_parts(self) -> (Option<i64>, AvRational) {
        (self.time, self.time_base)
    }

    /// Convert to underlying time to `i64` (the number of time units).
    ///
    /// # Safety
    ///
    /// Assumes that the caller knows the time base and applies it correctly when doing arithmetic
    /// operations on the time value.
    pub fn into_value(self) -> Option<i64> {
        self.time
    }

    /// Align the timestamp along another `time_base`.
    ///
    /// # Arguments
    ///
    /// * `time_base` - Target time base.
    pub(crate) fn aligned_with_rational(&self, time_base: AvRational) -> Time {
        Time {
            time: self
                .time
                .map(|time| time.rescale(self.time_base, time_base)),
            time_base,
        }
    }
}

impl From<Duration> for Time {
    /// Convert from a [`Duration`] to [`Time`].
    #[inline]
    fn from(duration: Duration) -> Self {
        Time::from_secs_f64(duration.as_secs_f64())
    }
}

impl From<Time> for Duration {
    /// Convert from a [`Time`] to a Rust-native [`Duration`].
    fn from(timestamp: Time) -> Self {
        Duration::from_secs_f64(timestamp.as_secs_f64())
    }
}

impl std::fmt::Display for Time {
    /// Format [`Time`] as follows:
    ///
    /// * If the inner value is not `None`: `time/time_base`.
    /// * If the inner value is `None`: `none`.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(time) = self.time {
            let num = self.time_base.numerator() as i64 * time;
            let den = self.time_base.denominator();
            write!(f, "{num}/{den} secs")
        } else {
            write!(f, "none")
        }
    }
}

/// This is a virtual object that represents two aligned times.
///
/// On this object, arthmetic operations can be performed that operate on the two contained times.
/// This virtual object ensures that the interface to these operations is safe.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Subtract the right-hand side timestamp from the left-hand side timestamp.
    pub fn subtract(self) -> Time {
        self.apply(|lhs, rhs| lhs - rhs)
    }

    /// Apply operation `f` on aligned timestamps.
    ///
    /// # Safety
    ///
    /// The closure operates on the numerator of two aligned times.
    ///
    /// # Arguments
    ///
    /// * `f` - Function to apply on the two aligned time numerator values.
    fn apply<F>(self, f: F) -> Time
    where
        F: FnOnce(i64, i64) -> i64,
    {
        match (self.lhs, self.rhs) {
            (Some(lhs_time), Some(rhs_time)) => Time {
                time: Some(f(lhs_time, rhs_time)),
                time_base: self.time_base,
            },
            _ => Time {
                time: None,
                time_base: self.time_base,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let time = Time::new(Some(2), AvRational::new(3, 9));
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 2.0 / 3.0);
        assert_eq!(time.into_value(), Some(2));
    }

    #[test]
    fn test_with_time_base() {
        let time = Time::new(Some(2), AvRational::new(3, 9));
        assert_eq!(time.as_secs(), 2.0 / 3.0);
        let time = time.with_time_base(AvRational::new(1, 9));
        assert_eq!(time.as_secs(), 2.0 / 3.0);
        assert_eq!(time.into_value(), Some(6));
    }

    #[test]
    fn test_from_nth_of_a_second() {
        let time = Time::from_nth_of_a_second(4);
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 0.25);
        assert_eq!(time.as_secs_f64(), 0.25);
        assert_eq!(Duration::from(time), Duration::from_millis(250));
    }

    #[test]
    fn test_from_secs() {
        let time = Time::from_secs(2.5);
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 2.5);
        assert_eq!(time.as_secs_f64(), 2.5);
        assert_eq!(Duration::from(time), Duration::from_millis(2500));
    }

    #[test]
    fn test_from_secs_f64() {
        let time = Time::from_secs(4.0);
        assert!(time.has_value());
        assert_eq!(time.as_secs_f64(), 4.0);
    }

    #[test]
    fn test_from_units() {
        let time = Time::from_units(3, 5);
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 3.0 / 5.0);
        assert_eq!(Duration::from(time), Duration::from_millis(600));
    }

    #[test]
    fn test_zero() {
        let time = Time::zero();
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 0.0);
        assert_eq!(time.as_secs_f64(), 0.0);
        assert_eq!(Duration::from(time), Duration::ZERO);
        let time = Time::zero();
        assert_eq!(time.into_value(), Some(0));
    }

    #[test]
    fn test_aligned_with() {
        let a = Time::from_units(3, 16);
        let b = Time::from_units(1, 8);
        let aligned = a.aligned_with(&b);
        assert_eq!(aligned.lhs, Some(3));
        assert_eq!(aligned.rhs, Some(2));
    }

    #[test]
    fn test_into_aligned_with() {
        let a = Time::from_units(2, 7);
        let b = Time::from_units(2, 3);
        let aligned = a.aligned_with(&b);
        assert_eq!(aligned.lhs, Some(2));
        assert_eq!(aligned.rhs, Some(5));
    }

    #[test]
    fn test_as_secs() {
        let time = Time::from_nth_of_a_second(4);
        assert_eq!(time.as_secs(), 0.25);
        let time = Time::from_secs(0.3);
        assert_eq!(time.as_secs(), 0.3);
        let time = Time::new(None, AvRational::new(0, 0));
        assert_eq!(time.as_secs(), 0.0);
    }

    #[test]
    fn test_as_secs_f64() {
        let time = Time::from_nth_of_a_second(4);
        assert_eq!(time.as_secs_f64(), 0.25);
        let time = Time::from_secs_f64(0.3);
        assert_eq!(time.as_secs_f64(), 0.3);
        let time = Time::new(None, AvRational::new(0, 0));
        assert_eq!(time.as_secs_f64(), 0.0);
    }

    #[test]
    fn test_into_parts() {
        let time = Time::new(Some(1), AvRational::new(2, 3));
        assert_eq!(time.into_parts(), (Some(1), AvRational::new(2, 3)));
    }

    #[test]
    fn test_into_value_none() {
        let time = Time::new(None, AvRational::new(0, 0));
        assert_eq!(time.into_value(), None);
    }

    #[test]
    fn test_add() {
        let a = Time::from_secs(0.2);
        let b = Time::from_secs(0.3);
        assert_eq!(a.aligned_with(&b).add(), Time::from_secs(0.5));
    }

    #[test]
    fn test_subtract() {
        let a = Time::from_secs(0.8);
        let b = Time::from_secs(0.4);
        assert_eq!(a.aligned_with(&b).subtract(), Time::from_secs(0.4));
    }

    #[test]
    fn test_apply() {
        let a = Time::from_secs(2.0);
        let b = Time::from_secs(0.25);
        assert_eq!(
            a.aligned_with(&b).apply(|x, y| (2 * x) + (3 * y)),
            Time::from_secs(4.75)
        );
    }

    #[test]
    fn test_apply_different_time_bases() {
        let a = Time::new(Some(3), AvRational::new(2, 32));
        let b = Time::from_nth_of_a_second(4);
        assert!(
            (a.aligned_with(&b).apply(|x, y| x + y).as_secs()
                - Time::from_secs(7.0 / 16.0).as_secs())
            .abs()
                < 0.001
        );
    }
}
