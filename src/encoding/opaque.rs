use alloc::borrow::{Cow, ToOwned};
use alloc::string::String;
use alloc::vec::Vec;
use core::mem;
use core::ops::{Deref, DerefMut};

use btreemultimap::BTreeMultiMap;
use bytes::{Buf, BufMut};

use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, EmptyState, TagMeasurer, TagWriter,
    WireType,
};
use crate::DecodeErrorKind::Truncated;
use crate::{Canonicity, DecodeError, Message, RawDistinguishedMessage, RawMessage};

/// Represents an opaque bilrost field value. Can represent any valid encoded value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpaqueValue<'a> {
    Varint(u64),
    LengthDelimited(Cow<'a, [u8]>),
    ThirtyTwoBit([u8; 4]),
    SixtyFourBit([u8; 8]),
}

use OpaqueValue::*;

impl<'a> OpaqueValue<'a> {
    pub fn u64(value: u64) -> Self {
        Varint(value)
    }

    pub fn i64(value: i64) -> Self {
        Varint(super::varint::i64_to_unsigned(value))
    }

    pub fn u32(value: u32) -> Self {
        Varint(value.into())
    }

    pub fn i32(value: i32) -> Self {
        Varint(super::varint::i64_to_unsigned(value as i64))
    }

    pub fn u16(value: u16) -> Self {
        Varint(value.into())
    }

    pub fn i16(value: i16) -> Self {
        Varint(super::varint::i64_to_unsigned(value as i64))
    }

    pub fn u8(value: u8) -> Self {
        Varint(value.into())
    }

    pub fn i8(value: i8) -> Self {
        Varint(super::varint::i64_to_unsigned(value as i64))
    }

    pub fn bool(value: bool) -> Self {
        Varint(if value { 1 } else { 0 })
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

    pub fn str(value: &'a str) -> Self {
        LengthDelimited(Cow::Borrowed(value.as_bytes()))
    }

    pub fn string<S: Into<String>>(value: S) -> Self {
        LengthDelimited(Cow::Owned(value.into().into_bytes()))
    }

    pub fn byte_slice(value: &'a [u8]) -> Self {
        LengthDelimited(Cow::Borrowed(value))
    }

    pub fn bytes<B: Into<Vec<u8>>>(value: B) -> Self {
        LengthDelimited(Cow::Owned(value.into()))
    }

    pub fn message<M: Message>(value: &M) -> Self {
        LengthDelimited(Cow::Owned(value.encode_to_vec()))
    }

    pub fn packed<'b, T: IntoIterator<Item = OpaqueValue<'b>>>(items: T) -> Self {
        let mut value = Vec::new();
        for item in items {
            item.encode_value(&mut value);
        }
        LengthDelimited(Cow::Owned(value))
    }

    fn wire_type(&self) -> WireType {
        match self {
            Varint(_) => WireType::Varint,
            LengthDelimited(_) => WireType::LengthDelimited,
            ThirtyTwoBit(_) => WireType::ThirtyTwoBit,
            SixtyFourBit(_) => WireType::SixtyFourBit,
        }
    }

    fn encode_value<B: BufMut + ?Sized>(&self, mut buf: &mut B) {
        match self {
            Varint(val) => {
                encode_varint(*val, buf);
            }
            LengthDelimited(val) => {
                encode_varint(val.len() as u64, buf);
                (&mut buf).put(val.as_ref());
            }
            ThirtyTwoBit(val) => {
                (&mut buf).put(val.as_slice());
            }
            SixtyFourBit(val) => {
                (&mut buf).put(val.as_slice());
            }
        }
    }

    fn encode_field<B: BufMut + ?Sized>(&self, tag: u32, buf: &mut B, tw: &mut TagWriter) {
        tw.encode_key(tag, self.wire_type(), buf);
        self.encode_value(buf);
    }

    fn value_encoded_len(&self) -> usize {
        match self {
            Varint(val) => encoded_len_varint(*val),
            LengthDelimited(val) => encoded_len_varint(val.len() as u64) + val.len(),
            ThirtyTwoBit(_) => 4,
            SixtyFourBit(_) => 8,
        }
    }

    fn decode_value<B: Buf + ?Sized>(
        wire_type: WireType,
        mut buf: Capped<B>,
    ) -> Result<Self, DecodeError> {
        Ok(match wire_type {
            WireType::Varint => Varint(buf.decode_varint()?),
            WireType::LengthDelimited => {
                let mut val = Vec::new();
                val.put(buf.take_length_delimited()?.take_all());
                LengthDelimited(Cow::Owned(val))
            }
            WireType::ThirtyTwoBit => {
                if buf.remaining_before_cap() < 4 {
                    return Err(DecodeError::new(Truncated));
                }
                let mut val = [0u8; 4];
                buf.copy_to_slice(&mut val);
                ThirtyTwoBit(val)
            }
            WireType::SixtyFourBit => {
                if buf.remaining_before_cap() < 8 {
                    return Err(DecodeError::new(Truncated));
                }
                let mut val = [0u8; 8];
                buf.copy_to_slice(&mut val);
                SixtyFourBit(val)
            }
        })
    }

    /// Get a copy of this value with borrowed or re-borrowed data.
    pub fn borrow(&self) -> OpaqueValue {
        match self {
            Varint(value) => Varint(*value),
            LengthDelimited(value) => LengthDelimited(Cow::Borrowed(value.as_ref())),
            ThirtyTwoBit(value) => ThirtyTwoBit(*value),
            SixtyFourBit(value) => SixtyFourBit(*value),
        }
    }

    /// Converts this value to a fully owned deep copy.
    pub fn convert_to_owned(self) -> OpaqueValue<'static> {
        match self {
            Varint(value) => Varint(value),
            LengthDelimited(Cow::Owned(value)) => LengthDelimited(Cow::Owned(value)),
            LengthDelimited(Cow::Borrowed(value)) => LengthDelimited(Cow::Owned(value.to_owned())),
            ThirtyTwoBit(value) => ThirtyTwoBit(value),
            SixtyFourBit(value) => SixtyFourBit(value),
        }
    }
}

/// Represents a bilrost field, with its tag and value. `OpaqueMessage` can encode and decode *any*
/// potentially valid bilrost message as opaque values, and will re-encode the exact same bytes.
/// Likewise, any state representable by `OpaqueMessage` encodes a potentially valid bilrost
/// message.
///
/// At present this is still an unstable API, mostly used for internals and testing. Trait
/// implementations and APIs of `OpaqueMessage` and `OpaqueValue` are subject to change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpaqueMessage<'a>(BTreeMultiMap<u32, OpaqueValue<'a>>);

impl OpaqueMessage<'_> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Produces a full copy of the message with all data (re-)borrowed.
    pub fn borrowed(&self) -> OpaqueMessage {
        self.iter().map(|(k, v)| (*k, v.borrow())).collect()
    }

    /// Converts this message to a fully owned deep copy.
    pub fn convert_to_owned(mut self) -> OpaqueMessage<'static> {
        for (_, value) in self.iter_mut() {
            let LengthDelimited(Cow::Borrowed(borrowed)) = value else {
                continue;
            };
            let owned_value = borrowed.to_owned();
            *value = LengthDelimited(Cow::Owned(owned_value));
        }
        // SAFETY: we've converted every `Cow` in the structure to `Owned` in-place
        unsafe { mem::transmute(self) }
    }
}

impl<'a> Deref for OpaqueMessage<'a> {
    type Target = BTreeMultiMap<u32, OpaqueValue<'a>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> DerefMut for OpaqueMessage<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub struct OpaqueIterator<'a> {
    iter: <BTreeMultiMap<u32, OpaqueValue<'a>> as IntoIterator>::IntoIter,
    current: Option<(u32, <Vec<OpaqueValue<'a>> as IntoIterator>::IntoIter)>,
}

impl<'a> Iterator for OpaqueIterator<'a> {
    type Item = (u32, OpaqueValue<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let (tag, value_iter) = match self.current.as_mut() {
            None => self.current.insert(
                self.iter
                    .next()
                    .map(|(tag, values)| (tag, values.into_iter()))?,
            ),
            Some(x) => x,
        };
        match value_iter.next() {
            None => {
                let (new_tag, new_values) = self.current.insert(
                    self.iter
                        .next()
                        .map(|(tag, values)| (tag, values.into_iter()))?,
                );
                Some((*new_tag, new_values.next()?))
            }
            Some(value) => Some((*tag, value)),
        }
    }
}

impl<'a> IntoIterator for OpaqueMessage<'a> {
    type Item = <OpaqueIterator<'a> as Iterator>::Item;
    type IntoIter = OpaqueIterator<'a>;
    fn into_iter(self) -> Self::IntoIter {
        OpaqueIterator {
            iter: self.0.into_iter(),
            current: None,
        }
    }
}

impl<'a, 'b> IntoIterator for &'b OpaqueMessage<'a> {
    type Item = <&'b BTreeMultiMap<u32, OpaqueValue<'a>> as IntoIterator>::Item;
    type IntoIter = <&'b BTreeMultiMap<u32, OpaqueValue<'a>> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> FromIterator<(u32, OpaqueValue<'a>)> for OpaqueMessage<'a> {
    fn from_iter<T: IntoIterator<Item = (u32, OpaqueValue<'a>)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl EmptyState for OpaqueMessage<'_> {
    #[inline]
    fn empty() -> Self {
        Self::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    fn clear(&mut self) {
        self.0.clear()
    }
}

impl RawMessage for OpaqueMessage<'_> {
    const __ASSERTIONS: () = ();

    fn raw_encode<B: BufMut + ?Sized>(&self, buf: &mut B) {
        let mut tw = TagWriter::new();
        for (tag, value) in self {
            value.encode_field(*tag, buf, &mut tw);
        }
    }

    fn raw_encoded_len(&self) -> usize {
        let mut tm = TagMeasurer::new();
        self.iter()
            .map(|(tag, value)| tm.key_len(*tag) + value.value_encoded_len())
            .sum()
    }

    fn raw_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        _duplicated: bool,
        buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError>
    where
        Self: Sized,
    {
        self.insert(tag, OpaqueValue::decode_value(wire_type, buf)?);
        Ok(())
    }
}

impl RawDistinguishedMessage for OpaqueMessage<'_> {
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        self.raw_decode_field(tag, wire_type, duplicated, buf, ctx)?;
        Ok(Canonicity::Canonical)
    }
}
