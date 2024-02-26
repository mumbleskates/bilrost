use bytes::{Buf, BufMut};

use crate::encoding::value_traits::{Collection, DistinguishedCollection};
use crate::encoding::{
    check_wire_type, Capped, DecodeContext, DistinguishedEncoder, DistinguishedValueEncoder,
    Encoder, FieldEncoder, General, NewForOverwrite, Packed, TagMeasurer, TagWriter, ValueEncoder,
    WireType, Wiretyped,
};
use crate::DecodeErrorKind::UnexpectedlyRepeated;
use crate::{Canonicity, DecodeError};

pub struct Unpacked<E = General>(E);

/// Returns `Some` if there are more bytes in the buffer and the next data in the buffer begins
/// with a "repeated" field key (a key with a tag delta of zero).
#[inline(always)]
fn peek_repeated_field<B: Buf + ?Sized>(buf: &mut Capped<B>) -> Option<WireType> {
    if buf.remaining_before_cap() == 0 {
        return None;
    }
    // Peek the first byte of the next field's key.
    let peek_key = buf.chunk()[0];
    if peek_key >= 4 {
        return None; // The next field has a different tag than this one.
    }
    // The next field's key has a repeated tag (its delta is zero). Consume the peeked key and
    // return its wire type
    buf.advance(1);
    Some(WireType::from(peek_key))
}

/// Decodes a collection value from the unpacked representation. This greedily consumes consecutive
/// fields as long as they have the same tag.
#[inline]
pub(crate) fn decode<T, E>(
    wire_type: WireType,
    collection: &mut T,
    mut buf: Capped<impl Buf + ?Sized>,
    ctx: DecodeContext,
) -> Result<(), DecodeError>
where
    T: Collection,
    T::Item: NewForOverwrite + ValueEncoder<E>,
{
    check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
    loop {
        // Decode one item
        let mut new_item = T::Item::new_for_overwrite();
        ValueEncoder::<E>::decode_value(&mut new_item, buf.lend(), ctx.clone())?;
        collection.insert(new_item)?;

        if let Some(next_wire_type) = peek_repeated_field(&mut buf) {
            check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, next_wire_type)?;
        } else {
            break;
        }
    }
    Ok(())
}

/// Decodes a collection value from the unpacked representation in distinguished mode. This greedily
/// consumes consecutive fields as long as they have the same tag.
#[inline]
pub(crate) fn decode_distinguished<T, E>(
    wire_type: WireType,
    collection: &mut T,
    mut buf: Capped<impl Buf + ?Sized>,
    ctx: DecodeContext,
) -> Result<Canonicity, DecodeError>
where
    T: DistinguishedCollection,
    T::Item: NewForOverwrite + Eq + DistinguishedValueEncoder<E>,
{
    check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, wire_type)?;
    let mut canon = Canonicity::Canonical;
    loop {
        // Decode one item
        let mut new_item = T::Item::new_for_overwrite();
        canon.update(DistinguishedValueEncoder::<E>::decode_value_distinguished(
            &mut new_item,
            buf.lend(),
            true,
            ctx.clone(),
        )?);
        canon.update(collection.insert_distinguished(new_item)?);

        if let Some(next_wire_type) = peek_repeated_field(&mut buf) {
            check_wire_type(<T::Item as Wiretyped<E>>::WIRE_TYPE, next_wire_type)?;
        } else {
            break;
        }
    }
    Ok(canon)
}

/// Unpacked encodes vecs as repeated fields and in relaxed decoding will accept both packed
/// and un-packed encodings.
impl<C, T, E> Encoder<Unpacked<E>> for C
where
    C: Collection<Item = T>,
    T: NewForOverwrite + ValueEncoder<E>,
{
    fn encode<B: BufMut + ?Sized>(tag: u32, value: &C, buf: &mut B, tw: &mut TagWriter) {
        for val in value.iter() {
            FieldEncoder::<E>::encode_field(tag, val, buf, tw);
        }
    }

    fn encoded_len(tag: u32, value: &C, tm: &mut TagMeasurer) -> usize {
        if !value.is_empty() {
            // Each *additional* field encoded after the first needs only 1 byte for the field key.
            tm.key_len(tag) + ValueEncoder::<E>::many_values_encoded_len(value.iter()) + value.len()
                - 1
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
        if duplicated {
            return Err(DecodeError::new(UnexpectedlyRepeated));
        }
        if wire_type == WireType::LengthDelimited
            && <C::Item as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
        {
            // We've encountered a length-delimited field when we aren't expecting one; try decoding
            // it in packed format instead.
            ValueEncoder::<Packed<E>>::decode_value(value, buf, ctx)
        } else {
            // Otherwise, decode in unpacked mode.
            decode::<C, E>(wire_type, value, buf, ctx)
        }
    }
}

/// Distinguished encoding enforces only the repeated field representation is allowed.
impl<C, T, E> DistinguishedEncoder<Unpacked<E>> for C
where
    Self: DistinguishedCollection<Item = T> + ValueEncoder<Packed<E>> + Encoder<Unpacked<E>>,
    T: NewForOverwrite + Eq + DistinguishedValueEncoder<E>,
{
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
        if wire_type == WireType::LengthDelimited
            && <T as Wiretyped<E>>::WIRE_TYPE != WireType::LengthDelimited
        {
            // We've encountered a length-delimited field when we aren't expecting one; try decoding
            // it in packed format instead.
            // The data is already known to be non-canonical; use expedient decoding
            <C as ValueEncoder<Packed<E>>>::decode_value(value, buf, ctx)?;
            Ok(Canonicity::NotCanonical)
        } else {
            // Otherwise, decode in unpacked mode.
            decode_distinguished::<C, E>(wire_type, value, buf, ctx)
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
