#![no_std]
#![doc(html_root_url = "https://docs.rs/bilrost-types/0.1005.0-dev")]

//! Analogs for protobuf well-known types, implemented alongside the
//! [`bilrost`][bilrost] crate. See that crate's documentation for details about the
//! library, and the [Protobuf reference][proto] for more information about the use cases and
//! semantics of these types.
//!
//! [bilrost]: https://docs.rs/bilrost
//!
//! [proto]: https://developers.google.com/protocol-buffers/docs/reference/google.protobuf

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod datetime;
mod types;

use core::fmt;
use core::str::FromStr;
use core::time;

pub use types::*;

// The Protobuf `Duration` and `Timestamp` types can't delegate to the standard library equivalents
// because the Protobuf versions are signed. To make them easier to work with, `From` conversions
// are defined in both directions.

const NANOS_PER_SECOND: i32 = 1_000_000_000;
const NANOS_MAX: i32 = NANOS_PER_SECOND - 1;

// TODO(widders): Message and into/from impls on time::Duration, time::Instant as optional features

impl core::ops::Neg for Duration {
    type Output = Self;

    fn neg(self) -> Self {
        Self {
            seconds: -self.seconds,
            nanos: -self.nanos,
        }
    }
}

// TODO(widders): addition and subtraction with Timestamp & Duration

impl Duration {
    /// Normalizes the duration to a canonical format.
    ///
    /// Based on [`google::protobuf::util::CreateNormalized`][1].
    ///
    /// [1]: https://github.com/google/protobuf/blob/v3.3.2/src/google/protobuf/util/time_util.cc#L79-L100
    pub fn normalize(&mut self) {
        // Make sure nanos is in the range.
        if self.nanos <= -NANOS_PER_SECOND || self.nanos >= NANOS_PER_SECOND {
            if let Some(seconds) = self
                .seconds
                .checked_add((self.nanos / NANOS_PER_SECOND) as i64)
            {
                self.seconds = seconds;
                self.nanos %= NANOS_PER_SECOND;
            } else if self.nanos < 0 {
                // Negative overflow! Set to the least normal value.
                self.seconds = i64::MIN;
                self.nanos = -NANOS_MAX;
            } else {
                // Positive overflow! Set to the greatest normal value.
                self.seconds = i64::MAX;
                self.nanos = NANOS_MAX;
            }
        }

        // nanos should have the same sign as seconds.
        if self.seconds < 0 && self.nanos > 0 {
            if let Some(seconds) = self.seconds.checked_add(1) {
                self.seconds = seconds;
                self.nanos -= NANOS_PER_SECOND;
            } else {
                // Positive overflow! Set to the greatest normal value.
                debug_assert_eq!(self.seconds, i64::MAX);
                self.nanos = NANOS_MAX;
            }
        } else if self.seconds > 0 && self.nanos < 0 {
            if let Some(seconds) = self.seconds.checked_sub(1) {
                self.seconds = seconds;
                self.nanos += NANOS_PER_SECOND;
            } else {
                // Negative overflow! Set to the least normal value.
                debug_assert_eq!(self.seconds, i64::MIN);
                self.nanos = -NANOS_MAX;
            }
        }
    }
}

impl TryFrom<time::Duration> for Duration {
    type Error = DurationError;

    /// Converts a `std::time::Duration` to a `Duration`, failing if the duration is too large.
    fn try_from(duration: time::Duration) -> Result<Duration, DurationError> {
        let seconds = i64::try_from(duration.as_secs()).map_err(|_| DurationError::OutOfRange)?;
        let nanos = duration.subsec_nanos() as i32;

        let mut duration = Duration { seconds, nanos };
        duration.normalize();
        Ok(duration)
    }
}

impl TryFrom<Duration> for time::Duration {
    type Error = DurationError;

    /// Converts a `Duration` to a `std::time::Duration`, failing if the duration is negative.
    fn try_from(mut duration: Duration) -> Result<time::Duration, DurationError> {
        duration.normalize();
        if duration.seconds >= 0 && duration.nanos >= 0 {
            Ok(time::Duration::new(
                duration.seconds as u64,
                duration.nanos as u32,
            ))
        } else {
            Err(DurationError::NegativeDuration(time::Duration::new(
                (-duration.seconds) as u64,
                (-duration.nanos) as u32,
            )))
        }
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = self.clone();
        d.normalize();
        if self.seconds < 0 && self.nanos < 0 {
            write!(f, "-")?;
        }
        write!(f, "{}", d.seconds.abs())?;

        // Format subseconds to either nothing, millis, micros, or nanos.
        let nanos = d.nanos.abs();
        if nanos == 0 {
            write!(f, "s")
        } else if nanos % 1_000_000 == 0 {
            write!(f, ".{:03}s", nanos / 1_000_000)
        } else if nanos % 1_000 == 0 {
            write!(f, ".{:06}s", nanos / 1_000)
        } else {
            write!(f, ".{:09}s", nanos)
        }
    }
}

/// A duration handling error.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum DurationError {
    /// Indicates failure to parse a [`Duration`] from a string.
    ///
    /// The [`Duration`] string format is specified in the [Protobuf JSON mapping specification][1].
    ///
    /// [1]: https://developers.google.com/protocol-buffers/docs/proto3#json
    ParseFailure,

    /// Indicates failure to convert a `bilrost_types::Duration` to a `std::time::Duration` because
    /// the duration is negative. The included `std::time::Duration` matches the magnitude of the
    /// original negative `bilrost_types::Duration`.
    NegativeDuration(time::Duration),

    /// Indicates failure to convert a `std::time::Duration` to a `bilrost_types::Duration`.
    ///
    /// Converting a `std::time::Duration` to a `bilrost_types::Duration` fails if the magnitude
    /// exceeds that representable by `bilrost_types::Duration`.
    OutOfRange,
}

impl fmt::Display for DurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DurationError::ParseFailure => write!(f, "failed to parse duration"),
            DurationError::NegativeDuration(duration) => {
                write!(f, "failed to convert negative duration: {:?}", duration)
            }
            DurationError::OutOfRange => {
                write!(f, "failed to convert duration out of range")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DurationError {}

impl FromStr for Duration {
    type Err = DurationError;

    fn from_str(s: &str) -> Result<Duration, DurationError> {
        datetime::parse_duration(s).ok_or(DurationError::ParseFailure)
    }
}

impl Timestamp {
    /// Normalizes the timestamp to a canonical format.
    ///
    /// Based on [`google::protobuf::util::CreateNormalized`][1].
    ///
    /// [1]: https://github.com/google/protobuf/blob/v3.3.2/src/google/protobuf/util/time_util.cc#L59-L77
    pub fn normalize(&mut self) {
        // Make sure nanos is in the range.
        if self.nanos <= -NANOS_PER_SECOND || self.nanos >= NANOS_PER_SECOND {
            if let Some(seconds) = self
                .seconds
                .checked_add((self.nanos / NANOS_PER_SECOND) as i64)
            {
                self.seconds = seconds;
                self.nanos %= NANOS_PER_SECOND;
            } else if self.nanos < 0 {
                // Negative overflow! Set to the earliest normal value.
                self.seconds = i64::MIN;
                self.nanos = 0;
            } else {
                // Positive overflow! Set to the latest normal value.
                self.seconds = i64::MAX;
                self.nanos = 999_999_999;
            }
        }

        // For Timestamp nanos should be in the range [0, 999999999].
        if self.nanos < 0 {
            if let Some(seconds) = self.seconds.checked_sub(1) {
                self.seconds = seconds;
                self.nanos += NANOS_PER_SECOND;
            } else {
                // Negative overflow! Set to the earliest normal value.
                debug_assert_eq!(self.seconds, i64::MIN);
                self.nanos = 0;
            }
        }
    }

    /// Normalizes the timestamp to a canonical format, returning the original value if it cannot be
    /// normalized.
    ///
    /// Normalization is based on [`google::protobuf::util::CreateNormalized`][1].
    ///
    /// [1]: https://github.com/google/protobuf/blob/v3.3.2/src/google/protobuf/util/time_util.cc#L59-L77
    pub fn try_normalize(mut self) -> Result<Timestamp, Timestamp> {
        let before = self.clone();
        self.normalize();
        // If the seconds value has changed, and is either i64::MIN or i64::MAX, then the timestamp
        // normalization overflowed.
        if (self.seconds == i64::MAX || self.seconds == i64::MIN) && self.seconds != before.seconds
        {
            Err(before)
        } else {
            Ok(self)
        }
    }

    /// Creates a new `Timestamp` at the start of the provided UTC date.
    pub fn date(year: i64, month: u8, day: u8) -> Result<Timestamp, TimestampError> {
        Timestamp::date_time_nanos(year, month, day, 0, 0, 0, 0)
    }

    /// Creates a new `Timestamp` instance with the provided UTC date and time.
    pub fn date_time(
        year: i64,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    ) -> Result<Timestamp, TimestampError> {
        Timestamp::date_time_nanos(year, month, day, hour, minute, second, 0)
    }

    /// Creates a new `Timestamp` instance with the provided UTC date and time.
    pub fn date_time_nanos(
        year: i64,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        nanos: u32,
    ) -> Result<Timestamp, TimestampError> {
        let date_time = datetime::DateTime {
            year,
            month,
            day,
            hour,
            minute,
            second,
            nanos,
        };

        if date_time.is_valid() {
            Ok(Timestamp::from(date_time))
        } else {
            Err(TimestampError::InvalidDateTime)
        }
    }
}

#[cfg(feature = "std")]
impl From<std::time::SystemTime> for Timestamp {
    fn from(system_time: std::time::SystemTime) -> Timestamp {
        let (seconds, nanos) = match system_time.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => {
                let seconds = i64::try_from(duration.as_secs()).unwrap();
                (seconds, duration.subsec_nanos() as i32)
            }
            Err(error) => {
                let duration = error.duration();
                let seconds = i64::try_from(duration.as_secs()).unwrap();
                let nanos = duration.subsec_nanos() as i32;
                if nanos == 0 {
                    (-seconds, 0)
                } else {
                    (-seconds - 1, 1_000_000_000 - nanos)
                }
            }
        };
        Timestamp { seconds, nanos }
    }
}

/// A timestamp handling error.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum TimestampError {
    /// Indicates that a [`Timestamp`] could not be converted to
    /// [`SystemTime`][std::time::SystemTime] because it is out of range.
    ///
    /// The range of times that can be represented by `SystemTime` depends on the platform. All
    /// `Timestamp`s are likely representable on 64-bit Unix-like platforms, but other platforms,
    /// such as Windows and 32-bit Linux, may not be able to represent the full range of
    /// `Timestamp`s.
    OutOfSystemRange(Timestamp),

    /// An error indicating failure to parse a timestamp in RFC-3339 format.
    ParseFailure,

    /// Indicates an error when constructing a timestamp due to invalid date or time data.
    InvalidDateTime,
}

impl fmt::Display for TimestampError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimestampError::OutOfSystemRange(timestamp) => {
                write!(
                    f,
                    "{} is not representable as a `SystemTime` because it is out of range",
                    timestamp
                )
            }
            TimestampError::ParseFailure => {
                write!(f, "failed to parse RFC-3339 formatted timestamp")
            }
            TimestampError::InvalidDateTime => {
                write!(f, "invalid date or time")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for TimestampError {}

#[cfg(feature = "std")]
impl TryFrom<Timestamp> for std::time::SystemTime {
    type Error = TimestampError;

    fn try_from(mut timestamp: Timestamp) -> Result<std::time::SystemTime, Self::Error> {
        let orig_timestamp = timestamp.clone();
        timestamp.normalize();

        let system_time = if timestamp.seconds >= 0 {
            std::time::UNIX_EPOCH.checked_add(time::Duration::from_secs(timestamp.seconds as u64))
        } else {
            std::time::UNIX_EPOCH.checked_sub(time::Duration::from_secs(
                timestamp
                    .seconds
                    .checked_neg()
                    .ok_or_else(|| TimestampError::OutOfSystemRange(timestamp.clone()))?
                    as u64,
            ))
        };

        let system_time = system_time.and_then(|system_time| {
            system_time.checked_add(time::Duration::from_nanos(timestamp.nanos as u64))
        });

        system_time.ok_or(TimestampError::OutOfSystemRange(orig_timestamp))
    }
}

impl FromStr for Timestamp {
    type Err = TimestampError;

    fn from_str(s: &str) -> Result<Timestamp, TimestampError> {
        datetime::parse_timestamp(s).ok_or(TimestampError::ParseFailure)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        datetime::DateTime::from(self.clone()).fmt(f)
    }
}

#[cfg(feature = "serde_json")]
impl From<serde_json::Value> for Value {
    fn from(from: serde_json::Value) -> Self {
        use value::Kind::*;
        Value {
            kind: match from {
                serde_json::Value::Null => Null,
                serde_json::Value::Bool(value) => Bool(value),
                serde_json::Value::Number(number) => {
                    if number.is_i64() {
                        Signed(number.as_i64().unwrap())
                    } else if number.is_u64() {
                        Unsigned(number.as_u64().unwrap())
                    } else {
                        Float(number.as_f64().unwrap())
                    }
                }
                serde_json::Value::String(value) => String(value),
                serde_json::Value::Array(values) => List(ListValue {
                    values: values.into_iter().map(Into::into).collect(),
                }),
                serde_json::Value::Object(items) => Struct(StructValue {
                    fields: items
                        .into_iter()
                        .map(|(key, value)| (key, value.into()))
                        .collect(),
                }),
            },
        }
    }
}

#[cfg(feature = "serde_json")]
impl TryFrom<Value> for serde_json::Value {
    type Error = ();

    fn try_from(value: Value) -> Result<Self, ()> {
        Ok(match value.kind {
            value::Kind::Null => serde_json::Value::Null,
            value::Kind::Float(value) => {
                serde_json::Value::Number(serde_json::Number::from_f64(value).ok_or(())?)
            }
            value::Kind::Signed(value) => {
                serde_json::Value::Number(serde_json::Number::from(value))
            }
            value::Kind::Unsigned(value) => {
                serde_json::Value::Number(serde_json::Number::from(value))
            }
            value::Kind::String(value) => serde_json::Value::String(value),
            value::Kind::Bool(value) => serde_json::Value::Bool(value),
            value::Kind::Struct(items) => serde_json::Value::Object(
                items
                    .fields
                    .into_iter()
                    .map(|(key, value)| Ok((key, value.try_into()?)))
                    .collect::<Result<_, _>>()?,
            ),
            value::Kind::List(list) => serde_json::Value::Array(
                list.values
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "std")]
    use ::{
        alloc::format,
        proptest::prelude::*,
        std::time::{self, SystemTime, UNIX_EPOCH},
    };

    use crate::datetime::DateTime;

    #[test]
    fn check_overflowing_datetimes() {
        // These DateTimes cause overflows and crashes in the prost crate.
        assert_eq!(
            Timestamp::from(DateTime {
                year: i64::from_le_bytes([178, 2, 0, 0, 0, 0, 0, 128]),
                month: 2,
                day: 2,
                hour: 8,
                minute: 58,
                second: 8,
                nanos: u32::from_le_bytes([0, 0, 0, 50]),
            }),
            Timestamp::MIN
        );
        assert_eq!(
            Timestamp::from(DateTime {
                year: i64::from_le_bytes([132, 7, 0, 0, 0, 0, 0, 128]),
                month: 2,
                day: 2,
                hour: 8,
                minute: 58,
                second: 8,
                nanos: u32::from_le_bytes([0, 0, 0, 50]),
            }),
            Timestamp::MIN
        );
        assert_eq!(
            Timestamp::from(DateTime {
                year: i64::from_le_bytes([80, 96, 32, 240, 99, 0, 32, 180]),
                month: 1,
                day: 18,
                hour: 19,
                minute: 26,
                second: 8,
                nanos: u32::from_le_bytes([0, 0, 0, 50]),
            }),
            Timestamp::MIN
        );
        assert_eq!(
            Timestamp::from(DateTime {
                year: DateTime::MIN.year - 1,
                month: 0,
                day: 0,
                hour: 0,
                minute: 0,
                second: 0,
                nanos: 0,
            }),
            Timestamp::MIN
        );
        assert_eq!(
            Timestamp::from(DateTime {
                year: i64::MIN,
                month: 0,
                day: 0,
                hour: 0,
                minute: 0,
                second: 0,
                nanos: 0,
            }),
            Timestamp::MIN
        );
        assert_eq!(
            Timestamp::from(DateTime {
                year: DateTime::MAX.year + 1,
                month: u8::MAX,
                day: u8::MAX,
                hour: u8::MAX,
                minute: u8::MAX,
                second: u8::MAX,
                nanos: u32::MAX,
            }),
            Timestamp::MAX
        );
        assert_eq!(
            Timestamp::from(DateTime {
                year: i64::MAX,
                month: u8::MAX,
                day: u8::MAX,
                hour: u8::MAX,
                minute: u8::MAX,
                second: u8::MAX,
                nanos: u32::MAX,
            }),
            Timestamp::MAX
        );
        assert_eq!(Timestamp::from(DateTime::MIN), Timestamp::MIN);
        assert_eq!(Timestamp::from(DateTime::MAX), Timestamp::MAX);
        assert_eq!(DateTime::from(Timestamp::MIN), DateTime::MIN);
        assert_eq!(DateTime::from(Timestamp::MAX), DateTime::MAX);
    }

    #[cfg(feature = "std")]
    proptest! {
        #[test]
        fn check_system_time_roundtrip(
            system_time in SystemTime::arbitrary(),
        ) {
            prop_assert_eq!(SystemTime::try_from(Timestamp::from(system_time)).unwrap(), system_time);
        }

        #[test]
        fn check_timestamp_roundtrip_via_system_time(
            seconds in i64::arbitrary(),
            nanos in i32::arbitrary(),
        ) {
            let mut timestamp = Timestamp { seconds, nanos };
            timestamp.normalize();
            if let Ok(system_time) = SystemTime::try_from(timestamp.clone()) {
                prop_assert_eq!(Timestamp::from(system_time), timestamp);
            }
        }

        #[test]
        fn check_timestamp_datetime_roundtrip(seconds: i64, nanos: i32) {
            let mut timestamp = Timestamp { seconds, nanos };
            timestamp.normalize();
            let timestamp = timestamp;
            let datetime: DateTime = timestamp.clone().into();
            prop_assert_eq!(Timestamp::from(datetime), timestamp);
        }

        #[test]
        fn check_duration_roundtrip(
            seconds in u64::arbitrary(),
            nanos in 0u32..1_000_000_000u32,
        ) {
            let std_duration = time::Duration::new(seconds, nanos);
            let bilrost_duration = match Duration::try_from(std_duration) {
                Ok(duration) => duration,
                Err(_) => return Err(TestCaseError::reject("duration out of range")),
            };
            prop_assert_eq!(time::Duration::try_from(bilrost_duration.clone()).unwrap(), std_duration);

            if std_duration != time::Duration::default() {
                let neg_prost_duration = Duration {
                    seconds: -bilrost_duration.seconds,
                    nanos: -bilrost_duration.nanos,
                };

                prop_assert!(
                    matches!(
                        time::Duration::try_from(neg_prost_duration),
                        Err(DurationError::NegativeDuration(d)) if d == std_duration,
                    )
                )
            }
        }

        #[test]
        fn check_duration_roundtrip_nanos(
            nanos in u32::arbitrary(),
        ) {
            let seconds = 0;
            let std_duration = std::time::Duration::new(seconds, nanos);
            let bilrost_duration = match Duration::try_from(std_duration) {
                Ok(duration) => duration,
                Err(_) => return Err(TestCaseError::reject("duration out of range")),
            };
            prop_assert_eq!(time::Duration::try_from(bilrost_duration.clone()).unwrap(), std_duration);

            if std_duration != time::Duration::default() {
                let neg_prost_duration = Duration {
                    seconds: -bilrost_duration.seconds,
                    nanos: -bilrost_duration.nanos,
                };

                prop_assert!(
                    matches!(
                        time::Duration::try_from(neg_prost_duration),
                        Err(DurationError::NegativeDuration(d)) if d == std_duration,
                    )
                )
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn check_duration_try_from_negative_nanos() {
        let seconds: u64 = 0;
        let nanos: u32 = 1;
        let std_duration = std::time::Duration::new(seconds, nanos);

        let neg_prost_duration = Duration {
            seconds: 0,
            nanos: -1,
        };

        assert!(matches!(
           time::Duration::try_from(neg_prost_duration),
           Err(DurationError::NegativeDuration(d)) if d == std_duration,
        ))
    }

    #[cfg(feature = "std")]
    #[test]
    fn check_timestamp_negative_seconds() {
        // Representative tests for the case of timestamps before the UTC Epoch time:
        // validate the expected behaviour that "negative second values with fractions
        // must still have non-negative nanos values that count forward in time"
        // https://developers.google.com/protocol-buffers/docs/reference/google.protobuf#google.protobuf.Timestamp
        //
        // To ensure cross-platform compatibility, all nanosecond values in these
        // tests are in minimum 100 ns increments.  This does not affect the general
        // character of the behaviour being tested, but ensures that the tests are
        // valid for both POSIX (1 ns precision) and Windows (100 ns precision).
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(1_001, 0)),
            Timestamp {
                seconds: -1_001,
                nanos: 0,
            }
        );
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(0, 999_999_900)),
            Timestamp {
                seconds: -1,
                nanos: 100,
            }
        );
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(2_001_234, 12_300)),
            Timestamp {
                seconds: -2_001_235,
                nanos: 999_987_700,
            }
        );
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(768, 65_432_100)),
            Timestamp {
                seconds: -769,
                nanos: 934_567_900,
            }
        );
    }

    #[cfg(all(unix, feature = "std"))]
    #[test]
    fn check_timestamp_negative_seconds_1ns() {
        // UNIX-only test cases with 1 ns precision
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(0, 999_999_999)),
            Timestamp {
                seconds: -1,
                nanos: 1,
            }
        );
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(1_234_567, 123)),
            Timestamp {
                seconds: -1_234_568,
                nanos: 999_999_877,
            }
        );
        assert_eq!(
            Timestamp::from(UNIX_EPOCH - time::Duration::new(890, 987_654_321)),
            Timestamp {
                seconds: -891,
                nanos: 12_345_679,
            }
        );
    }

    #[test]
    fn check_duration_normalize() {
        #[rustfmt::skip] // Don't mangle the table formatting.
        let cases = [
            // --- Table of test cases ---
            //        test seconds      test nanos  expected seconds  expected nanos
            (line!(),            0,              0,                0,              0),
            (line!(),            1,              1,                1,              1),
            (line!(),           -1,             -1,               -1,             -1),
            (line!(),            0,    999_999_999,                0,    999_999_999),
            (line!(),            0,   -999_999_999,                0,   -999_999_999),
            (line!(),            0,  1_000_000_000,                1,              0),
            (line!(),            0, -1_000_000_000,               -1,              0),
            (line!(),            0,  1_000_000_001,                1,              1),
            (line!(),            0, -1_000_000_001,               -1,             -1),
            (line!(),           -1,              1,                0,   -999_999_999),
            (line!(),            1,             -1,                0,    999_999_999),
            (line!(),           -1,  1_000_000_000,                0,              0),
            (line!(),            1, -1_000_000_000,                0,              0),
            (line!(), i64::MIN    ,              0,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1,              0,     i64::MIN + 1,              0),
            (line!(), i64::MIN    ,              1,     i64::MIN + 1,   -999_999_999),
            (line!(), i64::MIN    ,  1_000_000_000,     i64::MIN + 1,              0),
            (line!(), i64::MIN    , -1_000_000_000,     i64::MIN    ,   -999_999_999),
            (line!(), i64::MIN + 1, -1_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN + 2, -1_000_000_000,     i64::MIN + 1,              0),
            (line!(), i64::MIN    , -1_999_999_998,     i64::MIN    ,   -999_999_999),
            (line!(), i64::MIN + 1, -1_999_999_998,     i64::MIN    ,   -999_999_998),
            (line!(), i64::MIN + 2, -1_999_999_998,     i64::MIN + 1,   -999_999_998),
            (line!(), i64::MIN    , -1_999_999_999,     i64::MIN    ,   -999_999_999),
            (line!(), i64::MIN + 1, -1_999_999_999,     i64::MIN    ,   -999_999_999),
            (line!(), i64::MIN + 2, -1_999_999_999,     i64::MIN + 1,   -999_999_999),
            (line!(), i64::MIN    , -2_000_000_000,     i64::MIN    ,   -999_999_999),
            (line!(), i64::MIN + 1, -2_000_000_000,     i64::MIN    ,   -999_999_999),
            (line!(), i64::MIN + 2, -2_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN    ,   -999_999_998,     i64::MIN    ,   -999_999_998),
            (line!(), i64::MIN + 1,   -999_999_998,     i64::MIN + 1,   -999_999_998),
            (line!(), i64::MAX    ,              0,     i64::MAX    ,              0),
            (line!(), i64::MAX - 1,              0,     i64::MAX - 1,              0),
            (line!(), i64::MAX    ,             -1,     i64::MAX - 1,    999_999_999),
            (line!(), i64::MAX    ,  1_000_000_000,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  1_000_000_000,     i64::MAX    ,              0),
            (line!(), i64::MAX - 2,  1_000_000_000,     i64::MAX - 1,              0),
            (line!(), i64::MAX    ,  1_999_999_998,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  1_999_999_998,     i64::MAX    ,    999_999_998),
            (line!(), i64::MAX - 2,  1_999_999_998,     i64::MAX - 1,    999_999_998),
            (line!(), i64::MAX    ,  1_999_999_999,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  1_999_999_999,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 2,  1_999_999_999,     i64::MAX - 1,    999_999_999),
            (line!(), i64::MAX    ,  2_000_000_000,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  2_000_000_000,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 2,  2_000_000_000,     i64::MAX    ,              0),
            (line!(), i64::MAX    ,    999_999_998,     i64::MAX    ,    999_999_998),
            (line!(), i64::MAX - 1,    999_999_998,     i64::MAX - 1,    999_999_998),
        ];

        for case in cases.iter() {
            let mut test_duration = Duration {
                seconds: case.1,
                nanos: case.2,
            };
            test_duration.normalize();

            assert_eq!(
                test_duration,
                Duration {
                    seconds: case.3,
                    nanos: case.4,
                },
                "test case on line {} doesn't match",
                case.0,
            );
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn check_timestamp_normalize() {
        // Make sure that `Timestamp::normalize` behaves correctly on and near overflow.
        #[rustfmt::skip] // Don't mangle the table formatting.
        let cases = [
            // --- Table of test cases ---
            //        test seconds      test nanos  expected seconds  expected nanos
            (line!(),            0,              0,                0,              0),
            (line!(),            1,              1,                1,              1),
            (line!(),           -1,             -1,               -2,    999_999_999),
            (line!(),            0,    999_999_999,                0,    999_999_999),
            (line!(),            0,   -999_999_999,               -1,              1),
            (line!(),            0,  1_000_000_000,                1,              0),
            (line!(),            0, -1_000_000_000,               -1,              0),
            (line!(),            0,  1_000_000_001,                1,              1),
            (line!(),            0, -1_000_000_001,               -2,    999_999_999),
            (line!(),           -1,              1,               -1,              1),
            (line!(),            1,             -1,                0,    999_999_999),
            (line!(),           -1,  1_000_000_000,                0,              0),
            (line!(),            1, -1_000_000_000,                0,              0),
            (line!(), i64::MIN    ,              0,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1,              0,     i64::MIN + 1,              0),
            (line!(), i64::MIN    ,              1,     i64::MIN    ,              1),
            (line!(), i64::MIN    ,  1_000_000_000,     i64::MIN + 1,              0),
            (line!(), i64::MIN    , -1_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1, -1_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN + 2, -1_000_000_000,     i64::MIN + 1,              0),
            (line!(), i64::MIN    , -1_999_999_998,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1, -1_999_999_998,     i64::MIN    ,              0),
            (line!(), i64::MIN + 2, -1_999_999_998,     i64::MIN    ,              2),
            (line!(), i64::MIN    , -1_999_999_999,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1, -1_999_999_999,     i64::MIN    ,              0),
            (line!(), i64::MIN + 2, -1_999_999_999,     i64::MIN    ,              1),
            (line!(), i64::MIN    , -2_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1, -2_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN + 2, -2_000_000_000,     i64::MIN    ,              0),
            (line!(), i64::MIN    ,   -999_999_998,     i64::MIN    ,              0),
            (line!(), i64::MIN + 1,   -999_999_998,     i64::MIN    ,              2),
            (line!(), i64::MAX    ,              0,     i64::MAX    ,              0),
            (line!(), i64::MAX - 1,              0,     i64::MAX - 1,              0),
            (line!(), i64::MAX    ,             -1,     i64::MAX - 1,    999_999_999),
            (line!(), i64::MAX    ,  1_000_000_000,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  1_000_000_000,     i64::MAX    ,              0),
            (line!(), i64::MAX - 2,  1_000_000_000,     i64::MAX - 1,              0),
            (line!(), i64::MAX    ,  1_999_999_998,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  1_999_999_998,     i64::MAX    ,    999_999_998),
            (line!(), i64::MAX - 2,  1_999_999_998,     i64::MAX - 1,    999_999_998),
            (line!(), i64::MAX    ,  1_999_999_999,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  1_999_999_999,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 2,  1_999_999_999,     i64::MAX - 1,    999_999_999),
            (line!(), i64::MAX    ,  2_000_000_000,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 1,  2_000_000_000,     i64::MAX    ,    999_999_999),
            (line!(), i64::MAX - 2,  2_000_000_000,     i64::MAX    ,              0),
            (line!(), i64::MAX    ,    999_999_998,     i64::MAX    ,    999_999_998),
            (line!(), i64::MAX - 1,    999_999_998,     i64::MAX - 1,    999_999_998),
        ];

        for case in cases.iter() {
            let mut test_timestamp = crate::Timestamp {
                seconds: case.1,
                nanos: case.2,
            };
            test_timestamp.normalize();

            assert_eq!(
                test_timestamp,
                crate::Timestamp {
                    seconds: case.3,
                    nanos: case.4,
                },
                "test case on line {} doesn't match",
                case.0,
            );
        }
    }
}
