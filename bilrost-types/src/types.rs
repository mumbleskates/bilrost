use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use bilrost::{DistinguishedMessage, Message, Oneof};

/// A Duration represents a signed, fixed-length span of time represented as a count of seconds and
/// fractions of seconds at nanosecond resolution. It is independent of any calendar and concepts
/// like "day" or "month". It is related to Timestamp in that the difference between two Timestamp
/// values is a Duration.
///
/// Values of this type are not guaranteed to only exist in their normalized form.
///
/// # Examples
///
/// Example 1: Compute Duration from two Timestamps in pseudo code.
///
/// ```text
/// Timestamp start = ...;
/// Timestamp end = ...;
/// Duration duration = ...;
///
/// duration.seconds = end.seconds - start.seconds;
/// duration.nanos = end.nanos - start.nanos;
///
/// if (duration.seconds < 0 && duration.nanos > 0) {
///    duration.seconds += 1;
///    duration.nanos -= 1000000000;
/// } else if (duration.seconds > 0 && duration.nanos < 0) {
///    duration.seconds -= 1;
///    duration.nanos += 1000000000;
/// }
/// ```
///
/// Example 2: Compute Timestamp from Timestamp + Duration in pseudo code.
///
/// ```text
/// Timestamp start = ...;
/// Duration duration = ...;
/// Timestamp end = ...;
///
/// end.seconds = start.seconds + duration.seconds;
/// end.nanos = start.nanos + duration.nanos;
///
/// if (end.nanos < 0) {
///    end.seconds -= 1;
///    end.nanos += 1000000000;
/// } else if (end.nanos >= 1000000000) {
///    end.seconds += 1;
///    end.nanos -= 1000000000;
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Message, DistinguishedMessage)]
pub struct Duration {
    /// Signed seconds of the span of time. Must be from -315,576,000,000 to +315,576,000,000
    /// inclusive. Note: these bounds are computed from: 60 sec/min * 60 min/hr * 24 hr/day *
    /// 365.25 days/year * 10000 years
    #[bilrost(1)]
    pub seconds: i64,
    /// Signed fractions of a second at nanosecond resolution of the span of time. Durations less
    /// than one second are represented with a 0 `seconds` field and a positive or negative `nanos`
    /// field. For durations of one second or more, a non-zero value for the `nanos` field must be
    /// of the same sign as the `seconds` field. Must be from -999,999,999 to +999,999,999
    /// inclusive.
    #[bilrost(tag = 2, encoding = "fixed")]
    pub nanos: i32,
}

/// A Timestamp represents a point in time independent of any time zone or local calendar, encoded
/// as a count of seconds and fractions of seconds at nanosecond resolution. The count is relative
/// to an epoch at UTC midnight on January 1, 1970, in the proleptic Gregorian calendar which
/// extends the Gregorian calendar backwards indefinitely.
///
/// All minutes are 60 seconds long. Leap seconds are "smeared" so that no leap second table is
/// needed for interpretation, using a [24-hour linear smear](
/// <https://developers.google.com/time/smear>).
///
/// The range from 0001-01-01T00:00:00Z to 9999-12-31T23:59:59.999999999Z may be converted to and
/// from [RFC 3339](<https://www.ietf.org/rfc/rfc3339.txt>) date strings via the `Display` and
/// `FromStr` traits. Dates before and after those years are extended further, still in the
/// proleptic Gregorian calendar, in negative years or in positive years with more than 4 digits.
///
/// Values of this type are not guaranteed to only exist in their normalized form.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Message, DistinguishedMessage)]
pub struct Timestamp {
    /// Represents seconds of UTC time since Unix epoch 1970-01-01T00:00:00Z.
    #[bilrost(1)]
    pub seconds: i64,
    /// Non-negative fractions of a second at nanosecond resolution. Negative second values with
    /// fractions must still have non-negative nanos values that count forward in time. Must be from
    /// 0 to 999,999,999 inclusive.
    #[bilrost(tag = 2, encoding = "fixed")]
    pub nanos: i32,
}

impl Timestamp {
    pub const MIN: Self = Timestamp {
        seconds: i64::MIN,
        nanos: 0,
    };
    pub const MAX: Self = Timestamp {
        seconds: i64::MAX,
        nanos: 999999999,
    };
}

/// `Value` represents a dynamically typed JSON value which can be either null, a number (signed,
/// unsigned, or floating point in 64 bits), a string, a boolean, a string-keyed associative map of
/// other values, or a list of values.
#[derive(Clone, Debug, PartialEq, Oneof, Message)]
pub enum Value {
    /// Represents a JSON null value.
    Null,
    #[bilrost(1)]
    Float(f64),
    #[bilrost(2)]
    Signed(i64),
    #[bilrost(3)]
    Unsigned(u64),
    #[bilrost(4)]
    String(String),
    #[bilrost(5)]
    Bool(bool),
    /// Represents a structured value.
    #[bilrost(6)]
    Struct(StructValue),
    /// Represents a repeated `Value`.
    #[bilrost(7)]
    List(ListValue),
}

/// `StructValue` represents a structured data value analogous to a JSON object value.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct StructValue {
    /// Unordered map of dynamically typed values.
    #[bilrost(tag = 1, recurses)]
    pub fields: BTreeMap<String, Value>,
}

/// `ListValue` is a wrapper around a repeated list of values, analogous to a JSON list value.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct ListValue {
    /// Repeated field of dynamically typed values.
    #[bilrost(tag = 1, encoding = "packed", recurses)]
    pub values: Vec<Value>,
}
