use crate::buf::ReverseBuf;
use crate::encoding::underived::{
    underived_decode, underived_decode_distinguished, underived_encode, underived_encoded_len,
    underived_prepend,
};
use crate::encoding::{
    BorrowDecoder, Capped, DecodeContext, Decoder, DistinguishedBorrowDecoder,
    DistinguishedDecoder, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState,
    Encoder, ForOverwrite, RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder,
    WireType, Wiretyped,
};
use crate::{Canonicity, DecodeError};
use bytes::{Buf, BufMut};
use core::mem;
use core::ops::{Range, RangeInclusive};

impl<T, Estart, Eend> Wiretyped<(Estart, Eend), Range<T>> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T, Estart, Eend> ForOverwrite<(Estart, Eend), Range<T>> for ()
where
    (): ForOverwrite<Estart, T> + ForOverwrite<Eend, T>,
{
    #[inline]
    fn for_overwrite() -> Range<T> {
        <() as ForOverwrite<Estart, T>>::for_overwrite()
            ..<() as ForOverwrite<Eend, T>>::for_overwrite()
    }
}

impl<T, Estart, Eend> EmptyState<(Estart, Eend), Range<T>> for ()
where
    (): EmptyState<Estart, T> + EmptyState<Eend, T>,
{
    #[inline]
    fn empty() -> Range<T> {
        <() as EmptyState<Estart, T>>::empty()..<() as EmptyState<Eend, T>>::empty()
    }

    #[inline]
    fn is_empty(val: &Range<T>) -> bool {
        <() as EmptyState<Estart, T>>::is_empty(&val.start)
            && <() as EmptyState<Eend, T>>::is_empty(&val.end)
    }

    #[inline]
    fn clear(val: &mut Range<T>) {
        <() as EmptyState<Estart, T>>::clear(&mut val.start);
        <() as EmptyState<Eend, T>>::clear(&mut val.end);
    }
}

impl<T, Estart, Eend> ValueEncoder<(Estart, Eend), Range<T>> for ()
where
    (): Encoder<Estart, T> + Encoder<Eend, T>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &Range<T>, buf: &mut B) {
        underived_encode!(Range {
            0: Estart => start: &value.start,
            1: Eend => end: &value.end,
        }, buf);
    }

    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Range<T>, buf: &mut B) {
        underived_prepend!(Range {
            1: Eend => end: &value.end,
            0: Estart => start: &value.start,
        }, buf);
    }

    fn value_encoded_len(value: &Range<T>) -> usize {
        underived_encoded_len!(Range {
            0: Estart => start: &value.start,
            1: Eend => end: &value.end,
        })
    }
}

impl<T, Estart, Eend> ValueDecoder<(Estart, Eend), Range<T>> for ()
where
    (): Decoder<Estart, T> + Decoder<Eend, T>,
{
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Range<T>,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        underived_decode!(Range {
            0: Estart => start: &mut value.start,
            1: Eend => end: &mut value.end,
        }, owned, buf, ctx)
    }
}

impl<T, Estart, Eend> DistinguishedValueDecoder<(Estart, Eend), Range<T>> for ()
where
    (): DistinguishedDecoder<Estart, T> + DistinguishedDecoder<Eend, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Range<T>,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        underived_decode_distinguished!(Range {
            0: Estart => start: &mut value.start,
            1: Eend => end: &mut value.end,
        }, owned, buf, ctx)
    }
}

impl<'a, T, Estart, Eend> ValueBorrowDecoder<'a, (Estart, Eend), Range<T>> for ()
where
    (): BorrowDecoder<'a, Estart, T> + BorrowDecoder<'a, Eend, T>,
{
    fn borrow_decode_value(
        value: &mut Range<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        underived_decode!(Range {
            0: Estart => start: &mut value.start,
            1: Eend => end: &mut value.end,
        }, borrowed, buf, ctx)
    }
}

impl<'a, T, Estart, Eend> DistinguishedValueBorrowDecoder<'a, (Estart, Eend), Range<T>> for ()
where
    (): DistinguishedBorrowDecoder<'a, Estart, T> + DistinguishedBorrowDecoder<'a, Eend, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Range<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        underived_decode_distinguished!(Range {
            0: Estart => start: &mut value.start,
            1: Eend => end: &mut value.end,
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
            (Fixed, Fixed),
            relaxed,
            core::ops::Range<u32>,
            WireType::LengthDelimited
        );
        crate::encoding::test::check_type_test!(
            (Fixed, Fixed),
            distinguished,
            core::ops::Range<u32>,
            WireType::LengthDelimited
        );
    }
}

// When decoding or otherwise modifying `RangeInclusive`, we have to do a little dance. The type
// doesn't provide &mut access to its entries until it is decomposed so we swap it, decompose it,
// modify, and then re-compose and swap it back.

impl<T, Estart, Eend> Wiretyped<(Estart, Eend), RangeInclusive<T>> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<T, Estart, Eend> ForOverwrite<(Estart, Eend), RangeInclusive<T>> for ()
where
    (): ForOverwrite<Estart, T> + ForOverwrite<Eend, T>,
{
    #[inline]
    fn for_overwrite() -> RangeInclusive<T> {
        <() as ForOverwrite<Estart, T>>::for_overwrite()
            ..=<() as ForOverwrite<Eend, T>>::for_overwrite()
    }
}

impl<T, Estart, Eend> EmptyState<(Estart, Eend), RangeInclusive<T>> for ()
where
    (): EmptyState<Estart, T> + EmptyState<Eend, T>,
{
    #[inline]
    fn empty() -> RangeInclusive<T> {
        <() as EmptyState<Estart, T>>::empty()..=<() as EmptyState<Eend, T>>::empty()
    }

    #[inline]
    fn is_empty(val: &RangeInclusive<T>) -> bool {
        <() as EmptyState<Estart, T>>::is_empty(val.start())
            && <() as EmptyState<Eend, T>>::is_empty(val.end())
    }

    #[inline]
    fn clear(val: &mut RangeInclusive<T>) {
        let (mut start, mut end) = mem::replace(
            val,
            <() as ForOverwrite<Estart, T>>::for_overwrite()
                ..=<() as ForOverwrite<Eend, T>>::for_overwrite(),
        )
        .into_inner();

        <() as EmptyState<Estart, T>>::clear(&mut start);
        <() as EmptyState<Eend, T>>::clear(&mut end);

        drop(mem::replace(val, start..=end));
    }
}

impl<T, Estart, Eend> ValueEncoder<(Estart, Eend), RangeInclusive<T>> for ()
where
    (): Encoder<Estart, T> + Encoder<Eend, T>,
{
    fn encode_value<B: BufMut + ?Sized>(value: &RangeInclusive<T>, buf: &mut B) {
        underived_encode!(RangeInclusive {
            0: Estart => start: value.start(),
            1: Eend => end: value.end(),
        }, buf);
    }

    fn prepend_value<B: ReverseBuf + ?Sized>(value: &RangeInclusive<T>, buf: &mut B) {
        underived_prepend!(RangeInclusive {
            1: Eend => end: value.end(),
            0: Estart => start: value.start(),
        }, buf);
    }

    fn value_encoded_len(value: &RangeInclusive<T>) -> usize {
        underived_encoded_len!(RangeInclusive {
            0: Estart => start: value.start(),
            1: Eend => end: value.end(),
        })
    }
}

impl<T, Estart, Eend> ValueDecoder<(Estart, Eend), RangeInclusive<T>> for ()
where
    (): ForOverwrite<Estart, T> + ForOverwrite<Eend, T> + Decoder<Estart, T> + Decoder<Eend, T>,
{
    fn decode_value<B: Buf + ?Sized>(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<Estart, T>>::for_overwrite()
                ..=<() as ForOverwrite<Eend, T>>::for_overwrite(),
        )
        .into_inner();

        underived_decode!(RangeInclusive {
            0: Estart => start: &mut start,
            1: Eend => end: &mut end,
        }, owned, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(())
    }
}

impl<T, Estart, Eend> DistinguishedValueDecoder<(Estart, Eend), RangeInclusive<T>> for ()
where
    (): ForOverwrite<Estart, T>
        + ForOverwrite<Eend, T>
        + DistinguishedDecoder<Estart, T>
        + DistinguishedDecoder<Eend, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<Estart, T>>::for_overwrite()
                ..=<() as ForOverwrite<Eend, T>>::for_overwrite(),
        )
        .into_inner();

        let canon = underived_decode_distinguished!(RangeInclusive {
            0: Estart => start: &mut start,
            1: Eend => end: &mut end,
        }, owned, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(canon)
    }
}

impl<'a, T, Estart, Eend> ValueBorrowDecoder<'a, (Estart, Eend), RangeInclusive<T>> for ()
where
    (): ForOverwrite<Estart, T>
        + ForOverwrite<Eend, T>
        + BorrowDecoder<'a, Estart, T>
        + BorrowDecoder<'a, Eend, T>,
{
    fn borrow_decode_value(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<Estart, T>>::for_overwrite()
                ..=<() as ForOverwrite<Eend, T>>::for_overwrite(),
        )
        .into_inner();

        underived_decode!(RangeInclusive {
            0: Estart => start: &mut start,
            1: Eend => end: &mut end,
        }, borrowed, buf, ctx)?;

        drop(mem::replace(value, start..=end));
        Ok(())
    }
}

impl<'a, T, Estart, Eend> DistinguishedValueBorrowDecoder<'a, (Estart, Eend), RangeInclusive<T>>
    for ()
where
    (): ForOverwrite<Estart, T>
        + ForOverwrite<Eend, T>
        + DistinguishedBorrowDecoder<'a, Estart, T>
        + DistinguishedBorrowDecoder<'a, Eend, T>,
{
    const CHECKS_EMPTY: bool = true;

    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut RangeInclusive<T>,
        mut buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let (mut start, mut end) = mem::replace(
            value,
            <() as ForOverwrite<Estart, T>>::for_overwrite()
                ..=<() as ForOverwrite<Eend, T>>::for_overwrite(),
        )
        .into_inner();

        let canon = underived_decode_distinguished!(RangeInclusive {
            0: Estart => start: &mut start,
            1: Eend => end: &mut end,
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
            (Fixed, Fixed),
            relaxed,
            core::ops::RangeInclusive<u32>,
            WireType::LengthDelimited
        );
        crate::encoding::test::check_type_test!(
            (Fixed, Fixed),
            distinguished,
            core::ops::RangeInclusive<u32>,
            WireType::LengthDelimited
        );
    }
}
