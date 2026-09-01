use crate::buf::ReverseBuf;
use crate::encoding::schema::ValueRepr;
use crate::encoding::value_traits::empty_state_via_default;
use crate::encoding::{
    delegate_value_encoding, encode_varint, encoded_len_varint, prepend_varint, read_arc_str,
    Capped, DecodeContext, DistinguishedValueDecoder, General, GeneralGeneric,
    RestrictedDecodeContext, ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{Canonicity, DecodeError};
use bytes::{Buf, BufMut};
use core::str::from_utf8;

empty_state_via_default!(smol_str::SmolStr);

impl<const P: u8> Wiretyped<GeneralGeneric<P>, smol_str::SmolStr> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const P: u8> ValueRepr<GeneralGeneric<P>, smol_str::SmolStr> for () {
    fn repr(schema: &crate::encoding::schema::Schema) -> alloc::boxed::Box<dyn core::fmt::Display> {
        <() as ValueRepr<General, &str>>::repr(schema)
    }
}

impl<const P: u8> ValueEncoder<GeneralGeneric<P>, smol_str::SmolStr> for () {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &smol_str::SmolStr, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_bytes());
    }

    #[inline]
    fn prepend_value<B: ReverseBuf + ?Sized>(value: &smol_str::SmolStr, buf: &mut B) {
        buf.prepend_slice(value.as_bytes());
        prepend_varint(value.len() as u64, buf);
    }

    #[inline]
    fn value_encoded_len(value: &smol_str::SmolStr) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }
}

impl<const P: u8> ValueDecoder<GeneralGeneric<P>, smol_str::SmolStr> for () {
    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut smol_str::SmolStr,
        mut buf: Capped<B>,
        _ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        let string_data = buf.take_length_delimited()?;
        let string_len = string_data.remaining_before_cap();
        let decoded_val = if string_len <= 23 {
            // For short strings, we can always just create the result with an inline value.
            let mut inline = [0u8; 23];
            let mut buf = &mut inline[..];
            buf.put(string_data.take_all());
            let inline_string_data = from_utf8(&inline[..string_len]).map_err(|_| InvalidValue)?;
            smol_str::SmolStr::new_inline(inline_string_data)
        } else {
            smol_str::SmolStr::from(read_arc_str(string_data)?)
        };
        *value = decoded_val;
        Ok(())
    }
}

impl<const P: u8> DistinguishedValueDecoder<GeneralGeneric<P>, smol_str::SmolStr> for () {
    const CHECKS_EMPTY: bool = false;

    #[inline]
    fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
        value: &mut smol_str::SmolStr,
        buf: Capped<impl Buf + ?Sized>,
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        <() as ValueDecoder<GeneralGeneric<P>, _>>::decode_value(value, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

delegate_value_encoding!(
    encoding (GeneralGeneric<P>) borrows type (smol_str::SmolStr) as owned including distinguished
    with generics (const P: u8)
);

#[cfg(test)]
mod test {
    use crate::encoding::test::{check_type_test, decode_noncontiguous_pointered_str};
    use crate::encoding::General;
    use alloc::string::String;

    check_type_test!(General, relaxed, from String,
                     into smol_str::SmolStr, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from String, into smol_str::SmolStr,
                     WireType::LengthDelimited);

    #[test]
    fn noncontiguous() {
        decode_noncontiguous_pointered_str::<smol_str::SmolStr>();
    }
}
