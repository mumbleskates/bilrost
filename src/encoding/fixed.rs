use alloc::vec::Vec;

use crate::encoding::{
    delegate_encoding, Capped, DecodeContext, DistinguishedEncoder, DistinguishedFieldEncoder,
    DistinguishedValueEncoder, Encoder, FieldEncoder, TagMeasurer, TagWriter, ValueEncoder,
    WireType, Wiretyped,
};
use crate::DecodeError;

use bytes::{Buf, BufMut};

pub struct Fixed;

delegate_encoding!(delegate from (Fixed) to (crate::encoding::Unpacked<Fixed>) for type (Vec<T>)
    including distinguished with generics <T>);

/// Macros which emit implementations for fixed width numeric encoding.
macro_rules! fixed_width_common {
    (
        $ty:ty,
        $wire_type:expr,
        $put:ident,
        $get:ident
    ) => {
        impl Wiretyped<$ty> for Fixed {
            const WIRE_TYPE: WireType = $wire_type;
        }

        impl ValueEncoder<$ty> for Fixed {
            #[inline]
            fn encode_value<B: BufMut + ?Sized>(value: &$ty, buf: &mut B) {
                buf.$put(*value);
            }

            #[inline]
            fn value_encoded_len(_value: &$ty) -> usize {
                $wire_type.fixed_size().unwrap()
            }

            #[inline]
            fn decode_value<B: Buf + ?Sized>(
                value: &mut $ty,
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if buf.remaining() < $wire_type.fixed_size().unwrap() {
                    return Err(DecodeError::new("field truncated"));
                }
                *value = buf.$get();
                Ok(())
            }
        }
    };
}

macro_rules! fixed_width_int {
    (
        $test_name:ident,
        $ty:ty,
        $wire_type:expr,
        $put:ident,
        $get:ident
    ) => {
        fixed_width_common!($ty, $wire_type, $put, $get);

        impl DistinguishedValueEncoder<$ty> for Fixed {
            #[inline]
            fn decode_value_distinguished<B: Buf + ?Sized>(
                value: &mut $ty,
                buf: Capped<B>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                Fixed::decode_value(value, buf, ctx)
            }
        }

        impl Encoder<$ty> for Fixed {
            #[inline]
            fn encode<B: BufMut + ?Sized>(tag: u32, value: &$ty, buf: &mut B, tw: &mut TagWriter) {
                if *value != 0 {
                    Self::encode_field(tag, value, buf, tw);
                }
            }

            #[inline]
            fn encoded_len(tag: u32, value: &$ty, tm: &mut TagMeasurer) -> usize {
                if *value != 0 {
                    Self::field_encoded_len(tag, value, tm)
                } else {
                    0
                }
            }

            #[inline]
            fn decode<B: Buf + ?Sized>(
                wire_type: WireType,
                duplicated: bool,
                value: &mut $ty,
                buf: Capped<B>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(
                        "multiple occurrences of non-repeated field",
                    ));
                }
                Self::decode_field(wire_type, value, buf, ctx)
            }
        }

        impl DistinguishedEncoder<$ty> for Fixed {
            #[inline]
            fn decode_distinguished<B: Buf + ?Sized>(
                wire_type: WireType,
                duplicated: bool,
                value: &mut $ty,
                buf: Capped<B>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(
                        "multiple occurrences of non-repeated field",
                    ));
                }
                Self::decode_field_distinguished(wire_type, value, buf, ctx)?;
                if *value == 0 {
                    return Err(DecodeError::new(
                        "plain field was encoded with its zero value",
                    ));
                }
                Ok(())
            }
        }

        #[cfg(test)]
        mod $test_name {
            crate::encoding::check_type_test!(Fixed, expedient, $ty, $wire_type);
            crate::encoding::check_type_test!(Fixed, distinguished, $ty, $wire_type);
        }
    };
}

macro_rules! fixed_width_float {
    (
        $test_name:ident,
        $ty:ty,
        $wire_type:expr,
        $put:ident,
        $get:ident
    ) => {
        fixed_width_common!($ty, $wire_type, $put, $get);

        impl Encoder<$ty> for Fixed {
            #[inline]
            fn encode<B: BufMut + ?Sized>(tag: u32, value: &$ty, buf: &mut B, tw: &mut TagWriter) {
                // Preserve -0.0
                if value.to_bits() != 0 {
                    Self::encode_field(tag, value, buf, tw);
                }
            }

            #[inline]
            fn encoded_len(tag: u32, value: &$ty, tm: &mut TagMeasurer) -> usize {
                // Preserve -0.0
                if value.to_bits() != 0 {
                    Self::field_encoded_len(tag, value, tm)
                } else {
                    0
                }
            }

            #[inline]
            fn decode<B: Buf + ?Sized>(
                wire_type: WireType,
                duplicated: bool,
                value: &mut $ty,
                buf: Capped<B>,
                ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if duplicated {
                    return Err(DecodeError::new(
                        "multiple occurrences of non-repeated field",
                    ));
                }
                Self::decode_field(wire_type, value, buf, ctx)
            }
        }

        #[cfg(test)]
        mod $test_name {
            crate::encoding::check_type_test!(Fixed, expedient, $ty, $wire_type);

            mod delegated_from_general {
                crate::encoding::check_type_test!(General, expedient, $ty, $wire_type);
            }
        }
    };
}

fixed_width_float!(f32, f32, WireType::ThirtyTwoBit, put_f32_le, get_f32_le);
fixed_width_float!(f64, f64, WireType::SixtyFourBit, put_f64_le, get_f64_le);
fixed_width_int!(
    fixed_u32,
    u32,
    WireType::ThirtyTwoBit,
    put_u32_le,
    get_u32_le
);
fixed_width_int!(
    fixed_u64,
    u64,
    WireType::SixtyFourBit,
    put_u64_le,
    get_u64_le
);
fixed_width_int!(
    fixed_i32,
    i32,
    WireType::ThirtyTwoBit,
    put_i32_le,
    get_i32_le
);
fixed_width_int!(
    fixed_i64,
    i64,
    WireType::SixtyFourBit,
    put_i64_le,
    get_i64_le
);
