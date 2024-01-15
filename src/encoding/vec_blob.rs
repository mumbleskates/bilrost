use alloc::vec::Vec;

use bytes::{Buf, BufMut};

use crate::encoding::{
    check_type_test, delegate_encoding, encode_varint, encoded_len_varint, Capped, DecodeContext,
    DistinguishedEncoder, DistinguishedFieldEncoder, DistinguishedValueEncoder, Encoder,
    FieldEncoder, TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;

/// `VecBlob` implements encoding for blob values directly into `Vec<u8>`, and provides the base
/// implementation for that functionality. `Vec<u8>` cannot generically dispatch to `General`'s
/// encoding, since `General` already generically implements encoding for other kinds of `Vec`, but
/// this encoder can be used instead if it's desirable to have a value whose type is exactly
/// `Vec<u8>`.
pub struct VecBlob;

impl Wiretyped<Vec<u8>> for VecBlob {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl ValueEncoder<Vec<u8>> for VecBlob {
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

impl DistinguishedValueEncoder<Vec<u8>> for VecBlob {
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut Vec<u8>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        Self::decode_value(value, buf, ctx)
    }
}

impl Encoder<Vec<u8>> for VecBlob {
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &Vec<u8>, buf: &mut B, tw: &mut TagWriter) {
        if !value.is_empty() {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &Vec<u8>, tm: &mut TagMeasurer) -> usize {
        if !value.is_empty() {
            Self::field_encoded_len(tag, value, tm)
        } else {
            0
        }
    }

    #[inline]
    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Vec<u8>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of non-repeated field",
            ));
        }
        Self::decode_field(wire_type, value, buf, ctx)
    }
}

impl DistinguishedEncoder<Vec<u8>> for VecBlob {
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut Vec<u8>,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of non-repeated field",
            ));
        }
        Self::decode_field_distinguished(wire_type, value, buf, ctx)?;
        if value.is_empty() {
            return Err(DecodeError::new(
                "plain field was encoded with its zero value",
            ));
        }
        Ok(())
    }
}

delegate_encoding!(delegate from (VecBlob) to (crate::encoding::Unpacked<VecBlob>)
    for type (Vec<Vec<u8>>) including distinguished);

check_type_test!(VecBlob, expedient, Vec<u8>, WireType::LengthDelimited);
check_type_test!(VecBlob, distinguished, Vec<u8>, WireType::LengthDelimited);
