use crate::encoding::local_proxy::LocalProxy;
use crate::encoding::proxy::SealedBilrostTag;
use crate::encoding::type_support::common::time_proxies::{TimeDeltaProxy, TimestampProxy};
use crate::encoding::{
    delegate_proxied_encoding, empty_state_via_default, Canonicity, DecodeErrorKind,
    DistinguishedProxiable, EmptyState, ForOverwrite, General, Packed, Proxiable, Varint,
};
use crate::Canonicity::Canonical;
use crate::DecodeErrorKind::{InvalidValue, OutOfDomainValue};
use alloc::borrow::ToOwned;
use alloc::string::String;
use jiff::{
    civil::{date, Date, DateTime, Time},
    tz::{Offset, TimeZone},
    SignedDuration, Timestamp, Zoned,
};

#[cfg(test)]
pub(super) use {
    civil_date::test_dates, civil_datetime::test_datetimes, civil_time::test_times,
    signedduration::test_signeddurations, zoned::test_zoneds,
};

impl ForOverwrite<(), Date> for () {
    fn for_overwrite() -> Date {
        date(0, 1, 1)
    }
}

impl EmptyState<(), Date> for () {
    fn is_empty(val: &Date) -> bool {
        val == &date(0, 1, 1)
    }

    fn clear(val: &mut Date) {
        *val = <() as ForOverwrite<(), Date>>::for_overwrite();
    }
}

#[inline(always)]
fn parts_to_date<T>(year: T, ordinal0: T) -> Option<Date>
where
    T: TryInto<i16>,
{
    Date::new(year.try_into().ok()?, 1, 1)
        .ok()?
        .with()
        .day_of_year(ordinal0.try_into().ok()?.checked_add(1)?)
        .build()
        .ok()
}

impl Proxiable<SealedBilrostTag> for Date {
    type Proxy = LocalProxy<i16, 2>;

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([self.year(), self.day_of_year() - 1])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [year, ordinal0] = proxy.into_inner();
        *self = parts_to_date(year, ordinal0).ok_or(OutOfDomainValue)?;
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for Date {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([year, ordinal0], canon) = proxy.into_inner_distinguished();
        *self = parts_to_date(year, ordinal0).ok_or(OutOfDomainValue)?;
        Ok(canon)
    }
}

// Date encodes as a packed sequence of signed varints with trailing zeros cut off:
// [year, ordinal day in year (starting at zero)]. The empty value is January 1st on the year 0,
// not 1970.
delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (Date)
    using proxy tag (SealedBilrostTag)
    with general encodings
    including distinguished
    including schema
);

#[cfg(test)]
mod civil_date {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, /*check_type_test,*/ distinguished, relaxed,};
    use crate::encoding::{EmptyState, /*General,*/ WireType};
    use jiff::civil::{date, Date};
    use jiff::Span;
    use proptest::prelude::*;

    pub(in super::super) fn test_dates() -> impl Iterator<Item = Date> {
        [
            Date::MIN,
            Date::MAX,
            <() as EmptyState<(), Date>>::empty(),
            date(1970, 1, 1),
            date(1998, 6, 28),
        ]
        .into_iter()
    }

    #[test]
    fn check_type() {
        for date in test_dates() {
            relaxed::check_type_general(date, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(date, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(Date, via proxy with tag SealedBilrostTag);
    check_type_empty!(Date, via distinguished proxy with tag SealedBilrostTag);

    proptest! {
        #[test]
        fn check_relaxed(
            day_offset in
                Date::ZERO.until(Date::MIN).unwrap().get_days()
                ..=Date::ZERO.until(Date::MAX).unwrap().get_days(),
            tag: u32,
        ) {
            relaxed::check_type_general(
                Date::ZERO.checked_add(Span::new().days(day_offset)).unwrap(),
                tag,
                WireType::LengthDelimited,
            )?;
        }
        #[test]
        fn check_distinguished(
            day_offset in
                Date::ZERO.until(Date::MIN).unwrap().get_days()
                ..=Date::ZERO.until(Date::MAX).unwrap().get_days(),
            tag: u32,
        ) {
            distinguished::check_type_general(
                Date::ZERO.checked_add(Span::new().days(day_offset)).unwrap(),
                tag,
                WireType::LengthDelimited,
            )?;
        }
    }
}

impl ForOverwrite<(), Time> for () {
    fn for_overwrite() -> Time {
        Time::midnight()
    }
}

impl EmptyState<(), Time> for () {
    fn is_empty(val: &Time) -> bool {
        val == &Time::midnight()
    }

    fn clear(val: &mut Time) {
        *val = <() as ForOverwrite<(), Time>>::for_overwrite();
    }
}

#[inline(always)]
fn parts_to_time<T>(hour: T, min: T, sec: T, nano: T) -> Option<Time>
where
    T: TryInto<i8> + TryInto<i32>,
{
    Time::new(
        hour.try_into().ok()?,
        min.try_into().ok()?,
        sec.try_into().ok()?,
        nano.try_into().ok()?,
    )
    .ok()
}

impl Proxiable<SealedBilrostTag> for Time {
    type Proxy = LocalProxy<u32, 4>;

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([
            self.hour() as u32,
            self.minute() as u32,
            self.second() as u32,
            self.subsec_nanosecond() as u32,
        ])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [hour, min, sec, nano] = proxy.into_inner();
        *self = parts_to_time(hour, min, sec, nano).ok_or(OutOfDomainValue)?;
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for Time {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([hour, min, sec, nano], canon) = proxy.into_inner_distinguished();
        *self = parts_to_time(hour, min, sec, nano).ok_or(OutOfDomainValue)?;
        Ok(canon)
    }
}

// Time encodes as a packed sequence of UNsigned varints with trailing zeros cut off:
// [hour, minute, second, nanosecond].
delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (Time)
    using proxy tag (SealedBilrostTag)
    with general encodings
    including distinguished
    including schema
);

#[cfg(test)]
mod civil_time {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, /*check_type_test,*/ distinguished, relaxed,};
    use crate::encoding::{EmptyState, /*General,*/ WireType};
    use jiff::civil::{time, Time};
    use proptest::prelude::*;

    pub(in super::super) fn test_times() -> impl Iterator<Item = Time> + Clone {
        [
            Time::MIN,
            Time::MAX,
            <() as EmptyState<(), Time>>::empty(),
            time(17, 0, 0, 0),
            time(11, 11, 11, 111_111_111),
        ]
        .into_iter()
    }

    #[test]
    fn check_type() {
        for time in test_times() {
            relaxed::check_type_general(time, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(time, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(Time, via proxy with tag SealedBilrostTag);
    check_type_empty!(Time, via distinguished proxy with tag SealedBilrostTag);

    proptest! {
        #[test]
        fn check_relaxed(
            hour in 0..=23i8,
            minute in 0..=59i8,
            second in 0..=59i8,
            nanos in 0..=999_999_999i32,
            tag: u32,
        ) {
            relaxed::check_type_general(
                time(hour, minute, second, nanos),
                tag,
                WireType::LengthDelimited,
            )?;
        }
        #[test]
        fn check_distinguished(
            hour in 0..=23i8,
            minute in 0..=59i8,
            second in 0..=59i8,
            nanos in 0..=999_999_999i32,
            tag: u32,
        ) {
            distinguished::check_type_general(
                time(hour, minute, second, nanos),
                tag,
                WireType::LengthDelimited,
            )?;
        }
    }
}

impl ForOverwrite<(), DateTime> for () {
    fn for_overwrite() -> DateTime {
        DateTime::ZERO
    }
}

impl EmptyState<(), DateTime> for () {
    fn is_empty(val: &DateTime) -> bool {
        val == &DateTime::ZERO
    }

    fn clear(val: &mut DateTime) {
        *val = DateTime::ZERO;
    }
}

impl Proxiable<SealedBilrostTag> for DateTime {
    type Proxy = LocalProxy<i32, 6>;

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([
            self.year() as i32,
            (self.day_of_year() - 1) as i32,
            self.hour() as i32,
            self.minute() as i32,
            self.second() as i32,
            self.subsec_nanosecond() as i32,
        ])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [year, ordinal0, hour, min, sec, nano] = proxy.into_inner();
        *self = Self::from_parts(
            parts_to_date(year, ordinal0).ok_or(OutOfDomainValue)?,
            parts_to_time(hour, min, sec, nano).ok_or(OutOfDomainValue)?,
        );
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for DateTime {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([year, ordinal0, hour, min, sec, nano], canon) = proxy.into_inner_distinguished();
        *self = Self::from_parts(
            parts_to_date(year, ordinal0).ok_or(OutOfDomainValue)?,
            parts_to_time(hour, min, sec, nano).ok_or(OutOfDomainValue)?,
        );
        Ok(canon)
    }
}

// DateTime encodes as a packed sequence of signed varints with trailing zeros cut off:
// [year, ordinal day in year (starting at zero), hour, minute, second, nanosecond]. It can decode
// Date values as if they were truncated DateTimes. The empty value is midnight on January
// 1st of the year 0, not 1970.
delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (DateTime)
    using proxy tag (SealedBilrostTag)
    with general encodings
    including distinguished
    including schema
);

#[cfg(test)]
mod civil_datetime {
    use super::civil_date::test_dates;
    use super::civil_time::test_times;
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, /*check_type_test,*/ distinguished, relaxed,};
    use crate::encoding::{EmptyState, /*General,*/ WireType};
    // use alloc::vec::Vec;
    use itertools::iproduct;
    use jiff::civil::{date, time, Date, DateTime};
    use jiff::Span;
    use proptest::prelude::*;

    pub(in super::super) fn test_datetimes() -> impl IntoIterator<Item = DateTime> {
        [
            DateTime::MIN,
            DateTime::MAX,
            DateTime::default(),
            <() as EmptyState<(), DateTime>>::empty(),
            DateTime::from_parts(date(-44, 3, 15), time(12, 36, 27, 0)),
            DateTime::from_parts(date(-1753, 8, 21), time(14, 49, 8, 0)),
        ]
        .into_iter()
        .chain(
            iproduct!(test_dates(), test_times())
                .map(|(date, time)| DateTime::from_parts(date, time)),
        )
    }

    #[test]
    fn check_type() {
        for datetime in test_datetimes() {
            relaxed::check_type_general(datetime, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(datetime, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(DateTime, via proxy with tag SealedBilrostTag);
    check_type_empty!(DateTime, via distinguished proxy with tag SealedBilrostTag);

    proptest! {
        #[test]
        fn check_relaxed(
            day_offset in
                Date::ZERO.until(Date::MIN).unwrap().get_days()
                ..=Date::ZERO.until(Date::MAX).unwrap().get_days(),
            hour in 0..=23i8,
            minute in 0..=59i8,
            second in 0..=59i8,
            nanos in 0..=999_999_999i32,
            tag: u32,
        ) {
            relaxed::check_type_general(
                DateTime::from_parts(
                    Date::ZERO.checked_add(Span::new().days(day_offset)).unwrap(),
                    time(hour, minute, second, nanos),
                ),
                tag,
                WireType::LengthDelimited,
            )?;
        }
        #[test]
        fn check_distinguished(
            day_offset in
                Date::ZERO.until(Date::MIN).unwrap().get_days()
                ..=Date::ZERO.until(Date::MAX).unwrap().get_days(),
            hour in 0..=23i8,
            minute in 0..=59i8,
            second in 0..=59i8,
            nanos in 0..=999_999_999i32,
            tag: u32,
        ) {
            distinguished::check_type_general(
                DateTime::from_parts(
                    Date::ZERO.checked_add(Span::new().days(day_offset)).unwrap(),
                    time(hour, minute, second, nanos),
                ),
                tag,
                WireType::LengthDelimited,
            )?;
        }
    }
}

impl ForOverwrite<(), Zoned> for () {
    fn for_overwrite() -> Zoned {
        // equivalent to: DateTime::constant(0, 1, 1, 0, 0, 0, 0).in_tz("UTC")
        Zoned::new(Timestamp::constant(-62_167_219_200, 0), TimeZone::UTC)
    }
}

impl EmptyState<(), Zoned> for () {
    fn is_empty(val: &Zoned) -> bool {
        <() as EmptyState<(), _>>::is_empty(&val.datetime()) && val.offset().is_zero()
    }

    fn clear(val: &mut Zoned) {
        *val = <() as ForOverwrite<(), Zoned>>::for_overwrite();
    }
}

impl Proxiable<SealedBilrostTag> for Zoned {
    type Proxy = (DateTime, (i8, i8, i8, Option<String>));

    fn encode_proxy(&self) -> Self::Proxy {
        let offset_secs = self.offset().seconds();
        let secs = (offset_secs % 60) as i8;
        let offset_mins = offset_secs / 60;
        let mins = (offset_mins % 60) as i8;
        let hours = (offset_mins / 60) as i8;
        (
            self.timestamp().to_zoned(TimeZone::UTC).datetime(),
            (
                hours,
                mins,
                secs,
                if offset_secs == 0 {
                    // jiff's timezone with no offset always becomes "UTC" as a special case,
                    // which isn't an empty value, so we always just elide that name.
                    None
                } else {
                    self.time_zone().iana_name().map(ToOwned::to_owned)
                },
            ),
        )
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let (civil, (hours, mins, secs, tz_name)) = proxy;

        let utc = civil.to_zoned(TimeZone::UTC).unwrap();

        // we aren't very stringent about the allowed values for the offsets here, but we at least
        // check that they're in a reasonable range so we don't overflow.
        match (hours, mins, secs) {
            (-25..=25, -59..=59, -59..=59) => {}
            _ => return Err(OutOfDomainValue),
        }
        let offset_secs = (hours as i32 * 60 * 60) + (mins as i32 * 60) + secs as i32;

        *self = 'create: {
            if let Some(tz_name) = tz_name {
                if let Ok(tz) = TimeZone::get(&tz_name) {
                    // if we can load a timezone, we try to create a Zoned value in that zone
                    let in_zone = utc.with_time_zone(tz);
                    if in_zone.offset().seconds() == offset_secs {
                        break 'create in_zone;
                    }
                }
            }
            // if we can't find the named time zone or the time zone we found doesn't produce a
            // matching offset, we discard the name of the time zone and create the timestamp with
            // a fixed UTC offset.
            utc.with_time_zone(TimeZone::fixed(
                Offset::from_seconds(offset_secs).map_err(|_| OutOfDomainValue)?,
            ))
        };
        Ok(())
    }
}

// Because there's no real way to guarantee that the Zoned type will always have the same values,
// since the timezone database it's using can change, we don't implement distinguished encoding for
// the type.

// The encoding for DateTime<Tz> is the same as the (DateTime, Tz::Offset) that it is composed
// of.
delegate_proxied_encoding!(
    use encoding ((General, (Varint, Varint, Varint, General))) to encode proxied type (Zoned)
    using proxy tag (SealedBilrostTag)
    with general encodings
    including schema
);

#[cfg(test)]
mod zoned {
    use super::timestamp::test_timestamps;
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, relaxed};
    use crate::encoding::WireType;
    use itertools::iproduct;
    use jiff::tz::{Offset, TimeZone};
    use jiff::Zoned;

    fn test_timezones() -> impl Iterator<Item = TimeZone> + Clone {
        [
            TimeZone::fixed(Offset::MAX),
            TimeZone::fixed(Offset::MIN),
            TimeZone::UTC,
            TimeZone::get("Asia/Tokyo").unwrap(),
            TimeZone::get("America/Los_Angeles").unwrap(),
            TimeZone::get("Pacific/Kiritimati").unwrap(),
            TimeZone::get("Pacific/Midway").unwrap(),
        ]
        .into_iter()
    }

    pub(in super::super) fn test_zoneds() -> impl Iterator<Item = Zoned> + Clone {
        iproduct!(test_timestamps(), test_timezones())
            .map(|(ts, tz)| ts.to_zoned(tz))
            .chain([
                "2026-11-01T01:01:00-08[America/Los_Angeles]"
                    .parse()
                    .unwrap(),
                "2026-11-01T01:01:00-07[America/Los_Angeles]"
                    .parse()
                    .unwrap(),
            ])
    }

    #[test]
    fn check_type() {
        for zoned in test_zoneds() {
            relaxed::check_type_general(zoned, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(Zoned, via proxy with tag SealedBilrostTag);
}

empty_state_via_default!(Timestamp);

impl Proxiable<SealedBilrostTag> for Timestamp {
    type Proxy = TimestampProxy;

    fn encode_proxy(&self) -> Self::Proxy {
        TimestampProxy {
            secs: self.as_second(),
            nanos: self.subsec_nanosecond(),
        }
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        match (proxy.secs, proxy.nanos) {
            // we ensure that the sign of secs and nanos matches and that nanos is in-bounds
            (..=0, -999_999_999..=-1) | (.., 0) | (0.., 1..=999_999_999) => {}
            _ => return Err(InvalidValue),
        }
        *self = Self::new(proxy.secs, proxy.nanos).map_err(|_| OutOfDomainValue)?;
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for Timestamp {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        self.decode_proxy(proxy)?;
        Ok(Canonical)
    }
}

// The encoding for Timestamp matches that of bilrost_types::Timestamp.
delegate_proxied_encoding!(
    use encoding (General) to encode proxied type (Timestamp)
    using proxy tag (SealedBilrostTag)
    with general encodings
    including distinguished
    including schema
);

#[cfg(test)]
mod timestamp {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, distinguished, relaxed};
    use crate::encoding::{EmptyState, WireType};
    use jiff::civil::DateTime;
    use jiff::Timestamp;
    use proptest::prelude::*;

    check_type_empty!(Timestamp, via proxy with tag SealedBilrostTag);
    check_type_empty!(Timestamp, via distinguished proxy with tag SealedBilrostTag);

    pub(in super::super) fn test_timestamps() -> impl Iterator<Item = Timestamp> + Clone {
        [
            Timestamp::default(),
            Timestamp::MIN,
            Timestamp::MAX,
            <() as EmptyState<(), Timestamp>>::empty(),
            Timestamp::new(900, 10).unwrap(),
            Timestamp::new(-60, 0).unwrap(),
            DateTime::constant(0, 1, 1, 0, 0, 0, 0)
                .in_tz("UTC")
                .unwrap()
                .timestamp(),
        ]
        .into_iter()
    }

    #[test]
    fn check_type() {
        for td in test_timestamps() {
            relaxed::check_type_general(td, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(td, 123, WireType::LengthDelimited).unwrap();
        }
    }

    proptest! {
        #[test]
        fn check_relaxed(
            seconds in Timestamp::MIN.as_second()..=Timestamp::MAX.as_second(),
            nanos in -999_999_999..=999_999_999i32,
            tag: u32,
        ) {
            relaxed::check_type_general(
                Timestamp::new(seconds, nanos).unwrap(),
                tag,
                WireType::LengthDelimited,
            )?;
        }
        #[test]
        fn check_distinguished(
            seconds in Timestamp::MIN.as_second()..=Timestamp::MAX.as_second(),
            nanos in -999_999_999..=999_999_999i32,
            tag: u32,
        ) {
            distinguished::check_type_general(
                Timestamp::new(seconds, nanos).unwrap(),
                tag,
                WireType::LengthDelimited,
            )?;
        }
    }
}

empty_state_via_default!(SignedDuration);

impl Proxiable<SealedBilrostTag> for SignedDuration {
    type Proxy = TimeDeltaProxy;

    fn encode_proxy(&self) -> Self::Proxy {
        TimeDeltaProxy {
            secs: self.as_secs(),
            nanos: self.subsec_nanos(),
        }
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        match (proxy.secs, proxy.nanos) {
            // we ensure that the sign of secs and nanos matches and that nanos is in-bounds
            (..=0, -999_999_999..=-1) | (.., 0) | (0.., 1..=999_999_999) => {}
            _ => return Err(InvalidValue),
        }
        *self = Self::new(proxy.secs, proxy.nanos);
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for SignedDuration {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        self.decode_proxy(proxy)?;
        Ok(Canonical)
    }
}

// The encoding for SignedDuration matches that of bilrost_types::Duration.
delegate_proxied_encoding!(
    use encoding (General) to encode proxied type (SignedDuration)
    using proxy tag (SealedBilrostTag)
    with general encodings
    including distinguished
    including schema
);

#[cfg(test)]
mod signedduration {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, distinguished, relaxed};
    use crate::encoding::{EmptyState, WireType};
    use jiff::SignedDuration;
    use proptest::prelude::*;

    check_type_empty!(SignedDuration, via proxy with tag SealedBilrostTag);
    check_type_empty!(SignedDuration, via distinguished proxy with tag SealedBilrostTag);

    pub(in super::super) fn test_signeddurations() -> impl Iterator<Item = SignedDuration> + Clone {
        [
            SignedDuration::default(),
            SignedDuration::MIN,
            SignedDuration::MAX,
            <() as EmptyState<(), SignedDuration>>::empty(),
            SignedDuration::new(900, 10),
            SignedDuration::from_secs(-60),
        ]
        .into_iter()
    }

    #[test]
    fn check_type() {
        for sd in test_signeddurations() {
            relaxed::check_type_general(sd, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(sd, 123, WireType::LengthDelimited).unwrap();
        }
    }

    proptest! {
        #[test]
        fn check_relaxed(
            seconds: i64,
            nanos in -999_999_999..=999_999_999i32,
            tag: u32,
        ) {
            relaxed::check_type_general(
                SignedDuration::new(seconds, nanos),
                tag,
                WireType::LengthDelimited,
            )?;
        }
        #[test]
        fn check_distinguished(
            seconds: i64,
            nanos in -999_999_999..=999_999_999i32,
            tag: u32,
        ) {
            distinguished::check_type_general(
                SignedDuration::new(seconds, nanos),
                tag,
                WireType::LengthDelimited,
            )?;
        }
    }
}
