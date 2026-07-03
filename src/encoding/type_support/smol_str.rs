use crate::buf::ReverseBuf;
use crate::encoding::schema::ValueRepr;
use crate::encoding::value_traits::empty_state_via_default;
use crate::encoding::{
    delegate_value_encoding, encode_varint, encoded_len_varint, prepend_varint, Capped,
    DecodeContext, DistinguishedValueDecoder, General, GeneralGeneric, RestrictedDecodeContext,
    ValueDecoder, ValueEncoder, WireType, Wiretyped,
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
    fn repr(
        schema: &crate::encoding::schema::Schema,
    ) -> std::prelude::v1::Box<dyn core::fmt::Display> {
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
        _ctx: DecodeContext,
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
        } else if let (Some(whole_value_bytes), false) = (
            string_data.chunk().get(..string_len),
            cfg!(feature = "test-smolstr-force-noncontiguous"),
        ) {
            // Otherwise, the string is too long to fit inline, but it is available contiguously.
            //
            // We prefer taking this path even if the next branch where we create an Arc directly
            // is available, since on this path we can validate the string data *before* we copy
            // it rather than after.
            let input_string_data = from_utf8(whole_value_bytes).map_err(|_| InvalidValue)?;
            let res = smol_str::SmolStr::new(input_string_data);
            // We got the data by reading the chunk from the buf directly, so we must advance it
            // manually as well.
            buf.advance(string_len);
            res
        } else {
            #[cfg(all(rustc_1_82, not(feature = "forbid-unsafe")))]
            {
                // Otherwise, the data won't fit inline and isn't contiguous, so we have to copy it
                // out.
                //
                // We prefer this fast-path, when available: we create a preallocated Arc of the
                // right size, copy the data into it, validate it, and then convert it directly
                // into the result type which retains the Arc.
                use alloc::sync::Arc;
                #[allow(clippy::incompatible_msrv)]
                let mut arc = Arc::new_uninit_slice(string_len);
                let mut arc_slice = Arc::get_mut(&mut arc).unwrap();
                arc_slice.put(string_data.take_all());
                // Check that we wrote every byte in the buf
                debug_assert!(arc_slice.is_empty());
                // SAFETY: we just wrote to the buf's entire contents
                #[allow(clippy::incompatible_msrv)]
                let arc = unsafe { arc.assume_init() };
                // Validate that buf contains utf8
                from_utf8(&arc).map_err(|_| InvalidValue)?;
                // SAFETY: we just validated the contents of the arc are valid for str
                let arc = unsafe { core::mem::transmute::<Arc<[u8]>, Arc<str>>(arc) };
                smol_str::SmolStr::from(arc)
            }
            #[cfg(any(not(rustc_1_82), feature = "forbid-unsafe"))]
            {
                // Regrettably we can't use `SmolStrBuilder` because the chunks we read may not
                // all be valid utf8 on their own, and that api isn't avoiding an extra copy yet
                // anyway. And there are no nice ways to create that `Arc<[u8]>` until 1.82, and no
                // safe apis for turning it into a validated `Arc<str>` in any version. So in this
                // condition we just write it into a temporary `Vec`, copying the data twice.
                let mut temp_vec = alloc::vec::Vec::with_capacity(string_len);
                temp_vec.put(string_data.take_all());
                let allocated_string_data = from_utf8(&temp_vec).map_err(|_| InvalidValue)?;
                smol_str::SmolStr::new(allocated_string_data)
            }
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
    use super::*;
    use crate::encoding::test::check_type_test;
    use crate::encoding::General;
    use alloc::string::String;

    check_type_test!(General, relaxed, from String,
                     into smol_str::SmolStr, WireType::LengthDelimited);
    check_type_test!(General, distinguished, from String, into smol_str::SmolStr,
                     WireType::LengthDelimited);

    /// Test case covering the path that decodes noncontinguous
    #[test]
    fn decode_noncontiguous() {
        for (string_data, error) in [
            ("abcxyz".as_bytes(), None),
            (
                "significantly longer string that will have to go off stack".as_bytes(),
                None,
            ),
            ("\u{1f9d0}unicode, inline".as_bytes(), None),
            (
                "\u{1f9d0}unicode, too long to be inline, once again goes off stack".as_bytes(),
                None,
            ),
            ([0xff; 10].as_slice(), Some(InvalidValue)), // inline invalid
            ([0xff; 50].as_slice(), Some(InvalidValue)), // out-of-line invalid
        ] {
            // Put the string data into a non-contiguous buf
            let (pre, post) = string_data.split_at(3);
            let mut buf = pre.chain(post);
            prepend_varint(buf.len() as u64, &mut buf);
            assert!(buf.contiguous().is_none());
            let mut val = Default::default();
            let decode_result = <() as ValueDecoder<General, smol_str::SmolStr>>::decode_value(
                &mut val,
                Capped::new(&mut buf),
                Default::default(),
            );
            if let Some(error) = error {
                assert_eq!(decode_result.err().map(|err| err.kind()), Some(error));
            } else {
                assert!(decode_result.is_ok());
                assert_eq!(Ok(val.as_str()), from_utf8(string_data));
            }
            // The entire buffer should have been read regardless
            assert!(buf.is_empty());
        }
    }
}
