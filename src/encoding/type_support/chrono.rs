use crate::encoding::local_proxy::LocalProxy;
use crate::encoding::proxy::SealedBilrostTag;
use crate::encoding::type_support::common::time_proxies::TimeDeltaProxy;
use crate::encoding::{
    delegate_proxied_encoding, empty_state_via_default, Canonicity, DecodeErrorKind,
    DistinguishedProxiable, EmptyState, ForOverwrite, General, Packed, Proxiable, Varint,
};
use crate::Canonicity::Canonical;
use crate::DecodeErrorKind::{InvalidValue, OutOfDomainValue};
use chrono::{
    DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone,
    Timelike, Utc,
};

#[cfg(all(test, feature = "time"))]
pub(super) use {
    fixedoffset::test_zones,
    naivedate::test_dates,
    naivedatetime::test_datetimes,
    naivetime::test_times,
    timedelta::{random_timedelta, test_timedeltas},
};

impl ForOverwrite<(), NaiveDate> for () {
    fn for_overwrite() -> NaiveDate {
        NaiveDate::from_yo_opt(0, 1).unwrap()
    }
}

impl EmptyState<(), NaiveDate> for () {
    fn is_empty(val: &NaiveDate) -> bool {
        (val.year(), val.ordinal0()) == (0, 0)
    }

    fn clear(val: &mut NaiveDate) {
        *val = <() as ForOverwrite<(), NaiveDate>>::for_overwrite();
    }
}

#[inline(always)]
fn parts_to_naivedate(year: i32, ordinal0: i32) -> Option<NaiveDate> {
    NaiveDate::from_yo_opt(year, u32::try_from(ordinal0).ok()?.checked_add(1)?)
}

impl Proxiable<SealedBilrostTag> for NaiveDate {
    type Proxy = LocalProxy<i32, 2>;

    fn new_proxy() -> Self::Proxy {
        Self::Proxy::new_empty()
    }

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([self.year(), self.ordinal0() as i32])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [year, ordinal0] = proxy.into_inner();
        *self = parts_to_naivedate(year, ordinal0).ok_or(OutOfDomainValue)?;
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for NaiveDate {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([year, ordinal0], canon) = proxy.into_inner_distinguished();
        *self = parts_to_naivedate(year, ordinal0).ok_or(OutOfDomainValue)?;
        Ok(canon)
    }
}

// NaiveDate encodes as a packed sequence of signed varints with trailing zeros cut off:
// [year, ordinal day in year (starting at zero)]. The empty value is January 1st on the year 0,
// not 1970.
delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (NaiveDate)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod naivedate {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, check_type_test, distinguished, relaxed};
    use crate::encoding::{EmptyState, General, WireType};
    use alloc::vec::Vec;
    use chrono::NaiveDate;

    pub(in super::super) fn test_dates() -> impl Iterator<Item = NaiveDate> {
        [
            NaiveDate::MIN,
            NaiveDate::MAX,
            <() as EmptyState<(), NaiveDate>>::empty(),
            NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(1988, 6, 28).unwrap(),
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

    check_type_empty!(NaiveDate, via proxy with tag SealedBilrostTag);
    check_type_empty!(NaiveDate, via distinguished proxy with tag SealedBilrostTag);

    mod proptests {
        use super::*;
        check_type_test!(
            General,
            relaxed,
            from Vec<u8>,
            into NaiveDate,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                NaiveDate::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
        check_type_test!(
            General,
            distinguished,
            from Vec<u8>,
            into NaiveDate,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                NaiveDate::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
    }
}

impl ForOverwrite<(), NaiveTime> for () {
    fn for_overwrite() -> NaiveTime {
        NaiveTime::from_num_seconds_from_midnight_opt(0, 0).unwrap()
    }
}

impl EmptyState<(), NaiveTime> for () {
    fn is_empty(val: &NaiveTime) -> bool {
        (val.num_seconds_from_midnight(), val.nanosecond()) == (0, 0)
    }

    fn clear(val: &mut NaiveTime) {
        *val = <() as ForOverwrite<(), NaiveTime>>::for_overwrite();
    }
}

impl Proxiable<SealedBilrostTag> for NaiveTime {
    type Proxy = LocalProxy<u32, 4>;

    fn new_proxy() -> Self::Proxy {
        Self::Proxy::new_empty()
    }

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([
            self.hour(),
            self.minute(),
            self.second(),
            self.nanosecond(),
        ])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [hour, min, sec, nano] = proxy.into_inner();
        *self = Self::from_hms_nano_opt(hour, min, sec, nano).ok_or(OutOfDomainValue)?;
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for NaiveTime {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([hour, min, sec, nano], canon) = proxy.into_inner_distinguished();
        *self = Self::from_hms_nano_opt(hour, min, sec, nano).ok_or(OutOfDomainValue)?;
        Ok(canon)
    }
}

// NaiveTime encodes as a packed sequence of UNsigned varints with trailing zeros cut off:
// [hour, minute, second, nanosecond].
delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (NaiveTime)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod naivetime {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, check_type_test, distinguished, relaxed};
    use crate::encoding::{EmptyState, General, WireType};
    use alloc::vec::Vec;
    use chrono::NaiveTime;

    pub(in super::super) fn test_times() -> impl Iterator<Item = NaiveTime> + Clone {
        [
            NaiveTime::MIN,
            NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).unwrap(),
            <() as EmptyState<(), NaiveTime>>::empty(),
            NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            NaiveTime::from_hms_nano_opt(11, 11, 11, 111_111_111).unwrap(),
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

    check_type_empty!(NaiveTime, via proxy with tag SealedBilrostTag);
    check_type_empty!(NaiveTime, via distinguished proxy with tag SealedBilrostTag);

    mod proptests {
        use super::*;
        check_type_test!(
            General,
            relaxed,
            from Vec<u8>,
            into NaiveTime,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                NaiveTime::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
        check_type_test!(
            General,
            distinguished,
            from Vec<u8>,
            into NaiveTime,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                NaiveTime::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
    }
}

impl ForOverwrite<(), NaiveDateTime> for () {
    fn for_overwrite() -> NaiveDateTime {
        NaiveDateTime::new(
            <() as EmptyState<(), _>>::empty(),
            <() as EmptyState<(), _>>::empty(),
        )
    }
}

impl EmptyState<(), NaiveDateTime> for () {
    fn is_empty(val: &NaiveDateTime) -> bool {
        (
            val.year(),
            val.ordinal0(),
            val.num_seconds_from_midnight(),
            val.nanosecond(),
        ) == (0, 0, 0, 0)
    }

    fn clear(val: &mut NaiveDateTime) {
        *val = <() as ForOverwrite<(), NaiveDateTime>>::for_overwrite();
    }
}

#[inline(always)]
fn parts_to_naivetime(hour: i32, min: i32, sec: i32, nanos: i32) -> Option<NaiveTime> {
    NaiveTime::from_hms_nano_opt(
        hour.try_into().ok()?,
        min.try_into().ok()?,
        sec.try_into().ok()?,
        nanos.try_into().ok()?,
    )
}

impl Proxiable<SealedBilrostTag> for NaiveDateTime {
    type Proxy = LocalProxy<i32, 6>;

    fn new_proxy() -> Self::Proxy {
        Self::Proxy::new_empty()
    }

    fn encode_proxy(&self) -> Self::Proxy {
        Self::Proxy::new_without_empty_suffix([
            self.year(),
            self.ordinal0() as i32,
            self.hour() as i32,
            self.minute() as i32,
            self.second() as i32,
            self.nanosecond() as i32,
        ])
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let [year, ordinal0, hour, min, sec, nano] = proxy.into_inner();
        *self = Self::new(
            parts_to_naivedate(year, ordinal0).ok_or(OutOfDomainValue)?,
            parts_to_naivetime(hour, min, sec, nano).ok_or(OutOfDomainValue)?,
        );
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for NaiveDateTime {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        let ([year, ordinal0, hour, min, sec, nano], canon) = proxy.into_inner_distinguished();
        *self = Self::new(
            parts_to_naivedate(year, ordinal0).ok_or(OutOfDomainValue)?,
            parts_to_naivetime(hour, min, sec, nano).ok_or(OutOfDomainValue)?,
        );
        Ok(canon)
    }
}

// NaiveDateTime encodes as a packed sequence of signed varints with trailing zeros cut off:
// [year, ordinal day in year (starting at zero), hour, minute, second, nanosecond]. It can decode
// NaiveDate values as if they were truncated NaiveDateTimes. The empty value is midnight on January
// 1st of the year 0, not 1970.
delegate_proxied_encoding!(
    use encoding (Packed<Varint>) to encode proxied type (NaiveDateTime)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod naivedatetime {
    use super::naivedate::test_dates;
    use super::naivetime::test_times;
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, check_type_test, distinguished, relaxed};
    use crate::encoding::{EmptyState, General, WireType};
    use alloc::vec::Vec;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use itertools::iproduct;

    pub(in super::super) fn test_datetimes() -> impl IntoIterator<Item = NaiveDateTime> {
        [
            NaiveDateTime::MIN,
            NaiveDateTime::MAX,
            NaiveDateTime::default(),
            <() as EmptyState<(), NaiveDateTime>>::empty(),
            NaiveDateTime::new(
                NaiveDate::from_ymd_opt(-44, 3, 15).unwrap(),
                NaiveTime::from_hms_opt(12, 36, 27).unwrap(),
            ),
            NaiveDateTime::new(
                NaiveDate::from_ymd_opt(-1753, 8, 21).unwrap(),
                NaiveTime::from_hms_opt(14, 49, 8).unwrap(),
            ),
        ]
        .into_iter()
        .chain(
            iproduct!(test_dates(), test_times())
                .map(|(date, time)| NaiveDateTime::new(date, time)),
        )
    }

    #[test]
    fn check_type() {
        for datetime in test_datetimes() {
            relaxed::check_type_general(datetime, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(datetime, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(NaiveDateTime, via proxy with tag SealedBilrostTag);
    check_type_empty!(NaiveDateTime, via distinguished proxy with tag SealedBilrostTag);

    mod proptests {
        use super::*;
        check_type_test!(
            General,
            relaxed,
            from Vec<u8>,
            into NaiveDateTime,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                NaiveDateTime::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
        check_type_test!(
            General,
            distinguished,
            from Vec<u8>,
            into NaiveDateTime,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                NaiveDateTime::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
    }
}

impl ForOverwrite<(), Utc> for () {
    fn for_overwrite() -> Utc {
        Utc
    }
}

impl EmptyState<(), Utc> for () {
    fn is_empty(_: &Utc) -> bool {
        true
    }

    fn clear(_: &mut Utc) {}
}

impl Proxiable<SealedBilrostTag> for Utc {
    type Proxy = (i8, i8, i8);

    fn new_proxy() -> Self::Proxy {
        (0, 0, 0)
    }

    fn encode_proxy(&self) -> Self::Proxy {
        Self::new_proxy()
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        if proxy == Self::new_proxy() {
            Ok(())
        } else {
            Err(OutOfDomainValue)
        }
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for Utc {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        self.decode_proxy(proxy)?;
        Ok(Canonical)
    }
}

// The encoding for Utc is the same as the encoding for FixedOffset: it's a tuple of three signed
// varints (hour, minute, second) which are always zero. It always fails to decode when they are not
// all zero.
delegate_proxied_encoding!(
    use encoding ((Varint, Varint, Varint)) to encode proxied type (Utc)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod utc {
    use crate::encoding::{
        Capped, DecodeContext, DistinguishedValueDecoder, ForOverwrite, General,
        RestrictedDecodeContext, ValueDecoder, ValueEncoder,
    };
    use crate::Canonicity::{Canonical, NotCanonical};
    use crate::DecodeError;
    use crate::DecodeErrorKind::OutOfDomainValue;
    use alloc::vec::Vec;
    use chrono::{FixedOffset, Utc};

    #[test]
    fn utc_rejects_nonzero_offsets() {
        {
            let mut buf = Vec::new();
            let zero_offset = FixedOffset::east_opt(0).unwrap();
            <() as ValueEncoder<General, _>>::encode_value(&zero_offset, &mut buf);
            let mut utc = <() as ForOverwrite<(), Utc>>::for_overwrite();
            assert_eq!(
                <() as ValueDecoder<General, _>>::decode_value(
                    &mut utc,
                    Capped::new(&mut buf.as_slice()),
                    DecodeContext::default(),
                ),
                Ok(())
            );
            assert_eq!(
                <() as DistinguishedValueDecoder<General, _>>::decode_value_distinguished::<true>(
                    &mut utc,
                    Capped::new(&mut buf.as_slice()),
                    RestrictedDecodeContext::new(NotCanonical),
                ),
                Ok(Canonical)
            );
        }

        {
            let mut buf = Vec::new();
            let nonzero_offset = FixedOffset::east_opt(1000).unwrap();
            <() as ValueEncoder<General, _>>::encode_value(&nonzero_offset, &mut buf);
            let mut utc = <() as ForOverwrite<(), Utc>>::for_overwrite();
            assert_eq!(
                <() as ValueDecoder<General, _>>::decode_value(
                    &mut utc,
                    Capped::new(&mut buf.as_slice()),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
            assert_eq!(
                <() as DistinguishedValueDecoder<General, _>>::decode_value_distinguished::<true>(
                    &mut utc,
                    Capped::new(&mut buf.as_slice()),
                    RestrictedDecodeContext::new(NotCanonical),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }
    }
}

impl ForOverwrite<(), FixedOffset> for () {
    fn for_overwrite() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }
}

impl EmptyState<(), FixedOffset> for () {
    fn is_empty(val: &FixedOffset) -> bool {
        val.local_minus_utc() == 0
    }

    fn clear(val: &mut FixedOffset) {
        *val = <() as ForOverwrite<(), FixedOffset>>::for_overwrite();
    }
}

impl Proxiable<SealedBilrostTag> for FixedOffset {
    type Proxy = (i8, i8, i8);

    fn new_proxy() -> Self::Proxy {
        (0, 0, 0)
    }

    fn encode_proxy(&self) -> Self::Proxy {
        let offset_secs = self.local_minus_utc();
        let secs = (offset_secs % 60) as i8;
        let offset_mins = offset_secs / 60;
        let mins = (offset_mins % 60) as i8;
        let hours = (offset_mins / 60) as i8;
        (hours, mins, secs)
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let offset_secs = match proxy {
            (hours @ -23..=23, mins @ -59..=59, secs @ -59..=59) => {
                let total_offset = (hours as i32) * 60 * 60 + (mins as i32) * 60 + (secs as i32);

                // offsets should always have the same sign for all three components; we don't want
                // any two offsets to have the same total via different combinations.
                //
                // we enforce this even in relaxed mode because dealing with time is already bad
                // enough.
                let mut signums = [false; 3];
                for component in [hours, mins, secs] {
                    signums[(component.signum() + 1) as usize] = true;
                }
                if let [true, _, true] = signums {
                    return Err(InvalidValue);
                }

                total_offset
            }
            _ => return Err(OutOfDomainValue),
        };
        *self = Self::east_opt(offset_secs).unwrap();
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for FixedOffset {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        self.decode_proxy(proxy)?;
        Ok(Canonical)
    }
}

// The encoding for FixedOffset is (hour, minute, second) as a basic tuple of signed varints. It
// It fails to decode whenever the components have mixed signs or are out of range.
delegate_proxied_encoding!(
    use encoding ((Varint, Varint, Varint)) to encode proxied type (FixedOffset)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod fixedoffset {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, check_type_test, distinguished, relaxed};
    use crate::encoding::value_traits::ForOverwrite;
    use crate::encoding::{
        Capped, DecodeContext, DistinguishedValueDecoder, EmptyState, General,
        RestrictedDecodeContext, ValueDecoder, ValueEncoder, WireType,
    };
    use crate::Canonicity::NotCanonical;
    use crate::DecodeError;
    use crate::DecodeErrorKind::{InvalidValue, OutOfDomainValue};
    use alloc::vec::Vec;
    use chrono::FixedOffset;

    pub(in super::super) fn test_zones() -> impl Iterator<Item = FixedOffset> + Clone {
        [
            FixedOffset::east_opt(0).unwrap(),
            <() as EmptyState<(), FixedOffset>>::empty(),
            FixedOffset::west_opt(-7 * 3600 - 15 * 60).unwrap(),
            FixedOffset::east_opt(14 * 3600).unwrap(),
        ]
        .into_iter()
    }

    #[test]
    fn check_type() {
        for zone in test_zones() {
            relaxed::check_type_general(zone, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(zone, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(FixedOffset, via proxy with tag SealedBilrostTag);
    check_type_empty!(FixedOffset, via distinguished proxy with tag SealedBilrostTag);

    mod proptests {
        use super::*;
        check_type_test!(
            General,
            relaxed,
            from Vec<u8>,
            into FixedOffset,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                FixedOffset::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
        check_type_test!(
            General,
            distinguished,
            from Vec<u8>,
            into FixedOffset,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                FixedOffset::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
    }

    #[test]
    fn fixedoffset_rejects_out_of_range() {
        {
            let mut buf = Vec::new();
            let out_of_range: (i32, i32, i32) = (23, 45, 67);
            <() as ValueEncoder<General, _>>::encode_value(&out_of_range, &mut buf);
            let mut fixed = <() as ForOverwrite<(), FixedOffset>>::for_overwrite();
            assert_eq!(
                <() as ValueDecoder<General, _>>::decode_value(
                    &mut fixed,
                    Capped::new(&mut buf.as_slice()),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
            assert_eq!(
                <() as DistinguishedValueDecoder<General, _>>::decode_value_distinguished::<true>(
                    &mut fixed,
                    Capped::new(&mut buf.as_slice()),
                    RestrictedDecodeContext::new(NotCanonical),
                ),
                Err(DecodeError::new(OutOfDomainValue))
            );
        }
    }

    #[test]
    fn fixedoffset_rejects_mixed_signs() {
        {
            let mut buf = Vec::new();
            let out_of_range: (i32, i32, i32) = (10, 0, -10);
            <() as ValueEncoder<General, _>>::encode_value(&out_of_range, &mut buf);
            let mut fixed = <() as ForOverwrite<(), FixedOffset>>::for_overwrite();
            assert_eq!(
                <() as ValueDecoder<General, _>>::decode_value(
                    &mut fixed,
                    Capped::new(&mut buf.as_slice()),
                    DecodeContext::default(),
                ),
                Err(DecodeError::new(InvalidValue))
            );
            assert_eq!(
                <() as DistinguishedValueDecoder<General, _>>::decode_value_distinguished::<true>(
                    &mut fixed,
                    Capped::new(&mut buf.as_slice()),
                    RestrictedDecodeContext::new(NotCanonical),
                ),
                Err(DecodeError::new(InvalidValue))
            );
        }
    }
}

impl<Z> ForOverwrite<(), DateTime<Z>> for ()
where
    Z: TimeZone,
    (): EmptyState<(), Z::Offset>,
{
    fn for_overwrite() -> DateTime<Z> {
        DateTime::from_naive_utc_and_offset(
            <() as EmptyState<(), NaiveDateTime>>::empty(),
            <() as EmptyState<(), Z::Offset>>::empty(),
        )
    }
}

impl<Z> EmptyState<(), DateTime<Z>> for ()
where
    Z: TimeZone,
    (): EmptyState<(), Z::Offset>,
{
    fn is_empty(val: &DateTime<Z>) -> bool {
        <() as EmptyState<(), NaiveDateTime>>::is_empty(&val.naive_utc())
            && <() as EmptyState<(), _>>::is_empty(val.offset())
    }

    fn clear(val: &mut DateTime<Z>) {
        *val = <() as ForOverwrite<(), DateTime<Z>>>::for_overwrite();
    }
}

impl<Z> Proxiable<SealedBilrostTag> for DateTime<Z>
where
    Z: TimeZone,
    (): EmptyState<(), Z::Offset>,
{
    type Proxy = (NaiveDateTime, Z::Offset);

    fn new_proxy() -> Self::Proxy {
        <() as EmptyState<(), Self::Proxy>>::empty()
    }

    fn encode_proxy(&self) -> Self::Proxy {
        (self.naive_utc(), self.offset().clone())
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        let (naive, offset) = proxy;
        *self = Self::from_naive_utc_and_offset(naive, offset);
        Ok(())
    }
}

impl<Z> DistinguishedProxiable<SealedBilrostTag> for DateTime<Z>
where
    Z: TimeZone,
    (): EmptyState<(), Z::Offset>,
{
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        self.decode_proxy(proxy)?;
        Ok(Canonical)
    }
}

// The encoding for DateTime<Tz> is the same as the (NaiveDateTime, Tz::Offset) that it is composed
// of.
delegate_proxied_encoding!(
    use encoding (General) to encode proxied type (DateTime<Z>)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
    with where clause for relaxed ((): EmptyState<(), Z::Offset>)
    with generics (Z: TimeZone)
);

#[cfg(test)]
mod datetime {
    use super::fixedoffset::test_zones;
    use super::naivedatetime::test_datetimes;
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, check_type_test, distinguished, relaxed};
    use crate::encoding::{General, WireType};
    use alloc::vec::Vec;
    use chrono::{DateTime, FixedOffset, Utc};
    use itertools::iproduct;

    #[test]
    fn check_type() {
        for (naivedatetime, zone) in iproduct!(test_datetimes(), test_zones()) {
            let datetime = DateTime::<FixedOffset>::from_naive_utc_and_offset(naivedatetime, zone);
            relaxed::check_type_general(datetime, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(datetime, 123, WireType::LengthDelimited).unwrap();
        }
    }

    check_type_empty!(DateTime<Utc>, via proxy with tag SealedBilrostTag);
    check_type_empty!(DateTime<Utc>, via distinguished proxy with tag SealedBilrostTag);

    mod proptests {
        use super::*;
        check_type_test!(
            General,
            relaxed,
            from Vec<u8>,
            into DateTime<Utc>,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                DateTime::<Utc>::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
        check_type_test!(
            General,
            distinguished,
            from Vec<u8>,
            into DateTime<Utc>,
            converter(b) {
                use arbitrary::{Arbitrary, Unstructured};
                DateTime::<Utc>::arbitrary(&mut Unstructured::new(&b)).unwrap()
            },
            WireType::LengthDelimited
        );
    }
}

empty_state_via_default!(TimeDelta);

impl Proxiable<SealedBilrostTag> for TimeDelta {
    type Proxy = TimeDeltaProxy;

    fn new_proxy() -> Self::Proxy {
        TimeDeltaProxy::default()
    }

    fn encode_proxy(&self) -> Self::Proxy {
        TimeDeltaProxy {
            secs: self.num_seconds(),
            nanos: self.subsec_nanos(),
        }
    }

    fn decode_proxy(&mut self, proxy: Self::Proxy) -> Result<(), DecodeErrorKind> {
        const NOT_QUITE_I64_MIN: i64 = i64::MIN + 1;

        let (secs, nanos) = match (proxy.secs, proxy.nanos) {
            // we must be able to subtract 1 from secs no matter what
            (secs @ NOT_QUITE_I64_MIN..=0, nanos @ -999_999_999..=-1) => {
                (secs - 1, nanos + 1_000_000_000)
            }
            // we also ensure that the sign of secs and nanos matches and that nanos is in-bounds
            (secs, nanos @ 0) | (secs @ 0.., nanos @ 0..=999_999_999) => (secs, nanos),
            _ => return Err(InvalidValue),
        };
        // TimeDelta only wants to be constructed from a u32 nanos, which is its internal repr, even
        // though it only gives the value back as an i32 with the same sign as the original.
        *self = Self::new(secs, nanos as u32).ok_or(OutOfDomainValue)?;
        Ok(())
    }
}

impl DistinguishedProxiable<SealedBilrostTag> for TimeDelta {
    fn decode_proxy_distinguished(
        &mut self,
        proxy: Self::Proxy,
    ) -> Result<Canonicity, DecodeErrorKind> {
        self.decode_proxy(proxy)?;
        Ok(Canonical)
    }
}

// The encoding for TimeDelta matches that of bilrost_types::Duration.
delegate_proxied_encoding!(
    use encoding (General) to encode proxied type (TimeDelta)
    using proxy tag (SealedBilrostTag)
    with general encodings including distinguished
);

#[cfg(test)]
mod timedelta {
    use super::SealedBilrostTag;
    use crate::encoding::test::{check_type_empty, distinguished, relaxed};
    use crate::encoding::{EmptyState, WireType};
    use chrono::TimeDelta;
    use proptest::prelude::*;

    check_type_empty!(TimeDelta, via proxy with tag SealedBilrostTag);
    check_type_empty!(TimeDelta, via distinguished proxy with tag SealedBilrostTag);

    pub(in super::super) fn test_timedeltas() -> impl Iterator<Item = TimeDelta> + Clone {
        [
            TimeDelta::default(),
            TimeDelta::milliseconds(-i64::MAX), // apparently the minimum
            TimeDelta::milliseconds(i64::MAX),  // apparently the maximum
            <() as EmptyState<(), TimeDelta>>::empty(),
            TimeDelta::new(900, 10).unwrap(),
            TimeDelta::seconds(-60),
        ]
        .into_iter()
    }

    #[test]
    fn check_type() {
        for td in test_timedeltas() {
            relaxed::check_type_general(td, 123, WireType::LengthDelimited).unwrap();
            distinguished::check_type_general(td, 123, WireType::LengthDelimited).unwrap();
        }
    }

    #[cfg(feature = "time")]
    pub(in super::super) fn random_timedelta(rng: &mut impl Rng) -> TimeDelta {
        let millis = rng.gen_range(0..=i64::MAX);
        let submilli_nanos = rng.gen_range(0..1_000_000);
        let negative = rng.gen();
        milli_nanos_to_timedelta(millis, submilli_nanos, negative)
    }

    fn milli_nanos_to_timedelta(millis: i64, submilli_nanos: u32, negative: bool) -> TimeDelta {
        // compute millisecond part
        let secs = millis / 1000;
        let nanos = ((millis % 1000) * 1_000_000) as u32 + submilli_nanos;
        let td = TimeDelta::new(secs, nanos).unwrap();
        if negative {
            -td
        } else {
            td
        }
    }

    // we write these out because the arbitrary::Arbitrary impl for TimeDelta is, for some
    // reason, extremely fallible. The underlying data model for TimeDelta is also pretty weird,
    // in that its internal repr is (secs: i64, nanos: i32 /* always positive */), and it is
    // also documented to be restricted to a magnitude of plus or minus i64::MAX
    // *milliseconds* plus up to 999,999 nanoseconds, with a freely swappable sign.
    proptest! {
        #[test]
        fn check_relaxed(
            millis in 0..=i64::MAX,
            submilli_nanos in 0..=999_999u32,
            negative: bool,
            tag: u32,
        ) {
            relaxed::check_type_general(
                milli_nanos_to_timedelta(millis, submilli_nanos, negative),
                tag,
                WireType::LengthDelimited,
            )?;
        }
        #[test]
        fn check_distinguished(
            millis in 0..i64::MAX,
            submilli_nanos in 0..=999_999u32,
            negative: bool,
            tag: u32,
        ) {
            distinguished::check_type_general(
                milli_nanos_to_timedelta(millis, submilli_nanos, negative),
                tag,
                WireType::LengthDelimited,
            )?;
        }
    }
}
