use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, TagMeasurer, TagWriter, WireType,
};
use crate::{DecodeError, Message, RawDistinguishedMessage, RawMessage};
use alloc::vec::Vec;
use bytes::{Buf, BufMut};

/// Represents an opaque bilrost field value. Can represent any valid encoded value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpaqueValue {
    Varint(u64),
    LengthDelimited(Vec<u8>),
    ThirtyTwoBit([u8; 4]),
    SixtyFourBit([u8; 8]),
}
use OpaqueValue::*;

impl OpaqueValue {
    fn varint_u64(value: u64) -> Self {
        Varint(value)
    }

    fn varint_i64(value: i64) -> Self {
        Varint(super::i64_to_unsigned(value))
    }

    fn varint_u32(value: u32) -> Self {
        Varint(value.into())
    }

    fn varint_i32(value: i32) -> Self {
        Varint(super::i32_to_unsigned(value) as u64)
    }

    fn fixed_u64(value: u64) -> Self {
        SixtyFourBit(value.to_le_bytes())
    }

    fn fixed_i64(value: i64) -> Self {
        SixtyFourBit(value.to_le_bytes())
    }

    fn fixed_u32(value: u32) -> Self {
        ThirtyTwoBit(value.to_le_bytes())
    }

    fn fixed_i32(value: i32) -> Self {
        ThirtyTwoBit(value.to_le_bytes())
    }

    fn f64(value: f64) -> Self {
        SixtyFourBit(value.to_le_bytes())
    }

    fn f32(value: f32) -> Self {
        ThirtyTwoBit(value.to_le_bytes())
    }

    fn bytes<B: Into<Vec<u8>>>(value: B) -> Self {
        LengthDelimited(value.into())
    }

    fn message<M: Message>(value: &M) -> Self {
        LengthDelimited(value.encode_to_vec())
    }
}

/// Represents a bilrost field, with its tag and value. `Vec<OpaqueField>` can encode and decode any
/// valid bilrost message as opaque values, but may panic if its fields are not in ascending tag
/// order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpaqueField(pub u32, pub OpaqueValue);

/// Sort the fields of an opaque field collection so they are in ascending tag order.
pub fn sort_opaque_fields(fields: &mut Vec<OpaqueField>) {
    fields.sort_by_key(|f| f.0)
}

impl RawMessage for Vec<OpaqueField> {
    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B) {
        let mut tw = TagWriter::new();
        for OpaqueField(tag, value) in self {
            match value {
                Varint(val) => {
                    tw.encode_key(*tag, WireType::Varint, buf);
                    encode_varint(*val, buf);
                }
                LengthDelimited(val) => {
                    tw.encode_key(*tag, WireType::LengthDelimited, buf);
                    encode_varint(val.len() as u64, buf);
                    buf.put(val);
                }
                ThirtyTwoBit(val) => {
                    tw.encode_key(*tag, WireType::ThirtyTwoBit, buf);
                    buf.put(val);
                }
                SixtyFourBit(val) => {
                    tw.encode_key(*tag, WireType::SixtyFourBit, buf);
                    buf.put(val);
                }
            }
        }
    }

    fn raw_encoded_len(&self) -> usize {
        let mut tm = TagMeasurer::new();
        let mut total = 0;
        for OpaqueField(tag, value) in self {
            match value {
                Varint(val) => {
                    tm.key_len(*tag) + encoded_len_varint(*val);
                }
                LengthDelimited(val) => {
                    tm.key_len(*tag) + encoded_len_varint(val.len() as u64) + val.len()
                }
                ThirtyTwoBit(_) => tm.key_len(*tag) + 4,
                SixtyFourBit(_) => tm.key_len(*tag) + 8,
            }
        }
        total
    }

    fn raw_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        _duplicated: bool,
        mut buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        self.push(OpaqueField(
            tag,
            match wire_type {
                WireType::Varint => Varint(buf.decode_varint()?),
                WireType::LengthDelimited => {
                    let mut val = Vec::new();
                    val.put(buf.take_length_delimited()?);
                    LengthDelimited(val)
                }
                WireType::ThirtyTwoBit => {
                    let mut val = [0u8; 4];
                    buf.copy_to_slice(&mut val)
                }
                WireType::SixtyFourBit => {
                    let mut val = [0u8; 8];
                    buf.copy_to_slice(&mut val)
                }
            },
        ));
        Ok(())
    }
}

impl RawDistinguishedMessage for Vec<OpaqueField> {
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        self.raw_decode_field(tag, wire_type, duplicated, buf, ctx)
    }
}
