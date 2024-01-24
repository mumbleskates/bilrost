use alloc::vec::Vec;
use core::borrow::{Borrow, BorrowMut};
use core::ops::{Deref, DerefMut};

use bytes::{Buf, BufMut};

use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, TagMeasurer, TagWriter, WireType,
};
use crate::{DecodeError, Message, RawDistinguishedMessage, RawMessage};

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
    pub fn varint_u64(value: u64) -> Self {
        Varint(value)
    }

    pub fn varint_i64(value: i64) -> Self {
        Varint(super::i64_to_unsigned(value))
    }

    pub fn varint_u32(value: u32) -> Self {
        Varint(value.into())
    }

    pub fn varint_i32(value: i32) -> Self {
        Varint(super::i32_to_unsigned(value) as u64)
    }

    pub fn fixed_u64(value: u64) -> Self {
        SixtyFourBit(value.to_le_bytes())
    }

    pub fn fixed_i64(value: i64) -> Self {
        SixtyFourBit(value.to_le_bytes())
    }

    pub fn fixed_u32(value: u32) -> Self {
        ThirtyTwoBit(value.to_le_bytes())
    }

    pub fn fixed_i32(value: i32) -> Self {
        ThirtyTwoBit(value.to_le_bytes())
    }

    pub fn f64(value: f64) -> Self {
        SixtyFourBit(value.to_le_bytes())
    }

    pub fn f32(value: f32) -> Self {
        ThirtyTwoBit(value.to_le_bytes())
    }

    pub fn bytes<B: Into<Vec<u8>>>(value: B) -> Self {
        LengthDelimited(value.into())
    }

    pub fn message<M: Message>(value: &M) -> Self {
        LengthDelimited(value.encode_to_vec())
    }
}

/// Represents a bilrost field, with its tag and value. `Vec<OpaqueField>` can encode and decode any
/// valid bilrost message as opaque values, but may panic if its fields are not in ascending tag
/// order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OpaqueMessage(pub Vec<(u32, OpaqueValue)>);

impl Deref for OpaqueMessage {
    type Target = Vec<(u32, OpaqueValue)>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OpaqueMessage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl AsRef<Vec<(u32, OpaqueValue)>> for OpaqueMessage {
    fn as_ref(&self) -> &Vec<(u32, OpaqueValue)> {
        &self.0
    }
}

impl AsMut<Vec<(u32, OpaqueValue)>> for OpaqueMessage {
    fn as_mut(&mut self) -> &mut Vec<(u32, OpaqueValue)> {
        &mut self.0
    }
}

impl Borrow<Vec<(u32, OpaqueValue)>> for OpaqueMessage {
    fn borrow(&self) -> &Vec<(u32, OpaqueValue)> {
        &self.0
    }
}

impl BorrowMut<Vec<(u32, OpaqueValue)>> for OpaqueMessage {
    fn borrow_mut(&mut self) -> &mut Vec<(u32, OpaqueValue)> {
        &mut self.0
    }
}

impl FromIterator<(u32, OpaqueValue)> for OpaqueMessage {
    fn from_iter<T: IntoIterator<Item = (u32, OpaqueValue)>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}

impl IntoIterator for OpaqueMessage {
    type Item = (u32, OpaqueValue);
    type IntoIter = alloc::vec::IntoIter<(u32, OpaqueValue)>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a OpaqueMessage {
    type Item = &'a (u32, OpaqueValue);
    type IntoIter = core::slice::Iter<'a, (u32, OpaqueValue)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl OpaqueMessage {
    /// Sort the fields of the message so they are in ascending tag order and won't panic when
    /// encoding.
    pub fn sort_fields(&mut self) {
        self.sort_by_key(|(tag, _)| *tag);
    }
}

impl RawMessage for OpaqueMessage {
    fn raw_encode<B: BufMut + ?Sized>(&self, mut buf: &mut B) {
        let mut tw = TagWriter::new();
        for (tag, value) in self {
            match value {
                Varint(val) => {
                    tw.encode_key(*tag, WireType::Varint, buf);
                    encode_varint(*val, buf);
                }
                LengthDelimited(val) => {
                    tw.encode_key(*tag, WireType::LengthDelimited, buf);
                    encode_varint(val.len() as u64, buf);
                    (&mut buf).put(val.as_slice());
                }
                ThirtyTwoBit(val) => {
                    tw.encode_key(*tag, WireType::ThirtyTwoBit, buf);
                    (&mut buf).put(val.as_slice());
                }
                SixtyFourBit(val) => {
                    tw.encode_key(*tag, WireType::SixtyFourBit, buf);
                    (&mut buf).put(val.as_slice());
                }
            }
        }
    }

    fn raw_encoded_len(&self) -> usize {
        let mut tm = TagMeasurer::new();
        let mut total = 0;
        for (tag, value) in self {
            total += match value {
                Varint(val) => tm.key_len(*tag) + encoded_len_varint(*val),
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
        self.push((
            tag,
            match wire_type {
                WireType::Varint => Varint(buf.decode_varint()?),
                WireType::LengthDelimited => {
                    let mut val = Vec::new();
                    val.put(buf.take_length_delimited()?.take_all());
                    LengthDelimited(val)
                }
                WireType::ThirtyTwoBit => {
                    let mut val = [0u8; 4];
                    buf.copy_to_slice(&mut val);
                    ThirtyTwoBit(val)
                }
                WireType::SixtyFourBit => {
                    let mut val = [0u8; 8];
                    buf.copy_to_slice(&mut val);
                    SixtyFourBit(val)
                }
            },
        ));
        Ok(())
    }
}

impl RawDistinguishedMessage for OpaqueMessage {
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
