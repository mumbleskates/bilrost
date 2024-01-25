use bilrost::alloc::collections::BTreeMap;
use bilrost::alloc::string::String;
use bilrost::alloc::vec::Vec;
use bilrost::Message;

/// A Duration represents a signed, fixed-length span of time represented
/// as a count of seconds and fractions of seconds at nanosecond
/// resolution. It is independent of any calendar and concepts like "day"
/// or "month". It is related to Timestamp in that the difference between
/// two Timestamp values is a Duration and it can be added or subtracted
/// from a Timestamp. Range is approximately +-10,000 years.
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
///
/// Example 3: Compute Duration from datetime.timedelta in Python.
///
/// ```text
/// td = datetime.timedelta(days=3, minutes=10)
/// duration = Duration()
/// duration.FromTimedelta(td)
/// ```
///
/// # JSON Mapping
///
/// In JSON format, the Duration type is encoded as a string rather than an
/// object, where the string ends in the suffix "s" (indicating seconds) and
/// is preceded by the number of seconds, with nanoseconds expressed as
/// fractional seconds. For example, 3 seconds with 0 nanoseconds should be
/// encoded in JSON format as "3s", while 3 seconds and 1 nanosecond should
/// be expressed in JSON format as "3.000000001s", and 3 seconds and 1
/// microsecond should be expressed in JSON format as "3.000001s".
#[derive(Clone, Debug, PartialEq, PartialOrd, Message)]
pub struct Duration {
    /// Signed seconds of the span of time. Must be from -315,576,000,000
    /// to +315,576,000,000 inclusive. Note: these bounds are computed from:
    /// 60 sec/min * 60 min/hr * 24 hr/day * 365.25 days/year * 10000 years
    #[bilrost(1)]
    pub seconds: i64,
    /// Signed fractions of a second at nanosecond resolution of the span
    /// of time. Durations less than one second are represented with a 0
    /// `seconds` field and a positive or negative `nanos` field. For durations
    /// of one second or more, a non-zero value for the `nanos` field must be
    /// of the same sign as the `seconds` field. Must be from -999,999,999
    /// to +999,999,999 inclusive.
    #[bilrost(tag = 2, encoder = "fixed")]
    pub nanos: i32,
}

/// `Struct` represents a structured data value, consisting of fields
/// which map to dynamically typed values. In some languages, `Struct`
/// might be supported by a native representation. For example, in
/// scripting languages like JS a struct is represented as an
/// object. The details of that representation are described together
/// with the proto support for the language.
///
/// The JSON representation for `Struct` is JSON object.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct Struct {
    /// Unordered map of dynamically typed values.
    #[bilrost(1)]
    pub fields: BTreeMap<String, Value>,
}

/// `Value` represents a dynamically typed value which can be either
/// null, a number, a string, a boolean, a recursive struct value, or a
/// list of values. A producer of value is expected to set one of these
/// variants. Absence of any variant indicates an error.
///
/// The JSON representation for `Value` is JSON value.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct Value {
    /// The kind of value. None represents JSON `null`.
    #[bilrost(oneof(1, 2, 3, 4, 5))]
    pub kind: value::Kind,
}

/// Nested message and enum types in `Value`.
pub mod value {
    use super::String;

    /// The kind of value.
    #[derive(Clone, Debug, PartialEq, bilrost::Oneof)]
    pub enum Kind {
        /// Represents a JSON null value.
        Null,
        /// Represents a float64 value.
        #[bilrost(1)]
        NumberValue(f64),
        /// Represents a string value.
        #[bilrost(2)]
        StringValue(String),
        /// Represents a boolean value.
        #[bilrost(3)]
        BoolValue(bool),
        /// Represents a structured value.
        #[bilrost(4)]
        StructValue(super::Struct),
        /// Represents a repeated `Value`.
        #[bilrost(5)]
        ListValue(super::ListValue),
    }
}

/// `ListValue` is a wrapper around a repeated field of values.
///
/// The JSON representation for `ListValue` is JSON array.
#[derive(Clone, Debug, PartialEq, Message)]
pub struct ListValue {
    /// Repeated field of dynamically typed values.
    #[bilrost(tag = 1, encoder = "packed")]
    pub values: Vec<Value>,
}

/// A Timestamp represents a point in time independent of any time zone or local
/// calendar, encoded as a count of seconds and fractions of seconds at
/// nanosecond resolution. The count is relative to an epoch at UTC midnight on
/// January 1, 1970, in the proleptic Gregorian calendar which extends the
/// Gregorian calendar backwards to year one.
///
/// All minutes are 60 seconds long. Leap seconds are "smeared" so that no leap
/// second table is needed for interpretation, using a [24-hour linear
/// smear](<https://developers.google.com/time/smear>).
///
/// The range is from 0001-01-01T00:00:00Z to 9999-12-31T23:59:59.999999999Z. By
/// restricting to that range, we ensure that we can convert to and from [RFC
/// 3339](<https://www.ietf.org/rfc/rfc3339.txt>) date strings.
///
/// # Examples
///
/// Example 1: Compute Timestamp from POSIX `time()`.
///
/// ```text
/// Timestamp timestamp;
/// timestamp.set_seconds(time(NULL));
/// timestamp.set_nanos(0);
/// ```
///
/// Example 2: Compute Timestamp from POSIX `gettimeofday()`.
///
/// ```text
/// struct timeval tv;
/// gettimeofday(&tv, NULL);
///
/// Timestamp timestamp;
/// timestamp.set_seconds(tv.tv_sec);
/// timestamp.set_nanos(tv.tv_usec * 1000);
/// ```
///
/// Example 3: Compute Timestamp from Win32 `GetSystemTimeAsFileTime()`.
///
/// ```text
/// FILETIME ft;
/// GetSystemTimeAsFileTime(&ft);
/// UINT64 ticks = (((UINT64)ft.dwHighDateTime) << 32) | ft.dwLowDateTime;
///
/// // A Windows tick is 100 nanoseconds. Windows epoch 1601-01-01T00:00:00Z
/// // is 11644473600 seconds before Unix epoch 1970-01-01T00:00:00Z.
/// Timestamp timestamp;
/// timestamp.set_seconds((INT64) ((ticks / 10000000) - 11644473600LL));
/// timestamp.set_nanos((INT32) ((ticks % 10000000) * 100));
/// ```
///
/// Example 4: Compute Timestamp from Java `System.currentTimeMillis()`.
///
/// ```text
/// long millis = System.currentTimeMillis();
///
/// Timestamp timestamp = Timestamp.newBuilder().setSeconds(millis / 1000)
///      .setNanos((int) ((millis % 1000) * 1000000)).build();
/// ```
///
/// Example 5: Compute Timestamp from Java `Instant.now()`.
///
/// ```text
/// Instant now = Instant.now();
///
/// Timestamp timestamp =
///      Timestamp.newBuilder().setSeconds(now.getEpochSecond())
///          .setNanos(now.getNano()).build();
/// ```
///
/// Example 6: Compute Timestamp from current time in Python.
///
/// ```text
/// timestamp = Timestamp()
/// timestamp.GetCurrentTime()
/// ```
///
/// # JSON Mapping
///
/// In JSON format, the Timestamp type is encoded as a string in the
/// [RFC 3339](<https://www.ietf.org/rfc/rfc3339.txt>) format. That is, the
/// format is "{year}-{month}-{day}T{hour}:{min}:{sec}\[.{frac_sec}\]Z"
/// where {year} is always expressed using four digits while {month}, {day},
/// {hour}, {min}, and {sec} are zero-padded to two digits each. The fractional
/// seconds, which can go up to 9 digits (i.e. up to 1 nanosecond resolution),
/// are optional. The "Z" suffix indicates the timezone ("UTC"); the timezone
/// is required. A proto3 JSON serializer should always use UTC (as indicated by
/// "Z") when printing the Timestamp type and a proto3 JSON parser should be
/// able to accept both UTC and other timezones (as indicated by an offset).
///
/// For example, "2017-01-15T01:30:15.01Z" encodes 15.01 seconds past
/// 01:30 UTC on January 15, 2017.
///
/// In JavaScript, one can convert a Date object to this format using the
/// standard
/// [toISOString()](<https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date/toISOString>)
/// method. In Python, a standard `datetime.datetime` object can be converted
/// to this format using
/// [`strftime`](<https://docs.python.org/2/library/time.html#time.strftime>) with
/// the time format spec '%Y-%m-%dT%H:%M:%S.%fZ'. Likewise, in Java, one can use
/// the Joda Time's [`ISODateTimeFormat.dateTime()`](<http://www.joda.org/joda-time/apidocs/org/joda/time/format/ISODateTimeFormat.html#dateTime%2D%2D>) to obtain a formatter capable of generating timestamps in this format.
#[derive(Clone, Debug, PartialEq, PartialOrd, Message)]
pub struct Timestamp {
    /// Represents seconds of UTC time since Unix epoch 1970-01-01T00:00:00Z.
    #[bilrost(1)]
    pub seconds: i64,
    /// Non-negative fractions of a second at nanosecond resolution. Negative
    /// second values with fractions must still have non-negative nanos values
    /// that count forward in time. Must be from 0 to 999,999,999
    /// inclusive.
    #[bilrost(tag = 2, encoder = "fixed")]
    pub nanos: i32,
}
