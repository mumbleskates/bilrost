use alloc::borrow::{Cow, ToOwned};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::mem;
use core::ops::Index;

use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::{
    encode_varint, encoded_len_varint, prepend_varint, Capped, DecodeContext, EmptyState,
    ForOverwrite, RawDistinguishedMessageDecoder, RawMessage, RawMessageDecoder,
    RestrictedDecodeContext, RuntimeTagMeasurer, TagMeasurer, TagRevWriter, TagWriter, WireType,
};
use crate::iter::FlatAdapter;
use crate::DecodeErrorKind::Truncated;
use crate::{Canonicity, DecodeError, Message};

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

    fn prepend_value<B: ReverseBuf + ?Sized>(&self, buf: &mut B) {
        match self {
            Varint(val) => {
                prepend_varint(*val, buf);
            }
            LengthDelimited(val) => {
                buf.prepend_slice(val.as_ref());
                prepend_varint(val.len() as u64, buf);
            }
            ThirtyTwoBit(val) => {
                buf.prepend_slice(val.as_slice());
            }
            SixtyFourBit(val) => {
                buf.prepend_slice(val.as_slice());
            }
        }
    }

    fn encode_field<B: BufMut + ?Sized>(&self, tag: u32, buf: &mut B, tw: &mut TagWriter) {
        tw.encode_key(tag, self.wire_type(), buf);
        self.encode_value(buf);
    }

    fn prepend_field<B: ReverseBuf + ?Sized>(&self, tag: u32, buf: &mut B, tw: &mut TagRevWriter) {
        tw.begin_field(tag, self.wire_type(), buf);
        self.prepend_value(buf);
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
pub struct OpaqueMessage<'a>(BTreeMap<u32, Vec<OpaqueValue<'a>>>);

impl<'a> OpaqueMessage<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn insert(&mut self, tag: u32, value: OpaqueValue<'a>) {
        self.0.entry(tag).or_default().push(value);
    }

    pub fn iter(&self) -> OpaqueIter<'a, '_> {
        FlatAdapter(self.0.iter()).flatten()
    }

    pub fn iter_mut(&mut self) -> OpaqueIterMut<'a, '_> {
        FlatAdapter(self.0.iter_mut()).flatten()
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

impl<'a> Index<&u32> for OpaqueMessage<'a> {
    type Output = [OpaqueValue<'a>];

    fn index(&self, index: &u32) -> &Self::Output {
        &self.0[index]
    }
}

pub type OpaqueIter<'a, 'b> = core::iter::Flatten<
    FlatAdapter<alloc::collections::btree_map::Iter<'b, u32, Vec<OpaqueValue<'a>>>>,
>;

pub type OpaqueIterMut<'a, 'b> = core::iter::Flatten<
    FlatAdapter<alloc::collections::btree_map::IterMut<'b, u32, Vec<OpaqueValue<'a>>>>,
>;

pub type OpaqueIntoIter<'a> = core::iter::Flatten<
    FlatAdapter<alloc::collections::btree_map::IntoIter<u32, Vec<OpaqueValue<'a>>>>,
>;

impl<'a> IntoIterator for OpaqueMessage<'a> {
    type Item = (u32, OpaqueValue<'a>);
    type IntoIter = OpaqueIntoIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        FlatAdapter(self.0.into_iter()).flatten()
    }
}

impl<'a, 'b> IntoIterator for &'b OpaqueMessage<'a> {
    type Item = (&'b u32, &'b OpaqueValue<'a>);
    type IntoIter = OpaqueIter<'a, 'b>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> FromIterator<(u32, OpaqueValue<'a>)> for OpaqueMessage<'a> {
    fn from_iter<T: IntoIterator<Item = (u32, OpaqueValue<'a>)>>(iter: T) -> Self {
        let mut res = Self::new();
        for (tag, value) in iter {
            res.insert(tag, value);
        }
        res
    }
}

impl ForOverwrite for OpaqueMessage<'_> {
    fn for_overwrite() -> Self {
        Self::new()
    }
}

impl EmptyState for OpaqueMessage<'_> {
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

    fn raw_prepend<B: ReverseBuf + ?Sized>(&self, buf: &mut B) {
        let mut tw = TagRevWriter::new();
        for (&tag, value) in self.iter().rev() {
            value.prepend_field(tag, buf, &mut tw);
        }
        tw.finalize(buf);
    }

    fn raw_encoded_len(&self) -> usize {
        let mut tm = RuntimeTagMeasurer::new();
        self.iter()
            .map(|(tag, value)| tm.key_len(*tag) + value.value_encoded_len())
            .sum()
    }
}

impl RawMessageDecoder for OpaqueMessage<'_> {
    fn raw_decode_field<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        _duplicated: bool,
        buf: Capped<B>,
        _ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        self.insert(tag, OpaqueValue::decode_value(wire_type, buf)?);
        Ok(())
    }
}

impl RawDistinguishedMessageDecoder for OpaqueMessage<'_> {
    fn raw_decode_field_distinguished<B: Buf + ?Sized>(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<B>,
        ctx: RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        self.raw_decode_field(tag, wire_type, duplicated, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

// TODO(widders): borrowed decoding for OpaqueMessage is already possible :D
