use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{empty_state_via_default, for_overwrite_via_default};
use crate::encoding::{
    Capped, DecodeContext, DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState,
    ForOverwrite, General, PlainBytes, RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder,
    ValueEncoder, WireType, Wiretyped,
};
use crate::{Canonicity, DecodeError};
use alloc::borrow::Cow;
use alloc::vec::Vec;
use bytes::{Buf, BufMut};

empty_state_via_default!(&bstr::BStr);

impl Wiretyped<General> for &bstr::BStr {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for &bstr::BStr {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &&bstr::BStr, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&&***value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &&bstr::BStr, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&&***value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &&bstr::BStr) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&&***value)
    }
}

impl<'a> ValueBorrowDecoder<'a, General> for &'a bstr::BStr {
    #[inline]
    fn borrow_decode_value(
        value: &mut &'a bstr::BStr,
        mut buf: Capped<&'a [u8]>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        *value = bstr::BStr::new(buf.take_borrowed_length_delimited()?);
        Ok(())
    }
}

impl<'a> DistinguishedValueBorrowDecoder<'a, General> for &'a bstr::BStr {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut &'a bstr::BStr,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueBorrowDecoder::<General>::borrow_decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

for_overwrite_via_default!(bstr::BString);

impl EmptyState for bstr::BString {
    #[inline]
    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
    }

    #[inline]
    fn clear(&mut self) {
        Vec::clear(self)
    }
}

impl Wiretyped<General> for bstr::BString {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for bstr::BString {
    #[inline(always)]
    fn encode_value<B: BufMut + ?Sized>(value: &bstr::BString, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&**value, buf);
    }

    #[inline(always)]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &bstr::BString, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&**value, buf);
    }

    #[inline(always)]
    fn value_encoded_len(value: &bstr::BString) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&**value)
    }
}

impl ValueDecoder<General> for bstr::BString {
    #[inline(always)]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut bstr::BString,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(&mut **value, buf, ctx)
    }
}

impl DistinguishedValueDecoder<General> for bstr::BString {
    const CHECKS_EMPTY: bool = <Vec<u8> as DistinguishedValueDecoder<PlainBytes>>::CHECKS_EMPTY;

    #[inline(always)]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Self,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueDecoder::<PlainBytes>::decode_value_distinguished::<ALLOW_EMPTY>(
            &mut **value,
            buf,
            ctx,
        )
    }
}

impl Wiretyped<General> for Cow<'_, bstr::BStr> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<General> for Cow<'_, bstr::BStr> {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Self, buf: &mut B) {
        ValueEncoder::<PlainBytes>::encode_value(&&***value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &Self, buf: &mut B) {
        ValueEncoder::<PlainBytes>::prepend_value(&&***value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &Self) -> usize {
        ValueEncoder::<PlainBytes>::value_encoded_len(&&***value)
    }
}

impl ValueDecoder<General> for Cow<'_, bstr::BStr> {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Cow<bstr::BStr>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<General>::decode_value(value.to_mut(), buf, ctx)
    }
}

impl DistinguishedValueDecoder<General> for Cow<'_, bstr::BStr> {
    const CHECKS_EMPTY: bool = <bstr::BString as DistinguishedValueDecoder<General>>::CHECKS_EMPTY;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Cow<bstr::BStr>,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        DistinguishedValueDecoder::<General>::decode_value_distinguished::<ALLOW_EMPTY>(
            value.to_mut(),
            buf,
            ctx,
        )
    }
}

impl<'a> ValueBorrowDecoder<'a, General> for Cow<'a, bstr::BStr> {
    #[inline]
    fn borrow_decode_value(
        value: &mut Cow<'a, bstr::BStr>,
        buf: Capped<&'a [u8]>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut s = <&bstr::BStr>::for_overwrite();
        ValueBorrowDecoder::<General>::borrow_decode_value(&mut s, buf, ctx)?;
        *value = Cow::Borrowed(s);
        Ok(())
    }
}

impl<'a> DistinguishedValueBorrowDecoder<'a, General> for Cow<'a, bstr::BStr> {
    const CHECKS_EMPTY: bool =
        <&'a bstr::BStr as DistinguishedValueBorrowDecoder<'a, General>>::CHECKS_EMPTY;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut Cow<'a, bstr::BStr>,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueBorrowDecoder::<General>::borrow_decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod test {
    use super::{General, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(General, relaxed, from Vec<u8>, into bstr::BString,
        WireType::LengthDelimited);
    check_type_test!(General, distinguished, from Vec<u8>, into bstr::BString,
        WireType::LengthDelimited);
}
