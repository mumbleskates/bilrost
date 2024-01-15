use bytes::{Buf, BufMut};

use crate::encoding::value_traits::{Collection, DistinguishedCollection};
use crate::encoding::{
    Capped, DecodeContext, DistinguishedEncoder, DistinguishedFieldEncoder,
    DistinguishedValueEncoder, Encoder, FieldEncoder, General, NewForOverwrite, Packed,
    TagMeasurer, TagWriter, ValueEncoder, WireType,
};
use crate::DecodeError;

pub struct Unpacked<E = General>(E);

/// Unpacked encodes vecs as repeated fields and in relaxed decoding will accept both packed
/// and un-packed encodings.
impl<C, T, E> Encoder<C> for Unpacked<E>
where
    C: Collection<Item = T>,
    E: ValueEncoder<T>,
    T: NewForOverwrite,
{
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
        for val in value.iter() {
            E::encode_field(tag, val, buf, tw);
        }
    }

    fn encoded_len(tag: u32, value: &C, tm: &mut TagMeasurer) -> usize {
        if !value.is_empty() {
            // Each *additional* field encoded after the first needs only 1 byte for the field key.
            tm.key_len(tag) + E::many_values_encoded_len(value.iter()) + value.len() - 1
        } else {
            0
        }
    }

    fn decode<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        if wire_type == WireType::LengthDelimited && E::WIRE_TYPE != WireType::LengthDelimited {
            // We've encountered a length-delimited field when we aren't expecting one; try decoding
            // it in packed format instead.
            if duplicated {
                return Err(DecodeError::new(
                    "multiple occurrences of packed repeated field",
                ));
            }
            Packed::<E>::decode_value(value, buf, ctx)
        } else {
            // Otherwise, decode one field normally.
            // TODO(widders): we would take more fields greedily here
            let mut new_val = T::new_for_overwrite();
            E::decode_field(wire_type, &mut new_val, buf, ctx)?;
            value.insert(new_val).map_err(DecodeError::new)?;
            Ok(())
        }
    }
}

/// Distinguished encoding enforces only the repeated field representation is allowed.
impl<C, T, E> DistinguishedEncoder<C> for Unpacked<E>
where
    Self: Encoder<C>,
    C: DistinguishedCollection<Item = T>,
    E: DistinguishedValueEncoder<T>,
    T: NewForOverwrite + Eq,
{
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        _duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<(), DecodeError> {
        let mut new_val = T::new_for_overwrite();
        E::decode_field_distinguished(wire_type, &mut new_val, buf, ctx)?;
        value
            .insert_distinguished(new_val)
            .map_err(DecodeError::new)?;
        Ok(())
    }
}
