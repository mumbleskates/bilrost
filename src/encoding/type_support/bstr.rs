use crate::buf::ReverseBuf;
use crate::encoding::value_traits::{empty_state_via_default, for_overwrite_via_default};
use crate::encoding::{
    impl_cow_value_encoding, Capped, DecodeContext, DistinguishedValueBorrowDecoder,
    DistinguishedValueDecoder, EmptyState, General, PlainBytes, RestrictedDecodeContext,
    ValueBorrowDecoder, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::{Canonicity, DecodeError};
use alloc::vec::Vec;
use bytes::{Buf, BufMut};

empty_state_via_default!(&bstr::BStr);

impl<const G: u8> Wiretyped<General<G>> for &bstr::BStr {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const G: u8> ValueEncoder<General<G>> for &bstr::BStr {
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

impl<'a, const G: u8> ValueBorrowDecoder<'a, General<G>> for &'a bstr::BStr {
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

impl<'a, const G: u8> DistinguishedValueBorrowDecoder<'a, General<G>> for &'a bstr::BStr {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut &'a bstr::BStr,
        buf: Capped<&'a [u8]>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        ValueBorrowDecoder::<General<G>>::borrow_decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

#[cfg(test)]
mod ref_bstr {
    crate::encoding::test::check_borrowable!(
        borrowed: bstr::BStr,
        encoding: crate::encoding::GeneralInMessage,
        converter(s: Vec<u8>) { bstr::BString::new(s) },
    );
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

impl<const G: u8> Wiretyped<General<G>> for bstr::BString {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const G: u8> ValueEncoder<General<G>> for bstr::BString {
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

impl<const G: u8> ValueDecoder<General<G>> for bstr::BString {
    #[inline(always)]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut bstr::BString,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        ValueDecoder::<PlainBytes>::decode_value(&mut **value, buf, ctx)
    }
}

impl<const G: u8> DistinguishedValueDecoder<General<G>> for bstr::BString {
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

impl_cow_value_encoding!(
    borrowed bstr::BStr,
    owned bstr::BString,
    encoding General<G>,
    with generic (const G: u8)
);

#[cfg(test)]
mod test {
    use super::Vec;
    use crate::encoding::GeneralInMessage;
    use crate::encoding::test::check_type_test;
    check_type_test!(GeneralInMessage, relaxed, from Vec<u8>, into bstr::BString,
        WireType::LengthDelimited);
    check_type_test!(GeneralInMessage, distinguished, from Vec<u8>, into bstr::BString,
        WireType::LengthDelimited);
}
