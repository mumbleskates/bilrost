use crate::buf::ReverseBuf;
use crate::encoding::schema::{Schema, ValueRepr};
use crate::encoding::value_traits::for_overwrite_via_default;
use crate::encoding::{
    delegate_value_encoding, encode_varint, encoded_len_varint, prepend_varint, Capped,
    DecodeContext, DistinguishedValueDecoder, EmptyState, General, GeneralGeneric,
    RestrictedDecodeContext, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{Canonicity, DecodeError};
use alloc::boxed::Box;
use bytes::{Buf, BufMut};
use core::fmt::Display;

for_overwrite_via_default!(bytestring::ByteString);

impl EmptyState<(), bytestring::ByteString> for () {
    #[inline]
    fn is_empty(val: &bytestring::ByteString) -> bool {
        val.is_empty()
    }

    #[inline]
    fn clear(val: &mut bytestring::ByteString) {
        *val = Default::default();
    }
}

impl<const P: u8> Wiretyped<GeneralGeneric<P>, bytestring::ByteString> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueRepr<GeneralGeneric<P>, bytestring::ByteString> for () {
    fn repr(schema: &Schema) -> Box<dyn Display> {
        <() as ValueRepr<General, &str>>::repr(schema)
    }
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, bytestring::ByteString> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &bytestring::ByteString, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &bytestring::ByteString, buf: &mut B) {
        buf.prepend_slice(value.as_bytes());
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &bytestring::ByteString) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }
}

impl<const P: u8> ValueDecoder<GeneralGeneric<P>, bytestring::ByteString> for () {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut bytestring::ByteString,
        mut buf: Capped<B>,
        _ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut string_data = buf.take_length_delimited()?;
        let string_len = string_data.remaining_before_cap();
        *value = bytestring::ByteString::try_from(string_data.copy_to_bytes(string_len))
            .map_err(|_| DecodeError::new(InvalidValue))?;
        Ok(())
    }
}

impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, bytestring::ByteString> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut bytestring::ByteString,
        buf: Capped<impl Buf + ?Sized>,
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueDecoder<GeneralGeneric<P>, _>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (GeneralGeneric<P>) borrows type (bytestring::ByteString) as owned including distinguished
    with generics (const P: u8)
);

#[cfg(test)]
mod test {
    use crate::encoding::test::check_type_test;
    use crate::encoding::General;
    use alloc::string::String;
    check_type_test!(General, relaxed, from String,
        into bytestring::ByteString, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from String, into bytestring::ByteString,
        WireType::LengthDelimited);
}
