use crate::encoding::{
    empty_state_via_default, encode_varint, encoded_len_varint, encoder_where_value_encoder, Buf,
    BufMut, Canonicity, Capped, DecodeContext, DistinguishedValueEncoder, EmptyState, Encoder,
    TagMeasurer, TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use crate::DecodeErrorKind::OutOfDomainValue;

pub struct Varint;

encoder_where_value_encoder!(Varint);

/// Zig-zag encoding: These functions implement storing signed in unsigned integers by encoding the
/// sign bit in the least significant bit.
#[inline]
fn i8_to_unsigned(value: i8) -> u8 {
    ((value << 1) ^ (value >> 7)) as u8
}

#[inline]
fn u8_to_signed(value: u8) -> i8 {
    ((value >> 1) as i8) ^ (-((value & 1) as i8))
}

#[inline]
fn i16_to_unsigned(value: i16) -> u16 {
    ((value << 1) ^ (value >> 15)) as u16
}

#[inline]
fn u16_to_signed(value: u16) -> i16 {
    ((value >> 1) as i16) ^ (-((value & 1) as i16))
}

#[inline]
fn i32_to_unsigned(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

#[inline]
fn u32_to_signed(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

#[inline]
pub(crate) fn i64_to_unsigned(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

#[inline]
pub(crate) fn u64_to_signed(value: u64) -> i64 {
    ((value >> 1) as i64) ^ (-((value & 1) as i64))
}

/// Macro which emits implementations for variable width numeric encoding.
macro_rules! varint {
    (
        $name:ident,
        $ty:ty,
        to_uint64($to_uint64_value:ident) $to_uint64:expr,
        from_uint64($from_uint64_value:ident) $from_uint64:expr
    ) => {
        empty_state_via_default!($ty);

        impl Wiretyped<Varint> for $ty {
            const WIRE_TYPE: WireType = WireType::Varint;
        }

        impl ValueEncoder<Varint> for $ty {
            #[inline]
            fn encode_value<B: BufMut + ?Sized>($to_uint64_value: &$ty, buf: &mut B) {
                encode_varint($to_uint64, buf);
            }

            #[inline]
            fn value_encoded_len($to_uint64_value: &$ty) -> usize {
                encoded_len_varint($to_uint64)
            }

            #[inline]
            fn decode_value<B: Buf + ?Sized>(
                __value: &mut $ty,
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                let $from_uint64_value = buf.decode_varint()?;
                *__value = $from_uint64;
                Ok(())
            }
        }

        impl DistinguishedValueEncoder<Varint> for $ty {
            #[inline]
            fn decode_value_distinguished<B: Buf + ?Sized>(
                value: &mut $ty,
                buf: Capped<B>,
                allow_empty: bool,
                ctx: DecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                ValueEncoder::<Varint>::decode_value(value, buf, ctx)?;
                Ok(if !allow_empty && value.is_empty() {
                    Canonicity::NotCanonical
                } else {
                    Canonicity::Canonical
                })
            }
        }

        #[cfg(test)]
        mod $name {
            use crate::encoding::Varint;
            crate::encoding::test::check_type_test!(Varint, expedient, $ty, WireType::Varint);
            crate::encoding::test::check_type_test!(Varint, distinguished, $ty, WireType::Varint);
        }
    };
}

varint!(varint_bool, bool,
to_uint64(value) {
    u64::from(*value)
},
from_uint64(value) {
    match value {
        0 => false,
        1 => true,
        _ => return Err(DecodeError::new(OutOfDomainValue))
    }
});

varint!(varint_u8, u8,
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u8::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_u16, u16,
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u16::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_u32, u32,
to_uint64(value) {
    *value as u64
},
from_uint64(value) {
    u32::try_from(value).map_err(|_| DecodeError::new(OutOfDomainValue))?
});

varint!(varint_u64, u64,
to_uint64(value) {
    *value
},
from_uint64(value) {
    value
});

varint!(varint_i8, i8,
to_uint64(value) {
    i8_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u8::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    u8_to_signed(value)
});

varint!(varint_i16, i16,
to_uint64(value) {
    i16_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u16::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    u16_to_signed(value)
});

varint!(varint_i32, i32,
to_uint64(value) {
    i32_to_unsigned(*value) as u64
},
from_uint64(value) {
    let value = u32::try_from(value)
        .map_err(|_| DecodeError::new(OutOfDomainValue))?;
    u32_to_signed(value)
});

varint!(varint_i64, i64,
to_uint64(value) {
    i64_to_unsigned(*value)
},
from_uint64(value) {
    u64_to_signed(value)
});
