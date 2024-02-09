use alloc::vec::Vec;

use bytes::{Buf, BufMut};

use crate::encoding::value_traits::HasEmptyState;
use crate::encoding::{
    delegate_encoding, encoder_where_value_encoder, Capped, DecodeContext, DistinguishedEncoder,
    DistinguishedFieldEncoder, DistinguishedValueEncoder, Encoder, FieldEncoder, TagMeasurer,
    TagWriter, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use crate::DecodeErrorKind::{NotCanonical, Truncated, UnexpectedlyRepeated};

pub struct Fixed;

delegate_encoding!(delegate from (Fixed) to (crate::encoding::Unpacked<Fixed>) for type (Vec<T>)
    including distinguished with generics (T));

encoder_where_value_encoder!(Fixed);

/// Macros which emit implementations for fixed width numeric encoding.
macro_rules! fixed_width_common {
    (
        $ty:ty,
        $wire_type:ident,
        $put:ident,
        $get:ident
    ) => {
        impl Wiretyped<$ty> for Fixed {
            const WIRE_TYPE: WireType = WireType::$wire_type;
        }

        impl ValueEncoder<$ty> for Fixed {
            #[inline]
            fn encode_value<B: BufMut + ?Sized>(value: &$ty, buf: &mut B) {
                buf.$put(*value);
            }

            #[inline]
            fn value_encoded_len(_value: &$ty) -> usize {
                WireType::$wire_type.fixed_size().unwrap()
            }

            #[inline]
            fn decode_value<B: Buf + ?Sized>(
                value: &mut $ty,
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if buf.remaining() < WireType::$wire_type.fixed_size().unwrap() {
                    return Err(DecodeError::new(Truncated));
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
        $wire_type:ident,
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

        #[cfg(test)]
        mod $test_name {
            use crate::encoding::Fixed;

            crate::encoding::test::check_type_test!(Fixed, expedient, $ty, WireType::$wire_type);
            crate::encoding::test::check_type_test!(
                Fixed,
                distinguished,
                $ty,
                WireType::$wire_type
            );
        }
    };
}

macro_rules! fixed_width_float {
    (
        $test_name:ident,
        $ty:ty,
        $wire_type:ident,
        $put:ident,
        $get:ident
    ) => {
        fixed_width_common!($ty, $wire_type, $put, $get);

        impl HasEmptyState for $ty {
            #[inline]
            fn is_empty(&self) -> bool {
                // Preserve -0.0. This is actually the original motivation for `HasEmptyState`.
                self.to_bits() == 0
            }
        }

        #[cfg(test)]
        mod $test_name {
            use crate::encoding::Fixed;
            crate::encoding::test::check_type_test!(Fixed, expedient, $ty, WireType::$wire_type);

            mod delegated_from_general {
                use crate::encoding::General;
                crate::encoding::test::check_type_test!(
                    General,
                    expedient,
                    $ty,
                    WireType::$wire_type
                );
            }
        }
    };
}

fixed_width_float!(f32, f32, ThirtyTwoBit, put_f32_le, get_f32_le);
fixed_width_float!(f64, f64, SixtyFourBit, put_f64_le, get_f64_le);
fixed_width_int!(fixed_u32, u32, ThirtyTwoBit, put_u32_le, get_u32_le);
fixed_width_int!(fixed_u64, u64, SixtyFourBit, put_u64_le, get_u64_le);
fixed_width_int!(fixed_i32, i32, ThirtyTwoBit, put_i32_le, get_i32_le);
fixed_width_int!(fixed_i64, i64, SixtyFourBit, put_i64_le, get_i64_le);
