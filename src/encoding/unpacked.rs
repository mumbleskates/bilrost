use bytes::{Buf, BufMut};
use core::cmp::min;

use crate::encoding::value_traits::{Collection, DistinguishedCollection};
use crate::encoding::{
    Capped, DecodeContext, DistinguishedEncoder, DistinguishedFieldEncoder,
    DistinguishedValueEncoder, Encoder, FieldEncoder, General, NewForOverwrite, Packed,
    TagMeasurer, TagWriter, ValueEncoder, WireType,
};
use crate::DecodeErrorKind::UnexpectedlyRepeated;
use crate::{Canonicity, DecodeError};

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
                return Err(DecodeError::new(UnexpectedlyRepeated));
            }
            Packed::<E>::decode_value(value, buf, ctx)
        } else {
            // Otherwise, decode one field normally.
            // TODO(widders): we would take more fields greedily here
            let mut new_val = T::new_for_overwrite();
            E::decode_field(wire_type, &mut new_val, buf, ctx)?;
            Ok(value.insert(new_val)?)
        }
    }
}

/// Distinguished encoding enforces only the repeated field representation is allowed.
impl<C, T, E> DistinguishedEncoder<C> for Unpacked<E>
where
    Self: Encoder<C>,
    C: DistinguishedCollection<Item = T>,
    E: DistinguishedValueEncoder<T>,
    Packed<E>: ValueEncoder<C>,
    T: NewForOverwrite + Eq,
{
    fn decode_distinguished<B: Buf + ?Sized>(
        wire_type: WireType,
        duplicated: bool,
        value: &mut C,
        buf: Capped<B>,
        ctx: DecodeContext,
    ) -> Result<Canonicity, DecodeError> {
        if wire_type == WireType::LengthDelimited && E::WIRE_TYPE != WireType::LengthDelimited {
            // We've encountered a length-delimited field when we aren't expecting one; try decoding
            // it in packed format instead.
            if duplicated {
                return Err(DecodeError::new(UnexpectedlyRepeated));
            }
            // The data is already known to be non-canonical; use expedient decoding
            <Packed<E>>::decode_value(value, buf, ctx)?;
            Ok(Canonicity::NotCanonical)
        } else {
            // Otherwise, decode one field normally.
            // TODO(widders): we would take more fields greedily here
            let mut new_val = T::new_for_overwrite();
            Ok(min(
                E::decode_field_distinguished(wire_type, &mut new_val, buf, true, ctx)?,
                value.insert_distinguished(new_val)?,
            ))
        }
    }
}

#[cfg(test)]
mod test {
    use alloc::string::String;
    use alloc::vec::Vec;

    use proptest::proptest;

    use crate::encoding::test::{distinguished, expedient};
    use crate::encoding::{Fixed, Unpacked, WireType};

    proptest! {
        #[test]
        fn varint(value: Vec<u64>, tag: u32) {
            expedient::check_type_unpacked::<Vec<u64>, Unpacked>(
                value.clone(),
                tag,
                WireType::Varint,
            )?;
            distinguished::check_type_unpacked::<Vec<u64>, Unpacked>(value, tag, WireType::Varint)?;
        }

        #[test]
        fn length_delimited(value: Vec<String>, tag: u32) {
            expedient::check_type_unpacked::<Vec<String>, Unpacked>(
                value.clone(),
                tag,
                WireType::LengthDelimited,
            )?;
            distinguished::check_type_unpacked::<Vec<String>, Unpacked>(
                value,
                tag,
                WireType::LengthDelimited,
            )?;
        }

        #[test]
        fn fixed32(value: Vec<u32>, tag: u32) {
            expedient::check_type_unpacked::<Vec<u32>, Unpacked<Fixed>>(
                value.clone(),
                tag,
                WireType::ThirtyTwoBit,
            )?;
            distinguished::check_type_unpacked::<Vec<u32>, Unpacked<Fixed>>(
                value,
                tag,
                WireType::ThirtyTwoBit,
            )?;
        }

        #[test]
        fn fixed64(value: Vec<u64>, tag: u32) {
            expedient::check_type_unpacked::<Vec<u64>, Unpacked<Fixed>>(
                value.clone(),
                tag,
                WireType::SixtyFourBit,
            )?;
            distinguished::check_type_unpacked::<Vec<u64>, Unpacked<Fixed>>(
                value,
                tag,
                WireType::SixtyFourBit,
            )?;
        }
    }
}
