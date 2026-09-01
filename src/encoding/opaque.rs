use crate::buf::ReverseBuf;
use crate::encoding::schema::{PopulateSchema, RegisterMessage, Schema};
use crate::encoding::{
    encode_varint, encoded_len_varint, prepend_varint, Capped, DecodeContext,
    RawDistinguishedMessageBorrowDecoder, RawDistinguishedMessageDecoder, RawMessage,
    RawMessageBorrowDecoder, RawMessageDecoder, RestrictedDecodeContext, RuntimeTagMeasurer,
    TagMeasurer, TagRevWriter, TagWriter, WireType,
};
use crate::iter::FlatAdapter;
use crate::DecodeErrorKind::Truncated;
use crate::{Canonicity, DecodeError, Message};
use alloc::borrow::{Cow, ToOwned};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use bytes::{Buf, BufMut};
use core::ops::Index;

/// Represents an opaque bilrost field value. Can represent any valid encoded value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum OpaqueValue<'a> {
    Varint(u64),
    LengthDelimited(Cow<'a, [u8]>),
    ThirtyTwoBit([u8; 4]),
    SixtyFourBit([u8; 8]),
}

use OpaqueValue::*;

impl OpaqueValue<'_> {
    pub fn u64(value: u64) -> OpaqueValue<'static> {
        Varint(value)
    }

    pub fn i64(value: i64) -> OpaqueValue<'static> {
        Varint(super::varint::i64_to_unsigned(value))
    }

    pub fn u32(value: u32) -> OpaqueValue<'static> {
        Varint(value.into())
    }

    pub fn i32(value: i32) -> OpaqueValue<'static> {
        Varint(super::varint::i64_to_unsigned(value as i64))
    }

    pub fn u16(value: u16) -> OpaqueValue<'static> {
        Varint(value.into())
    }

    pub fn i16(value: i16) -> OpaqueValue<'static> {
        Varint(super::varint::i64_to_unsigned(value as i64))
    }

    pub fn u8(value: u8) -> OpaqueValue<'static> {
        Varint(value.into())
    }

    pub fn i8(value: i8) -> OpaqueValue<'static> {
        Varint(super::varint::i64_to_unsigned(value as i64))
    }

    pub fn bool(value: bool) -> OpaqueValue<'static> {
        Varint(if value { 1 } else { 0 })
    }

    pub fn fixed_u64(value: u64) -> OpaqueValue<'static> {
        SixtyFourBit(value.to_le_bytes())
    }

    pub fn fixed_i64(value: i64) -> OpaqueValue<'static> {
        SixtyFourBit(value.to_le_bytes())
    }

    pub fn fixed_u32(value: u32) -> OpaqueValue<'static> {
        ThirtyTwoBit(value.to_le_bytes())
    }

    pub fn fixed_i32(value: i32) -> OpaqueValue<'static> {
        ThirtyTwoBit(value.to_le_bytes())
    }

    pub fn f64(value: f64) -> OpaqueValue<'static> {
        SixtyFourBit(value.to_le_bytes())
    }

    pub fn f32(value: f32) -> OpaqueValue<'static> {
        ThirtyTwoBit(value.to_le_bytes())
    }

    pub fn str(value: &str) -> OpaqueValue<'_> {
        LengthDelimited(Cow::Borrowed(value.as_bytes()))
    }

    pub fn string<S: Into<String>>(value: S) -> OpaqueValue<'static> {
        LengthDelimited(Cow::Owned(value.into().into_bytes()))
    }

    pub fn byte_slice(value: &[u8]) -> OpaqueValue<'_> {
        LengthDelimited(Cow::Borrowed(value))
    }

    pub fn bytes<B: Into<Vec<u8>>>(value: B) -> OpaqueValue<'static> {
        LengthDelimited(Cow::Owned(value.into()))
    }

    pub fn message<M: Message>(value: &M) -> OpaqueValue<'static> {
        LengthDelimited(Cow::Owned(value.encode_to_vec()))
    }

    pub fn packed<'b, T: IntoIterator<Item = OpaqueValue<'b>>>(items: T) -> OpaqueValue<'static> {
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
    ) -> Result<OpaqueValue<'static>, DecodeError> {
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

    fn borrow_decode_value<'a>(
        wire_type: WireType,
        mut buf: Capped<&'a [u8]>,
    ) -> Result<OpaqueValue<'a>, DecodeError> {
        Ok(match wire_type {
            WireType::Varint => Varint(buf.decode_varint()?),
            WireType::LengthDelimited => {
                LengthDelimited(Cow::Borrowed(buf.take_borrowed_length_delimited()?))
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
    pub fn borrow(&self) -> OpaqueValue<'_> {
        match self {
            Varint(value) => Varint(*value),
            LengthDelimited(value) => LengthDelimited(Cow::Borrowed(value.as_ref())),
            ThirtyTwoBit(value) => ThirtyTwoBit(*value),
            SixtyFourBit(value) => SixtyFourBit(*value),
        }
    }

    /// Converts this value to a fully owned deep copy.
    pub fn into_owned(self) -> OpaqueValue<'static> {
        match self {
            Varint(value) => Varint(value),
            LengthDelimited(Cow::Owned(value)) => LengthDelimited(Cow::Owned(value)),
            LengthDelimited(Cow::Borrowed(value)) => LengthDelimited(Cow::Owned(value.to_owned())),
            ThirtyTwoBit(value) => ThirtyTwoBit(value),
            SixtyFourBit(value) => SixtyFourBit(value),
        }
    }
}

/// Represents a decoded Bilrost message. `OpaqueMessage` can encode and decode *any* potentially
/// valid Bilrost message, and will re-encode the exact same bytes. The type is fully bijective to
/// the set of potentially valid encoded Bilrost messages.
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

    /// Produces a full copy of the message with all borrowable data (re-)borrowed.
    pub fn to_borrowed(&self) -> OpaqueMessage<'_> {
        self.iter().map(|(k, v)| (*k, v.borrow())).collect()
    }

    #[cfg(not(feature = "forbid-unsafe"))]
    /// Converts this message to a fully owned deep copy.
    pub fn into_owned(mut self) -> OpaqueMessage<'static> {
        for (_, value) in self.iter_mut() {
            if let LengthDelimited(delimited) = value {
                delimited.to_mut();
            }
        }
        // SAFETY: we've converted every `Cow` in the structure to `Owned` in-place; no values that
        // have the lifetime we are transmuting can still exist
        unsafe { core::mem::transmute(self) }
    }
    #[cfg(feature = "forbid-unsafe")]
    /// Converts this message to a fully owned deep copy.
    pub fn into_owned(self) -> OpaqueMessage<'static> {
        // In the safe version we into-iterate, convert to owned, and collect both the outer
        // BTreeMap and each inner Vec of values rather than doing the conversion in-place.
        // This means the BTreeMap itself probably gets re-built since it probably doesn't have
        // the same in-place specializations Vec does.
        OpaqueMessage::<'static>(
            self.0
                .into_iter()
                .map(|(tag, values)| {
                    (
                        tag,
                        values
                            .into_iter()
                            .map(|value| match value {
                                LengthDelimited(delimited) => {
                                    LengthDelimited(delimited.into_owned().into())
                                }
                                Varint(val) => Varint(val),
                                ThirtyTwoBit(val) => ThirtyTwoBit(val),
                                SixtyFourBit(val) => SixtyFourBit(val),
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
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

impl RegisterMessage for OpaqueMessage<'static> {
    fn register(schema: &Schema) {
        schema.register_message::<Self>("OpaqueMessage", |_| {});
    }
}

impl RawMessage for OpaqueMessage<'_> {
    const __ASSERTIONS: () = ();

    fn empty() -> Self {
        OpaqueMessage::new()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline]
    fn clear(&mut self) {
        self.0.clear()
    }

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
        _ctx: impl DecodeContext,
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
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        self.raw_decode_field(tag, wire_type, duplicated, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}

impl<'a> RawMessageBorrowDecoder<'a> for OpaqueMessage<'a> {
    fn raw_borrow_decode_field(
        &mut self,
        tag: u32,
        wire_type: WireType,
        _duplicated: bool,
        buf: Capped<&'a [u8]>,
        _ctx: impl DecodeContext,
    ) -> Result<(), DecodeError> {
        self.insert(tag, OpaqueValue::borrow_decode_value(wire_type, buf)?);
        Ok(())
    }
}

impl<'a> RawDistinguishedMessageBorrowDecoder<'a> for OpaqueMessage<'a> {
    fn raw_borrow_decode_field_distinguished(
        &mut self,
        tag: u32,
        wire_type: WireType,
        duplicated: bool,
        buf: Capped<&'a [u8]>,
        ctx: impl RestrictedDecodeContext,
    ) -> Result<Canonicity, DecodeError>
    where
        Self: Sized,
    {
        self.raw_borrow_decode_field(tag, wire_type, duplicated, buf, ctx.into_inner())?;
        Ok(Canonicity::Canonical)
    }
}
