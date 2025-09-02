use crate::buf::ReverseBuf;
use crate::encoding::underived::{
    underived_decode, underived_decode_distinguished, underived_encode, underived_encoded_len,
    underived_prepend,
};
use crate::encoding::{
    BorrowDecoder, Capped, DecodeContext, Decoder, DistinguishedBorrowDecoder,
    DistinguishedDecoder, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState,
    Encoder, ForOverwrite, General, RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder,
    ValueEncoder, WireType, Wiretyped,
};
use crate::{Canonicity, DecodeError};
use bytes::{Buf, BufMut};
use core::mem;
use core::ops::{Range, RangeInclusive};

/// Encoding that encodes ranges as (start, end) tuples.
pub type RangeAsTuple<E = General> = (E,);

impl<T, E> Wiretyped<RangeAsTuple<E>, Range<T>> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T, E> ForOverwrite<RangeAsTuple<E>, Range<T>> for ()
where
    (): ForOverwrite<E, T>,
{
    #[inline]
    fn for_overwrite() -> Range<T> {
        <() as ForOverwrite<E, T>>::for_overwrite()..<() as ForOverwrite<E, T>>::for_overwrite()
    }
}

impl<T, E> EmptyState<RangeAsTuple<E>, Range<T>> for ()
where
    (): EmptyState<E, T>,
{
    #[inline]
    fn empty() -> Range<T> {
        <() as EmptyState<E, T>>::empty()..<() as EmptyState<E, T>>::empty()
    }

    #[inline]
    fn is_empty(val: &Range<T>) -> bool {
        <() as EmptyState<E, T>>::is_empty(&val.start)
            && <() as EmptyState<E, T>>::is_empty(&val.end)
    }

    #[inline]
    fn clear(val: &mut Range<T>) {
        <() as EmptyState<E, T>>::clear(&mut val.start);
        <() as EmptyState<E, T>>::clear(&mut val.end);
    }
}

impl<T, E> ValueEncoder<RangeAsTuple<E>, Range<T>> for ()
where
    (): Encoder<E, T>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &Range<T>, buf: &mut B) {
        underived_encode!(Range {
            0: E => start: &value.start,
            1: E => end: &value.end,
        }, buf);
    }

    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Range<T>, buf: &mut B) {
        underived_prepend!(Range {
            1: E => end: &value.end,
            0: E => start: &value.start,
        }, buf);
    }

    fn value_encoded_len(value: &Range<T>) -> usize {
        underived_encoded_len!(Range {
            0: E => start: &value.start,
            1: E => end: &value.end,
        })
    }
}

impl<T, E> ValueDecoder<RangeAsTuple<E>, Range<T>> for ()
where
    (): Decoder<E, T>,
{
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Range<T>,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        underived_decode!(Range {
            0: E => start: &mut value.start,
            1: E => end: &mut value.end,
        }, owned, buf, ctx)
    }
}

impl<T, E> DistinguishedValueDecoder<RangeAsTuple<E>, Range<T>> for ()
where
    (): DistinguishedDecoder<E, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Range<T>,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        underived_decode_distinguished!(Range {
            0: E => start: &mut value.start,
            1: E => end: &mut value.end,
        }, owned, buf, ctx)
    }
}

impl<'a, T, E> ValueBorrowDecoder<'a, RangeAsTuple<E>, Range<T>> for ()
where
    (): BorrowDecoder<'a, E, T>,
{
    fn borrow_decode_value(
        value: &mut Range<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        underived_decode!(Range {
            0: E => start: &mut value.start,
            1: E => end: &mut value.end,
        }, borrowed, buf, ctx)
    }
}

impl<'a, T, E> DistinguishedValueBorrowDecoder<'a, RangeAsTuple<E>, Range<T>> for ()
where
    (): DistinguishedBorrowDecoder<'a, E, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Range<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        underived_decode_distinguished!(Range {
            0: E => start: &mut value.start,
            1: E => end: &mut value.end,
        }, borrowed, buf, ctx)
    }
}

#[cfg(test)]
mod range {
    mod general {
        use crate::encoding::General;
        crate::encoding::test::check_type_test!(
            General,
            relaxed,
            core::ops::Range<u16>,
            WireType::LengthDelimited
        );
        crate::encoding::test::check_type_test!(
            General,
            distinguished,
            core::ops::Range<u16>,
            WireType::LengthDelimited
        );
    }
    mod fixed {
        use crate::encoding::Fixed;
        crate::encoding::test::check_type_test!(
            (Fixed,),
            relaxed,
            core::ops::Range<u32>,
            WireType::LengthDelimited
        );
        crate::encoding::test::check_type_test!(
            (Fixed,),
            distinguished,
            core::ops::Range<u32>,
            WireType::LengthDelimited
        );
    }
}

// When decoding or otherwise modifying `RangeInclusive`, we have to do a little dance. The type
// doesn't provide &mut access to its entries until it is decomposed so we swap it, decompose it,
// modify, and then re-compose and swap it back.

impl<T, E> Wiretyped<RangeAsTuple<E>, RangeInclusive<T>> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T, E> ForOverwrite<RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): ForOverwrite<E, T>,
{
    #[inline]
    fn for_overwrite() -> RangeInclusive<T> {
        <() as ForOverwrite<E, T>>::for_overwrite()..=<() as ForOverwrite<E, T>>::for_overwrite()
    }
}

impl<T, E> EmptyState<RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): EmptyState<E, T>,
{
    #[inline]
    fn empty() -> RangeInclusive<T> {
        <() as EmptyState<E, T>>::empty()..=<() as EmptyState<E, T>>::empty()
    }

    #[inline]
    fn is_empty(val: &RangeInclusive<T>) -> bool {
        <() as EmptyState<E, T>>::is_empty(val.start())
            && <() as EmptyState<E, T>>::is_empty(val.end())
    }

    #[inline]
    fn clear(val: &mut RangeInclusive<T>) {
        let (mut start, mut end) = mem::replace(
            val,
            <() as ForOverwrite<E, T>>::for_overwrite()
                ..=<() as ForOverwrite<E, T>>::for_overwrite(),
        )
        .into_inner();

        <() as EmptyState<E, T>>::clear(&mut start);
        <() as EmptyState<E, T>>::clear(&mut end);

        drop(mem::replace(val, start..=end));
    }
}

impl<T, E> ValueEncoder<RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): Encoder<E, T>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &RangeInclusive<T>, buf: &mut B) {
        underived_encode!(RangeInclusive {
            0: E => start: value.start(),
            1: E => end: value.end(),
        }, buf);
    }

    fn prepend_value<B: ReverseBuf + ?Sized>(value: &RangeInclusive<T>, buf: &mut B) {
        underived_prepend!(RangeInclusive {
            1: E => end: value.end(),
            0: E => start: value.start(),
        }, buf);
    }

    fn value_encoded_len(value: &RangeInclusive<T>) -> usize {
        underived_encoded_len!(RangeInclusive {
            0: E => start: value.start(),
            1: E => end: value.end(),
        })
    }
}

impl<T, E> ValueDecoder<RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): ForOverwrite<E, T> + Decoder<E, T>,
{
    fn decode_value<B: Buf + ?Sized>(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<E, T>>::for_overwrite()
                ..=<() as ForOverwrite<E, T>>::for_overwrite(),
        )
        .into_inner();

        underived_decode!(RangeInclusive {
            0: E => start: &mut start,
            1: E => end: &mut end,
        }, owned, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(())
    }
}

impl<T, E> DistinguishedValueDecoder<RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): ForOverwrite<E, T> + DistinguishedDecoder<E, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<E, T>>::for_overwrite()
                ..=<() as ForOverwrite<E, T>>::for_overwrite(),
        )
        .into_inner();

        let canon = underived_decode_distinguished!(RangeInclusive {
            0: E => start: &mut start,
            1: E => end: &mut end,
        }, owned, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(canon)
    }
}

impl<'a, T, E> ValueBorrowDecoder<'a, RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): ForOverwrite<E, T> + BorrowDecoder<'a, E, T>,
{
    fn borrow_decode_value(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<E, T>>::for_overwrite()
                ..=<() as ForOverwrite<E, T>>::for_overwrite(),
        )
        .into_inner();

        underived_decode!(RangeInclusive {
            0: E => start: &mut start,
            1: E => end: &mut end,
        }, borrowed, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(())
    }
}

impl<'a, T, E> DistinguishedValueBorrowDecoder<'a, RangeAsTuple<E>, RangeInclusive<T>> for ()
where
    (): ForOverwrite<E, T> + DistinguishedBorrowDecoder<'a, E, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<E, T>>::for_overwrite()
                ..=<() as ForOverwrite<E, T>>::for_overwrite(),
        )
        .into_inner();

        let canon = underived_decode_distinguished!(RangeInclusive {
            0: E => start: &mut start,
            1: E => end: &mut end,
        }, borrowed, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(canon)
    }
}

#[cfg(test)]
mod range_inclusive {
    mod general {
        use crate::encoding::General;
        crate::encoding::test::check_type_test!(
            General,
            relaxed,
            core::ops::RangeInclusive<u16>,
            WireType::LengthDelimited
        );
        crate::encoding::test::check_type_test!(
            General,
            distinguished,
            core::ops::RangeInclusive<u16>,
            WireType::LengthDelimited
        );
    }
    mod fixed {
        use crate::encoding::Fixed;
        crate::encoding::test::check_type_test!(
            (Fixed,),
            relaxed,
            core::ops::RangeInclusive<u32>,
            WireType::LengthDelimited
        );
        crate::encoding::test::check_type_test!(
            (Fixed,),
            distinguished,
            core::ops::RangeInclusive<u32>,
            WireType::LengthDelimited
        );
    }
}
