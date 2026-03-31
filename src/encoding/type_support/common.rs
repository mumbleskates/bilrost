#[cfg(any(feature = "chrono", feature = "time"))]
pub(crate) mod time_proxies {
    use crate::buf::ReverseBuf;
    use crate::encoding::schema::{FieldSet, MessageSchema, Schema, ValueSchema};
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
    use alloc::boxed::Box;
    use bytes::{Buf, BufMut};
    use core::fmt::Display;

    #[derive(Debug, Default, PartialEq, Eq)]
    pub(crate) struct TimeDeltaProxy {
        pub(crate) secs: i64,
        pub(crate) nanos: i32,
    }

    empty_state_via_default!(TimeDeltaProxy);

    impl<const P: u8> Wiretyped<GeneralGeneric<P>, TimeDeltaProxy> for () {
        const WIRE_TYPE: WireType = WireType::LengthDelimited;
    }

    impl MessageSchema for TimeDeltaProxy {
        underived_schema!(TimeDelta {
            1: General => secs: i64,
            2: Fixed => nanos: i32,
        });
    }

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
}

/// This is where we show that we have equivalent encodings for the time and chrono crate types.
#[cfg(all(test, feature = "chrono", feature = "time"))]
mod chrono_time_value_compat {
    use crate::encoding::type_support::time::with_random_values;
    use crate::encoding::type_support::{chrono as impl_chrono, time as impl_time};
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

    #[test]
    fn date() {
        for chrono_date in impl_chrono::test_dates() {
            let Some(time_date) = date_c_to_t(chrono_date) else {
                continue;
            };
            assert_same_encoding(&chrono_date, &time_date);
        }
        for time_date in with_random_values(impl_time::test_dates()) {
            let Some(chrono_date) = date_t_to_c(time_date) else {
                continue;
            };
            assert_same_encoding(&time_date, &chrono_date);
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

    #[test]
    fn time() {
        for chrono_time in impl_chrono::test_times() {
            let Some(time_time) = time_c_to_t(chrono_time) else {
                continue;
            };
            assert_same_encoding(&chrono_time, &time_time);
        }
        for time_time in with_random_values(impl_time::test_times()) {
            let Some(chrono_time) = time_t_to_c(time_time) else {
                continue;
            };
            assert_same_encoding(&time_time, &chrono_time);
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

    #[test]
    fn datetime() {
        for chrono_datetime in impl_chrono::test_datetimes() {
            let Some(time_datetime) = datetime_c_to_t(chrono_datetime) else {
                continue;
            };
            assert_same_encoding(&chrono_datetime, &time_datetime);
        }
        for time_datetime in with_random_values(impl_time::test_datetimes()) {
            let Some(chrono_datetime) = datetime_t_to_c(time_datetime) else {
                continue;
            };
            assert_same_encoding(&time_datetime, &chrono_datetime);
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
        let (datetime, offset) = aware.encode_proxy();
        aware_compose_time((datetime_c_to_t(datetime)?, offset_c_to_t(offset)?))
    }

    fn aware_t_to_c(aware: time::OffsetDateTime) -> Option<chrono::DateTime<FixedOffset>> {
        let (datetime, offset) = aware.encode_proxy();
        aware_compose_chrono((datetime_t_to_c(datetime)?, offset_t_to_c(offset)?))
    }

    #[test]
    fn aware_date() {
        for chrono_pair in iproduct!(impl_chrono::test_datetimes(), impl_chrono::test_zones()) {
            let chrono_aware = aware_compose_chrono(chrono_pair).unwrap();
            let Some(time_aware) = aware_c_to_t(chrono_aware) else {
                continue;
            };
            assert_same_encoding(&chrono_aware, &time_aware);
        }
        for time_pair in with_random_values(iproduct!(
            impl_time::test_datetimes(),
            impl_time::test_zones()
        )) {
            let time_aware = aware_compose_time(time_pair).unwrap();
            let Some(chrono_aware) = aware_t_to_c(time_aware) else {
                continue;
            };
            assert_same_encoding(&time_aware, &chrono_aware);
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

    #[test]
    fn timedelta() {
        let mut rng = rand::thread_rng();
        for chrono_delta in impl_chrono::test_timedeltas()
            .chain(core::iter::repeat_with(|| {
                impl_chrono::random_timedelta(&mut rng)
            }))
            .take(crate::encoding::type_support::time::RANDOM_SAMPLES)
        {
            let Some(time_delta) = delta_c_to_t(chrono_delta) else {
                continue;
            };
            assert_same_encoding(&chrono_delta, &time_delta);
        }
        for time_delta in with_random_values(impl_time::test_durations()) {
            let Some(chrono_delta) = delta_t_to_c(time_delta) else {
                continue;
            };
            assert_same_encoding(&time_delta, &chrono_delta);
        }
    }
}
