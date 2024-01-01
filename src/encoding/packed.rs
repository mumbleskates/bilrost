use crate::bytes::{Buf, BufMut};
use crate::encoding::{
    encode_varint, encoded_len_varint, Capped, DecodeContext, DistinguishedEncoder,
    DistinguishedFieldEncoder, DistinguishedValueEncoder, Encoder, FieldEncoder, General,
    NewForOverwrite, TagMeasurer, TagWriter, ValueEncoder, Veclike, WireType, Wiretyped,
};
use crate::helpers::FallibleIter;
use crate::DecodeError;

pub struct Packed<E = General>(E);

/// Packed encodings are always length delimited.
impl<T, E> Wiretyped<T> for Packed<E> {
    const WIRE_TYPE: WireType = WireType::LengthDelimited;
}

impl<C, T, E> ValueEncoder<C> for Packed<E>
where
    C: Veclike<Item = T>,
    E: ValueEncoder<T>,
    T: NewForOverwrite,
{
    fn encode_value<B: BufMut>(value: &C, buf: &mut B) {
        encode_varint(E::many_values_encoded_len(value) as u64, buf);
        for val in value.iter() {
            E::encode_value(val, buf);
        }
    }

    fn value_encoded_len(value: &C) -> usize {
        let inner_len = E::many_values_encoded_len(value);
        encoded_len_varint(inner_len as u64) + inner_len
    }

    fn decode_value<B: Buf>(
        value: &mut C,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if capped.remaining_before_cap() % E::WIRE_TYPE.encoded_size_alignment() != 0 {
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        let mut consumer = FallibleIter::new(capped.consume(|buf| {
            let mut new_val = T::new_for_overwrite();
            E::decode_value(&mut new_val, buf, ctx.clone())?;
            Ok(new_val)
        }));
        value.extend(&mut consumer);
        consumer.check()
    }
}

impl<C, T, E> DistinguishedValueEncoder<C> for Packed<E>
where
    C: Veclike<Item = T> + Eq,
    E: DistinguishedValueEncoder<T>,
    T: NewForOverwrite + Eq,
{
    fn decode_value_distinguished<B: Buf>(
        value: &mut C,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let capped = buf.take_length_delimited()?;
        if capped.remaining_before_cap() % E::WIRE_TYPE.encoded_size_alignment() != 0 {
            return Err(DecodeError::new("packed field is not a valid length"));
        }
        let mut consumer = FallibleIter::new(capped.consume(|buf| {
            let mut new_val = T::new_for_overwrite();
            E::decode_value_distinguished(&mut new_val, buf, ctx.clone())?;
            Ok(new_val)
        }));
        value.extend(&mut consumer);
        consumer.check()
    }
}

/// ValueEncoder for packed repeated encodings lets this value type nest.
impl<C, E> Encoder<C> for Packed<E>
where
    C: Veclike,
    Packed<E>: ValueEncoder<C>,
{
    #[inline]
    fn encode<B: BufMut>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
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
    fn decode<B: Buf>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: &mut Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if duplicated {
            return Err(DecodeError::new(
                "multiple occurrences of packed repeated field",
            ));
        }
        Self::decode_field(wire_type, value, buf, ctx)
    }
}

impl<C, E> DistinguishedEncoder<C> for Packed<E>
where
    C: Veclike + Eq,
    Packed<E>: DistinguishedValueEncoder<C> + Encoder<C>,
{
    #[inline]
    fn decode_distinguished<B: Buf>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: &mut Capped<B>,
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
