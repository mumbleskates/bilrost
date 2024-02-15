use alloc::borrow::Cow;
use alloc::vec::Vec;
use core::ops::Deref;

use bytes::{Buf, BufMut};

use crate::encoding::{
    delegate_encoding, encode_varint, encoded_len_varint, encoder_where_value_encoder, Capped,
    DecodeContext, DistinguishedEncoder, DistinguishedValueEncoder, Encoder, HasEmptyState,
    TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use crate::DecodeErrorKind::{InvalidValue, NotCanonical};

/// `PlainBytes` implements encoding for blob values directly into `Vec<u8>`, and provides the base
/// implementation for that functionality. `Vec<u8>` cannot generically dispatch to `General`'s
/// encoding, since `General` already generically implements encoding for other kinds of `Vec`, but
/// this encoder can be used instead if it's desirable to have a value whose type is exactly
/// `Vec<u8>`.
pub struct PlainBytes;

encoder_where_value_encoder!(PlainBytes);

impl Wiretyped<Vec<u8>> for PlainBytes {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Vec<u8>> for PlainBytes {
    fn encode_value<B: BufMut + ?Sized>(value: &Vec<u8>, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_slice());
    }

    fn value_encoded_len(value: &Vec<u8>) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut Vec<u8>,
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let buf = buf.take_length_delimited()?;
        value.clear();
        value.reserve(buf.remaining_before_cap());
        value.put(buf.take_all());
        Ok(())
    }
}

impl DistinguishedValueEncoder<Vec<u8>> for PlainBytes {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Vec<u8>,
        mut buf: Capped<B>,
        allow_empty: bool,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let buf = buf.take_length_delimited()?;
        if !allow_empty && !buf.has_remaining() {
            return Err(DecodeError::new(NotCanonical));
        }
        value.clear();
        value.reserve(buf.remaining_before_cap());
        value.put(buf.take_all());
        Ok(())
    }
}

delegate_encoding!(delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<Vec<u8>>) including distinguished);
delegate_encoding!(delegate from (PlainBytes) to (crate::encoding::Unpacked<PlainBytes>)
    for type (Vec<Cow<'a, [u8]>>) including distinguished with generics ('a));

#[cfg(test)]
mod vec_u8 {
    use super::{PlainBytes, Vec};
    use crate::encoding::test::check_type_test;
    check_type_test!(PlainBytes, expedient, Vec<u8>, WireType::LengthDelimited);
    check_type_test!(
        PlainBytes,
        distinguished,
        Vec<u8>,
        WireType::LengthDelimited
    );
}

impl Wiretyped<Cow<'_, [u8]>> for PlainBytes {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Cow<'_, [u8]>> for PlainBytes {
    #[inline]
    fn encode_value<B: BufMut + ?Sized>(value: &Cow<[u8]>, buf: &mut B) {
        encode_varint(value.len() as u64, buf);
        buf.put_slice(value.as_ref());
    }

    #[inline]
    fn value_encoded_len(value: &Cow<[u8]>) -> usize {
        encoded_len_varint(value.len() as u64) + value.len()
    }

    #[inline]
    fn decode_value<B: Buf + ?Sized>(
        value: &mut Cow<[u8]>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value.to_mut(), buf, ctx)
    }
}

impl DistinguishedValueEncoder<Cow<'_, [u8]>> for PlainBytes {
    #[inline]
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Cow<[u8]>,
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value_distinguished(value.to_mut(), buf, allow_empty, ctx)
    }
}

#[cfg(test)]
mod cow_bytes {
    use super::{Cow, PlainBytes};
    use crate::encoding::test::check_type_test;
    check_type_test!(PlainBytes, expedient, Cow<[u8]>, WireType::LengthDelimited);
    check_type_test!(
        PlainBytes,
        distinguished,
        Cow<[u8]>,
        WireType::LengthDelimited
    );
}

impl<const N: usize> Wiretyped<[u8; N]> for PlainBytes {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<const N: usize> HasEmptyState for [u8; N] {
    fn is_empty(&self) -> bool {
        self.iter().all(|&byte| byte == 0)
    }
}

impl<const N: usize> ValueEncoder<[u8; N]> for PlainBytes {
    fn encode_value<B: BufMut + ?Sized>(value: &[u8; N], mut buf: &mut B) {
        encode_varint(N as u64, buf);
        (&mut buf).put(value.as_slice())
    }

    fn value_encoded_len(_value: &[u8; N]) -> usize {
        encoded_len_varint(N as u64) + N
    }

    fn many_values_encoded_len<I>(values: I) -> usize
    where
        I: ExactSizeIterator,
        I::Item: Deref<Target = [u8; N]>,
    {
        values.len() * (encoded_len_varint(N as u64) + N)
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut [u8; N],
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut delimited = buf.take_length_delimited()?;
        if delimited.remaining_before_cap() != N {
            return Err(DecodeError::new(InvalidValue));
        }
        delimited.copy_to_slice(value.as_mut_slice());
        Ok(())
    }
}

impl<const N: usize> DistinguishedValueEncoder<[u8; N]> for PlainBytes {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut [u8; N],
        buf: Capped<B>,
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value, buf, ctx)?; // Distinguished value decoding is the same
        if !allow_empty && value.is_empty() {
            return Err(DecodeError::new(NotCanonical));
        } else {
            Ok(())
        }
    }
}

// TODO(widders): ArrayVec

#[cfg(test)]
mod u8_array {
    mod length_0 {
        use super::super::PlainBytes;
        use crate::encoding::test::check_type_test;
        check_type_test!(PlainBytes, expedient, [u8; 0], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 0],
            WireType::LengthDelimited
        );
    }

    mod length_1 {
        use super::super::PlainBytes;
        use crate::encoding::test::check_type_test;
        check_type_test!(PlainBytes, expedient, [u8; 1], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 1],
            WireType::LengthDelimited
        );
    }

    mod length_8 {
        use super::super::PlainBytes;
        use crate::encoding::test::check_type_test;
        check_type_test!(PlainBytes, expedient, [u8; 8], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 8],
            WireType::LengthDelimited
        );
    }

    mod length_13 {
        use super::super::PlainBytes;
        use crate::encoding::test::check_type_test;
        check_type_test!(PlainBytes, expedient, [u8; 13], WireType::LengthDelimited);
        check_type_test!(
            PlainBytes,
            distinguished,
            [u8; 13],
            WireType::LengthDelimited
        );
    }
}
