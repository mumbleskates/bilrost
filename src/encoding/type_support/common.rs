#[cfg(any(feature = "chrono", feature = "time", feature = "jiff"))]
pub(crate) mod time_proxies {
    use crate::buf::ReverseBuf;
    use crate::encoding::underived::{
        underived_decode, underived_decode_distinguished, underived_encode, underived_encoded_len,
        underived_prepend, underived_schema,
    };
    use crate::encoding::{
        delegate_value_encoding, empty_state_via_default, Capped, DecodeContext,
        DistinguishedValueDecoder, Fixed, General, GeneralGeneric, RestrictedDecodeContext,
        ValueDecoder, ValueEncoder, WireType, Wiretyped,
    };
    use crate::DecodeErrorKind::InvalidValue;
    use crate::{Canonicity, DecodeError};
    use bytes::{Buf, BufMut};

    /// Encoding standin for bilrost_types::Duration
    #[derive(Debug, Default, PartialEq, Eq)]
    pub(crate) struct TimeDeltaProxy {
        pub(crate) secs: i64,
        pub(crate) nanos: i32,
    }

    empty_state_via_default!(TimeDeltaProxy);

    impl<const P: u8> Wiretyped<GeneralGeneric<P>, TimeDeltaProxy> for () {
        const WIRE_TYPE: WireType = WireType::LengthDelimited;
    }

    underived_schema!(TimeDeltaProxy: "TimeDelta" {
        1: General => secs: i64,
        2: Fixed => nanos: i32,
    });

    impl<const P: u8> ValueEncoder<GeneralGeneric<P>, TimeDeltaProxy> for () {
        fn encode_value<B: BufMut + ?Sized>(value: &TimeDeltaProxy, buf: &mut B) {
            underived_encode!(TimeDelta {
                1: General => secs: &value.secs,
                2: Fixed => nanos: &value.nanos,
            }, buf)
        }

        fn prepend_value<B: ReverseBuf + ?Sized>(value: &TimeDeltaProxy, buf: &mut B) {
            underived_prepend!(TimeDelta {
                2: Fixed => nanos: &value.nanos,
                1: General => secs: &value.secs,
            }, buf)
        }

        fn value_encoded_len(value: &TimeDeltaProxy) -> usize {
            underived_encoded_len!(TimeDelta {
                1: General => secs: &value.secs,
                2: Fixed => nanos: &value.nanos,
            })
        }
    }

    impl<const P: u8> ValueDecoder<GeneralGeneric<P>, TimeDeltaProxy> for () {
        fn decode_value<B: Buf + ?Sized>(
            value: &mut TimeDeltaProxy,
            mut buf: Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            underived_decode!(TimeDelta {
                1: General => secs: &mut value.secs,
                2: Fixed => nanos: &mut value.nanos,
            }, owned, buf, ctx)?;
            if value.secs.signum() as i32 * value.nanos.signum() == -1 {
                Err(DecodeError::new(InvalidValue))
            } else {
                Ok(())
            }
        }
    }

    impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, TimeDeltaProxy> for () {
        const CHECKS_EMPTY: bool = true;

        fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
            value: &mut TimeDeltaProxy,
            mut buf: Capped<impl Buf + ?Sized>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            underived_decode_distinguished!(TimeDelta {
                1: General => secs: &mut value.secs,
                2: Fixed => nanos: &mut value.nanos,
            }, owned, buf, ctx)
        }
    }

    delegate_value_encoding!(
        encoding (GeneralGeneric<P>) borrows type (TimeDeltaProxy) as owned including distinguished
        with generics (const P: u8)
    );

    /// TimestampProxy is exactly like TimeDeltaProxy but it has a different name and semantically
    /// stands in for bilrost_types::Timestamp instead.
    #[derive(Debug, Default, PartialEq, Eq)]
    pub(crate) struct TimestampProxy {
        pub(crate) secs: i64,
        pub(crate) nanos: i32,
    }

    empty_state_via_default!(TimestampProxy);

    impl<const P: u8> Wiretyped<GeneralGeneric<P>, TimestampProxy> for () {
        const WIRE_TYPE: WireType = WireType::LengthDelimited;
    }

    underived_schema!(TimestampProxy: "Timestamp" {
        1: General => secs: i64,
        2: Fixed => nanos: i32,
    });

    impl<const P: u8> ValueEncoder<GeneralGeneric<P>, TimestampProxy> for () {
        fn encode_value<B: BufMut + ?Sized>(value: &TimestampProxy, buf: &mut B) {
            underived_encode!(Timestamp {
                1: General => secs: &value.secs,
                2: Fixed => nanos: &value.nanos,
            }, buf)
        }

        fn prepend_value<B: ReverseBuf + ?Sized>(value: &TimestampProxy, buf: &mut B) {
            underived_prepend!(Timestamp {
                2: Fixed => nanos: &value.nanos,
                1: General => secs: &value.secs,
            }, buf)
        }

        fn value_encoded_len(value: &TimestampProxy) -> usize {
            underived_encoded_len!(Timestamp {
                1: General => secs: &value.secs,
                2: Fixed => nanos: &value.nanos,
            })
        }
    }

    impl<const P: u8> ValueDecoder<GeneralGeneric<P>, TimestampProxy> for () {
        fn decode_value<B: Buf + ?Sized>(
            value: &mut TimestampProxy,
            mut buf: Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError> {
            underived_decode!(Timestamp {
                1: General => secs: &mut value.secs,
                2: Fixed => nanos: &mut value.nanos,
            }, owned, buf, ctx)
        }
    }

    impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, TimestampProxy> for () {
        const CHECKS_EMPTY: bool = true;

        fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
            value: &mut TimestampProxy,
            mut buf: Capped<impl Buf + ?Sized>,
            ctx: RestrictedDecodeContext,
        ) -> Result<Canonicity, DecodeError> {
            underived_decode_distinguished!(Timestamp {
                1: General => secs: &mut value.secs,
                2: Fixed => nanos: &mut value.nanos,
            }, owned, buf, ctx)
        }
    }

    delegate_value_encoding!(
        encoding (GeneralGeneric<P>) borrows type (TimestampProxy) as owned including distinguished
        with generics (const P: u8)
    );
}

/// This is where we show that we have equivalent encodings for the time and chrono crate types.
#[cfg(all(test, feature = "chrono", feature = "time", feature = "jiff"))]
mod chrono_time_value_compat {
    use crate::encoding::type_support::time::with_random_values;
    use crate::encoding::type_support::{
        chrono as impl_chrono, jiff as impl_jiff, time as impl_time,
    };
    use crate::encoding::{EmptyState, General, Proxiable, ValueEncoder};
    use alloc::fmt::Debug;
    use alloc::vec::Vec;
    use chrono::{Datelike, FixedOffset, Timelike};
    use itertools::iproduct;

    fn assert_same_encoding<T, U>(t: &T, u: &U)
    where
        T: Debug,
        U: Debug,
        (): ValueEncoder<General, T> + ValueEncoder<General, U>,
    {
        let mut tbuf = Vec::new();
        <() as ValueEncoder<General, T>>::encode_value(t, &mut tbuf);
        let mut ubuf = Vec::new();
        <() as ValueEncoder<General, U>>::encode_value(u, &mut ubuf);
        if tbuf != ubuf {
            assert_eq!(tbuf, ubuf, "asserting that {t:?} and {u:?} encode the same");
        }
    }

    fn date_c_to_t(date: chrono::NaiveDate) -> Option<time::Date> {
        time::Date::from_ordinal_date(date.year(), date.ordinal() as u16).ok()
    }

    fn date_t_to_c(date: time::Date) -> Option<chrono::NaiveDate> {
        chrono::NaiveDate::from_yo_opt(date.year(), date.ordinal().into())
    }

    fn date_c_to_j(date: chrono::NaiveDate) -> Option<jiff::civil::Date> {
        jiff::civil::Date::new(date.year().try_into().ok()?, 1, 1)
            .ok()?
            .with()
            .day_of_year(date.ordinal() as i16)
            .build()
            .ok()
    }

    fn date_j_to_c(date: jiff::civil::Date) -> Option<chrono::NaiveDate> {
        chrono::NaiveDate::from_ymd_opt(date.year() as i32, date.month() as u32, date.day() as u32)
    }

    #[test]
    fn date() {
        for chrono_date in impl_chrono::test_dates() {
            if let Some(time_date) = date_c_to_t(chrono_date) {
                assert_same_encoding(&chrono_date, &time_date);
            };
            if let Some(jiff_date) = date_c_to_j(chrono_date) {
                assert_same_encoding(&chrono_date, &jiff_date);
            }
        }
        for time_date in with_random_values(impl_time::test_dates()) {
            if let Some(chrono_date) = date_t_to_c(time_date) {
                assert_same_encoding(&time_date, &chrono_date);
            };
        }
        for jiff_date in impl_jiff::test_dates() {
            if let Some(chrono_date) = date_j_to_c(jiff_date) {
                assert_same_encoding(&jiff_date, &chrono_date);
            }
        }
    }

    fn time_c_to_t(t: chrono::NaiveTime) -> Option<time::Time> {
        time::Time::from_hms_nano(
            u8::try_from(t.hour()).unwrap(),
            u8::try_from(t.minute()).unwrap(),
            u8::try_from(t.second()).unwrap(),
            t.nanosecond(),
        )
        .ok()
    }

    fn time_t_to_c(t: time::Time) -> Option<chrono::NaiveTime> {
        chrono::NaiveTime::from_hms_nano_opt(
            t.hour().into(),
            t.minute().into(),
            t.second().into(),
            t.nanosecond(),
        )
    }

    fn time_c_to_j(t: chrono::NaiveTime) -> Option<jiff::civil::Time> {
        jiff::civil::Time::new(
            t.hour() as i8,
            t.minute() as i8,
            t.second() as i8,
            t.nanosecond() as i32,
        )
        .ok()
    }

    fn time_j_to_c(t: jiff::civil::Time) -> Option<chrono::NaiveTime> {
        chrono::NaiveTime::from_hms_nano_opt(
            t.hour() as u32,
            t.minute() as u32,
            t.second() as u32,
            t.subsec_nanosecond() as u32,
        )
    }

    #[test]
    fn time() {
        for chrono_time in impl_chrono::test_times() {
            if let Some(time_time) = time_c_to_t(chrono_time) {
                assert_same_encoding(&chrono_time, &time_time);
            }
            if let Some(jiff_time) = time_c_to_j(chrono_time) {
                assert_same_encoding(&chrono_time, &jiff_time);
            }
        }
        for time_time in with_random_values(impl_time::test_times()) {
            if let Some(chrono_time) = time_t_to_c(time_time) {
                assert_same_encoding(&time_time, &chrono_time);
            }
        }
        for jiff_time in impl_jiff::test_times() {
            if let Some(chrono_time) = time_j_to_c(jiff_time) {
                assert_same_encoding(&jiff_time, &chrono_time);
            }
        }
    }

    fn datetime_c_to_t(dt: chrono::NaiveDateTime) -> Option<time::PrimitiveDateTime> {
        Some(time::PrimitiveDateTime::new(
            date_c_to_t(dt.date())?,
            time_c_to_t(dt.time())?,
        ))
    }

    fn datetime_t_to_c(dt: time::PrimitiveDateTime) -> Option<chrono::NaiveDateTime> {
        Some(chrono::NaiveDateTime::new(
            date_t_to_c(dt.date())?,
            time_t_to_c(dt.time())?,
        ))
    }

    fn datetime_c_to_j(dt: chrono::NaiveDateTime) -> Option<jiff::civil::DateTime> {
        Some(jiff::civil::DateTime::from_parts(
            date_c_to_j(dt.date())?,
            time_c_to_j(dt.time())?,
        ))
    }

    fn datetime_j_to_c(dt: jiff::civil::DateTime) -> Option<chrono::NaiveDateTime> {
        Some(chrono::NaiveDateTime::new(
            date_j_to_c(dt.date())?,
            time_j_to_c(dt.time())?,
        ))
    }

    #[test]
    fn datetime() {
        for chrono_datetime in impl_chrono::test_datetimes() {
            if let Some(time_datetime) = datetime_c_to_t(chrono_datetime) {
                assert_same_encoding(&chrono_datetime, &time_datetime);
            }
            if let Some(jiff_datetime) = datetime_c_to_j(chrono_datetime) {
                assert_same_encoding(&chrono_datetime, &jiff_datetime);
            }
        }
        for time_datetime in with_random_values(impl_time::test_datetimes()) {
            if let Some(chrono_datetime) = datetime_t_to_c(time_datetime) {
                assert_same_encoding(&time_datetime, &chrono_datetime);
            }
        }
        for jiff_datetime in impl_jiff::test_datetimes() {
            if let Some(chrono_datetime) = datetime_j_to_c(jiff_datetime) {
                assert_same_encoding(&jiff_datetime, &chrono_datetime);
            }
        }
    }

    fn offset_c_to_t(offset: FixedOffset) -> Option<time::UtcOffset> {
        time::UtcOffset::from_whole_seconds(offset.local_minus_utc()).ok()
    }

    fn offset_t_to_c(offset: time::UtcOffset) -> Option<FixedOffset> {
        FixedOffset::east_opt(offset.whole_seconds())
    }

    #[test]
    fn zone() {
        for chrono_offset in impl_chrono::test_zones() {
            let Some(time_offset) = offset_c_to_t(chrono_offset) else {
                continue;
            };
            assert_same_encoding(&chrono_offset, &time_offset);
        }
        for time_offset in with_random_values(impl_time::test_zones()) {
            let Some(chrono_offset) = offset_t_to_c(time_offset) else {
                continue;
            };
            assert_same_encoding(&time_offset, &chrono_offset);
        }
    }

    fn aware_compose_chrono(
        pair: (chrono::NaiveDateTime, FixedOffset),
    ) -> Option<chrono::DateTime<FixedOffset>> {
        let mut result = <() as EmptyState<(), chrono::DateTime<FixedOffset>>>::empty();
        result.decode_proxy(pair).ok()?;
        Some(result)
    }

    fn aware_compose_time(
        pair: (time::PrimitiveDateTime, time::UtcOffset),
    ) -> Option<time::OffsetDateTime> {
        let mut result = <() as EmptyState<(), time::OffsetDateTime>>::empty();
        result.decode_proxy(pair).ok()?;
        Some(result)
    }

    fn aware_c_to_t(aware: chrono::DateTime<FixedOffset>) -> Option<time::OffsetDateTime> {
        let wall_time_utc = datetime_c_to_t(aware.naive_utc())?;
        let wall_time = time::OffsetDateTime::new_in_offset(
            wall_time_utc.date(),
            wall_time_utc.time(),
            time::UtcOffset::UTC,
        );
        let offset = offset_c_to_t(*aware.offset())?;
        let res = wall_time.checked_to_offset(offset)?;
        assert_eq!(aware.timestamp(), res.unix_timestamp());
        Some(res)
    }

    fn aware_t_to_c(aware: time::OffsetDateTime) -> Option<chrono::DateTime<FixedOffset>> {
        let wall_time_zoned =
            datetime_t_to_c(time::PrimitiveDateTime::new(aware.date(), aware.time()))?;
        let tz = chrono::FixedOffset::east_opt(aware.offset().whole_seconds())?;
        let wall_time_utc = wall_time_zoned.checked_sub_offset(tz)?;
        let res =
            chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(wall_time_utc, tz);
        assert_eq!(aware.unix_timestamp(), res.timestamp());
        Some(res)
    }

    fn aware_c_to_j(aware: chrono::DateTime<FixedOffset>) -> Option<jiff::Zoned> {
        let res = jiff::Timestamp::new(aware.timestamp(), aware.nanosecond() as i32)
            .ok()?
            .to_zoned(jiff::tz::TimeZone::fixed(
                jiff::tz::Offset::from_seconds(aware.offset().local_minus_utc()).ok()?,
            ));
        assert_eq!(
            aware.timestamp(),
            res.timestamp().as_second()
                + if res.timestamp().subsec_nanosecond() < 0 {
                    -1
                } else {
                    0
                },
            "chrono {:?} / jiff {:?}",
            aware,
            res
        );
        Some(res)
    }

    fn aware_j_to_c(aware: &jiff::Zoned) -> Option<chrono::DateTime<FixedOffset>> {
        if aware.offset().seconds() != 0 && aware.time_zone().iana_name().is_some() {
            return None;
        }
        let (secs, signed_nanos) = (
            aware.timestamp().as_second(),
            aware.timestamp().subsec_nanosecond(),
        );
        let (corrected_secs, unsigned_nanos) = if signed_nanos < 0 {
            (secs.checked_sub(1)?, (signed_nanos + 1_000_000_000) as u32)
        } else {
            (secs, signed_nanos as u32)
        };

        let res = chrono::DateTime::from_timestamp(corrected_secs, unsigned_nanos)?
            .with_timezone(&FixedOffset::east_opt(aware.offset().seconds())?);
        assert_eq!(
            aware.timestamp().as_second()
                + if aware.timestamp().subsec_nanosecond() < 0 {
                    -1
                } else {
                    0
                },
            res.timestamp(),
            "jiff {:?} / chrono {:?}",
            aware,
            res
        );
        Some(res)
    }

    #[test]
    fn aware_date() {
        for chrono_pair in iproduct!(impl_chrono::test_datetimes(), impl_chrono::test_zones()) {
            let chrono_aware = aware_compose_chrono(chrono_pair).unwrap();
            if let Some(time_aware) = aware_c_to_t(chrono_aware) {
                if chrono_aware.offset().local_minus_utc() == 0 {
                    assert_same_encoding(&chrono_aware, &time_aware);
                }
            }
            if let Some(jiff_aware) = aware_c_to_j(chrono_aware) {
                assert_same_encoding(&chrono_aware, &jiff_aware);
            }
        }
        for time_pair in with_random_values(iproduct!(
            impl_time::test_datetimes(),
            impl_time::test_zones()
        )) {
            let time_aware = aware_compose_time(time_pair).unwrap();
            if let Some(chrono_aware) = aware_t_to_c(time_aware) {
                if time_aware.offset().is_utc() {
                    assert_same_encoding(&time_aware, &chrono_aware);
                }
            }
        }
        for jiff_aware in impl_jiff::test_zoneds() {
            if let Some(chrono_aware) = aware_j_to_c(&jiff_aware) {
                assert_same_encoding(&jiff_aware, &chrono_aware);
            }
        }
    }

    fn delta_c_to_t(delta: chrono::TimeDelta) -> Option<time::Duration> {
        time::Duration::seconds(delta.num_seconds())
            .checked_add(time::Duration::nanoseconds(delta.subsec_nanos().into()))
    }

    fn delta_t_to_c(delta: time::Duration) -> Option<chrono::TimeDelta> {
        let (secs, signed_nanos) = (delta.whole_seconds(), delta.subsec_nanoseconds());
        let (corrected_secs, unsigned_nanos) = if signed_nanos < 0 {
            (secs.checked_sub(1)?, (signed_nanos + 1_000_000_000) as u32)
        } else {
            (secs, signed_nanos as u32)
        };
        chrono::TimeDelta::new(corrected_secs, unsigned_nanos)
    }

    fn delta_c_to_j(delta: chrono::TimeDelta) -> Option<jiff::SignedDuration> {
        Some(jiff::SignedDuration::new(
            delta.num_seconds(),
            delta.subsec_nanos(),
        ))
    }

    fn delta_j_to_c(delta: jiff::SignedDuration) -> Option<chrono::TimeDelta> {
        let (secs, signed_nanos) = (delta.as_secs(), delta.subsec_nanos());
        let (corrected_secs, unsigned_nanos) = if signed_nanos < 0 {
            (secs.checked_sub(1)?, (signed_nanos + 1_000_000_000) as u32)
        } else {
            (secs, signed_nanos as u32)
        };
        chrono::TimeDelta::new(corrected_secs, unsigned_nanos)
    }

    #[test]
    fn timedelta() {
        let mut rng = rand::thread_rng();
        for chrono_delta in impl_chrono::test_timedeltas()
            .chain(core::iter::repeat_with(|| {
                impl_chrono::random_timedelta(&mut rng)
            }))
            .take(crate::encoding::type_support::time::RANDOM_SAMPLES)
        {
            if let Some(time_delta) = delta_c_to_t(chrono_delta) {
                assert_same_encoding(&chrono_delta, &time_delta);
            }
            if let Some(jiff_delta) = delta_c_to_j(chrono_delta) {
                assert_same_encoding(&chrono_delta, &jiff_delta);
            }
        }
        for time_delta in with_random_values(impl_time::test_durations()) {
            if let Some(chrono_delta) = delta_t_to_c(time_delta) {
                assert_same_encoding(&time_delta, &chrono_delta);
            }
        }
        for jiff_delta in impl_jiff::test_signeddurations() {
            if let Some(chrono_delta) = delta_j_to_c(jiff_delta) {
                assert_same_encoding(&jiff_delta, &chrono_delta);
            }
        }
    }
}
