use crate::encoding::value_traits::{Collection, DistinguishedCollection};
use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, DistinguishedEncoder,
    DistinguishedFieldEncoder, DistinguishedValueEncoder, Encoder, FieldEncoder, General,
    NewForOverwrite, TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;

use bytes::{Buf, BufMut};

pub struct Packed<E = General>(E);

/// Packed encodings are always length delimited.
impl<T, E> Wiretyped<T> for Packed<E> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<C, T, E> ValueEncoder<C> for Packed<E>
where
    C: Collection<Item = T>,
    E: ValueEncoder<T>,
    T: NewForOverwrite,
{
    fn encode_value<B: BufMut + ?Sized>(value: &C, buf: &mut B) {
        encode_varint(E::many_values_encoded_len(value.iter()) as u64, buf);
        for val in value.iter() {
            E::encode_value(val, buf);
        }
    }

    fn value_encoded_len(value: &C) -> usize {
        let inner_len = E::many_values_encoded_len(value.iter());
        // TODO(widders): address general cases where u64 may overflow usize, with care
        encoded_len_varint(inner_len as u64) + inner_len
    }

    fn decode_value<B: Buf + ?Sized>(
        value: &mut C,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if E::WIRE_TYPE.fixed_size().map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        for val in capped.consume(|buf| {
            let mut new_val = T::new_for_overwrite();
            E::decode_value(&mut new_val, buf.lend(), ctx.clone())?;
            Ok(new_val)
        }) {
            value.insert(val?).map_err(DecodeError::new)?;
        }
        Ok(())
    }
}

impl<C, T, E> DistinguishedValueEncoder<C> for Packed<E>
where
    C: DistinguishedCollection<Item = T> + Eq,
    E: DistinguishedValueEncoder<T>,
    T: NewForOverwrite + Eq,
{
    fn decode_value_distinguished<B: Buf + ?Sized>(
        value: &mut C,
        mut buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if E::WIRE_TYPE.fixed_size().map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        for val in capped.consume(|buf| {
            let mut new_val = T::new_for_overwrite();
            E::decode_value_distinguished(&mut new_val, buf.lend(), ctx.clone())?;
            Ok(new_val)
        }) {
            value.insert(val?).map_err(DecodeError::new)?;
        }
        Ok(())
    }
}

/// ValueEncoder for packed repeated encodings lets this value type nest.
impl<C, T, E> Encoder<C> for Packed<E>
where
    C: Collection<Item = T>,
    Self: ValueEncoder<C>,
    E: ValueEncoder<T>,
    T: NewForOverwrite,
{
    #[inline]
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
        if !value.is_empty() {
            Self::encode_field(tag, value, buf, tw);
        }
    }

    #[inline]
    fn encoded_len(tag: u32, value: &C, tm: &mut TagMeasurer) -> usize {
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
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            if duplicated {
                return Err(DecodeError::new(
                    "multiple occurrences of packed repeated field",
                ));
            }
            Self::decode_value(value, buf, ctx)
        } else {
            // Otherwise, try decoding it in the
            // TODO(widders): we would take more fields greedily here
            let mut new_val = T::new_for_overwrite();
            E::decode_field(wire_type, &mut new_val, buf, ctx)?;
            value.insert(new_val).map_err(DecodeError::new)?;
            Ok(())
        }
    }
}

impl<C, E> DistinguishedEncoder<C> for Packed<E>
where
    C: DistinguishedCollection,
    Self: DistinguishedValueEncoder<C> + Encoder<C>,
{
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of packed repeated field",
            ));
        }
        Self::decode_field_distinguished(wire_type, value, buf, ctx)?;
        if value.is_empty() {
            return Err(DecodeError::new("packed field encoded with no values"));
        }
        Ok(())
    }
}
