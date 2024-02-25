use bytes::{Buf, BufMut};

use crate::encoding::value_traits::{Collection, DistinguishedCollection};
use crate::encoding::{
    encode_varint, encoded_len_varint, unpacked, Canonicity, Capped, DecodeContext, DecodeError,
    DistinguishedEncoder, DistinguishedValueEncoder, Encoder, FieldEncoder, General,
    NewForOverwrite, TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeErrorKind::{Truncated, UnexpectedlyRepeated};

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
        let mut capped = buf.take_length_delimited()?;
        if E::WIRE_TYPE.fixed_size().map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new(Truncated));
        }
        while capped.has_remaining()? {
            let mut new_val = T::new_for_overwrite();
            E::decode_value(&mut new_val, capped.lend(), ctx.clone())?;
            value.insert(new_val)?;
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
        allow_empty: bool,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        let mut capped = buf.take_length_delimited()?;
        if !allow_empty && capped.remaining_before_cap() == 0 {
            return Ok(Canonicity::NotCanonical);
        }
        if E::WIRE_TYPE.fixed_size().map_or(false, |fixed_size| {
            capped.remaining_before_cap() % fixed_size != 0
        }) {
            return Err(DecodeError::new(Truncated));
        }
        let mut canon = Canonicity::Canonical;
        while capped.has_remaining()? {
            let mut new_val = T::new_for_overwrite();
            canon.update(E::decode_value_distinguished(
                &mut new_val,
                capped.lend(),
                true,
                ctx.clone(),
            )?);
            canon.update(value.insert_distinguished(new_val)?);
        }
        Ok(canon)
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
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            Self::decode_value(value, buf, ctx)
        } else {
            // Otherwise, try decoding it in the unpacked representation
            unpacked::decode::<C, E>(wire_type, value, buf, ctx)
        }
    }
}

impl<C, T, E> DistinguishedEncoder<C> for Packed<E>
where
    C: DistinguishedCollection<Item = T>,
    T: NewForOverwrite + Eq,
    E: DistinguishedValueEncoder<T> + ValueEncoder<T>,
    Self: DistinguishedValueEncoder<C> + Encoder<C>,
{
    #[inline]
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited {
            // We've encountered the expected length-delimited type: decode it in packed format.
            // Set allow_empty=false: empty collections are not canonical
            Self::decode_value_distinguished(value, buf, false, ctx)
        } else {
            // Otherwise, try decoding it in the unpacked representation
            unpacked::decode::<C, E>(wire_type, value, buf, ctx)?;
            Ok(Canonicity::NotCanonical)
        }
    }
}
