use std::vec::Vec;

use crate::buf::ReverseBuf;
use crate::encoding::value_traits::empty_state_via_default;
use crate::encoding::{
    delegate_value_encoding, encode_varint, encoded_len_varint, prepend_varint, Capped,
    DecodeContext, DistinguishedValueDecoder, GeneralGeneric, RestrictedDecodeContext,
    ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::InvalidValue;
use crate::{Canonicity, DecodeError};
use bytes::{Buf, BufMut};

empty_state_via_default!(smol_str::SmolStr);

impl<const P: u8> Wiretyped<GeneralGeneric<P>, smol_str::SmolStr> for () {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
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
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let string_data = buf.take_length_delimited()?;
        let string_len = string_data.remaining_before_cap();
        let decoded_val = if string_len <= 23 {
            let mut inline = [0u8; 23];
            let mut buf = &mut inline[..];
            buf.put(string_data.take_all());
            let inline_string_data =
                str::from_utf8(&inline[..string_len]).map_err(|_| InvalidValue)?;
            smol_str::SmolStr::new_inline(inline_string_data)
        } else {
            // TODO: to avoid the extra copy here, we'd prefer to create an appropriately sized
            //  `Arc<[MaybeUninit<u8>]>`, read into it, and validate it in-place before turning it
            //  into an `Arc<str>` and creating a `SmolStr` directly from that.
            //
            //  Regrettably we can't use `SmolStrBuilder` because the chunks we read may not all be
            //  valid utf8 on their own, and there are no remotely MSRV-compatible ways to perform
            //  the alternative process. So, larger strings will just get copied twice.
            let mut buf = Vec::with_capacity(string_len);
            buf.put(string_data.take_all());
            let allocated_string_data = str::from_utf8(&buf).map_err(|_| InvalidValue)?;
            smol_str::SmolStr::new(allocated_string_data)
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
        ctx: RestrictedDecodeContext,
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
    use crate::encoding::test::check_type_test;
    use crate::encoding::General;
    use alloc::string::String;
    check_type_test!(General, relaxed, from String,
                     into smol_str::SmolStr, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from String, into smol_str::SmolStr,
                     WireType::LengthDelimited);
}
