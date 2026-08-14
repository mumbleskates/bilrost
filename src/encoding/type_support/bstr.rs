use crate::buf::ReverseBuf;
use crate::encoding::schema::{Schema, ValueRepr};
use crate::encoding::value_traits::{empty_state_via_default, for_overwrite_via_default};
use crate::encoding::{
    delegate_value_encoding, impl_cow_value_encoding, Capped, DecodeContext,
    DistinguishedValueBorrowDecoder, DistinguishedValueDecoder, EmptyState, GeneralGeneric,
    PlainBytes, RestrictedDecodeContext, ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType,
    Wiretyped,
};
use crate::{Canonicity, DecodeError};
use alloc::boxed::Box;
use alloc::vec::Vec;
use bytes::{Buf, BufMut};
use core::fmt::Display;

empty_state_via_default!(&'a bstr::BStr, with generics ('a));

impl<const P: u8> Wiretyped<GeneralGeneric<P>, &bstr::BStr> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueRepr<GeneralGeneric<P>, &bstr::BStr> for () {
    fn repr(schema: &Schema) -> Box<dyn Display> {
        <() as ValueRepr<PlainBytes, &[u8]>>::repr(schema)
    }
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, &bstr::BStr> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &&bstr::BStr, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&&***value, buf)
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &&bstr::BStr, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&&***value, buf)
    }

    #[inline]
    fn value_encoded_len(value: &&bstr::BStr) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&&***value)
    }
}

impl<'a, const P: u8> ValueBorrowDecoder<'a, GeneralGeneric<P>, &'a bstr::BStr> for () {
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

impl<'a, const P: u8> DistinguishedValueBorrowDecoder<'a, GeneralGeneric<P>, &'a bstr::BStr>
    for ()
{
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut &'a bstr::BStr,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueBorrowDecoder<GeneralGeneric<P>, _>>::borrow_decode_value(
            value,
            buf,
            ctx.into_inner(),
        )?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod ref_bstr {
    crate::encoding::test::check_borrowable!(
        borrowed: bstr::BStr,
        encoding: crate::encoding::General,
        converter(s: Vec<u8>) { bstr::BString::new(s) },
    );
}

for_overwrite_via_default!(bstr::BString);

impl EmptyState<(), bstr::BString> for () {
    #[inline]
    fn is_empty(val: &bstr::BString) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut bstr::BString) {
        val.clear();
    }
}

impl<const P: u8> Wiretyped<GeneralGeneric<P>, bstr::BString> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueRepr<GeneralGeneric<P>, bstr::BString> for () {
    fn repr(schema: &Schema) -> Box<dyn Display> {
        <() as ValueRepr<PlainBytes, &[u8]>>::repr(schema)
    }
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, bstr::BString> for () {
    #[inline(always)]
    fn encode_value<B: BufMut + ?Sized>(value: &bstr::BString, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::encode_value(&**value, buf);
    }

    #[inline(always)]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &bstr::BString, buf: &mut B) {
        <() as ValueEncoder<PlainBytes, _>>::prepend_value(&**value, buf);
    }

    #[inline(always)]
    fn value_encoded_len(value: &bstr::BString) -> usize {
        <() as ValueEncoder<PlainBytes, _>>::value_encoded_len(&**value)
    }
}

impl<const P: u8> ValueDecoder<GeneralGeneric<P>, bstr::BString> for () {
    #[inline(always)]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut bstr::BString,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        <() as ValueDecoder<PlainBytes, _>>::decode_value(&mut **value, buf, ctx)
    }
}

impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, bstr::BString> for () {
    const CHECKS_EMPTY: bool = <() as DistinguishedValueDecoder<PlainBytes, Vec<u8>>>::CHECKS_EMPTY;

    #[inline(always)]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut bstr::BString,
        buf: Capped<impl Buf + ?Sized>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as DistinguishedValueDecoder<PlainBytes, _>>::decode_value_distinguished::<ALLOW_EMPTY>(
            &mut **value,
            buf,
            ctx,
        )
    }
}

delegate_value_encoding!(
    encoding (GeneralGeneric<P>) borrows type (bstr::BString) as owned including distinguished
    with generics (const P: u8)
);

impl_cow_value_encoding!(
    borrowed bstr::BStr,
    owned bstr::BString,
    encoding GeneralGeneric<P>,
    with generic (const P: u8)
);

#[cfg(test)]
mod test {
    use super::Vec;
    use crate::encoding::test::check_type_test;
    use crate::encoding::General;
    check_type_test!(General, relaxed, from Vec<u8>, into bstr::BString,
        WireType::LengthDelimited);
    check_type_test!(General, distinguished, from Vec<u8>, into bstr::BString,
        WireType::LengthDelimited);
}
