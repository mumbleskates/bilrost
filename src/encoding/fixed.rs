use bytes::{Buf, BufMut};

use crate::buf::ReverseBuf;
use crate::encoding::{
    delegate_value_encoding, encoding_implemented_via_value_encoding,
    Canonicity, Capped, DecodeContext, DistinguishedValueDecoder, RestrictedDecodeContext,
    ValueDecoder, ValueEncoder, WireType, Wiretyped,
};
use crate::DecodeError;
use crate::DecodeErrorKind::Truncated;

pub struct Fixed;

encoding_implemented_via_value_encoding!(Fixed);

/// Macros which emit implementations for fixed width numeric encoding.
macro_rules! fixed_width_common {
    (
        $ty:ty,
        $wire_type:ident,
        $put:ident,
        $prepend:ident,
        $get:ident
    ) => {
        impl Wiretyped<Fixed> for $ty {
            const WIRE_TYPE: WireType = WireType::$wire_type;
        }

        impl ValueEncoder<Fixed> for $ty {
            #[inline(always)]
            fn encode_value<B: BufMut + ?Sized>(value: &$ty, buf: &mut B) {
                buf.$put(*value);
            }

            #[inline(always)]
            fn prepend_value<B: ReverseBuf + ?Sized>(value: &$ty, buf: &mut B) {
                buf.$prepend(*value);
            }

            #[inline(always)]
            fn value_encoded_len(_value: &$ty) -> usize {
                WireType::$wire_type.fixed_size().unwrap()
            }
        }

        impl ValueDecoder<Fixed> for $ty {
            #[inline(always)]
            fn decode_value<B: Buf + ?Sized>(
                value: &mut $ty,
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if buf.remaining_before_cap() < WireType::$wire_type.fixed_size().unwrap() {
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
        $prepend:ident,
        $get:ident
    ) => {
        fixed_width_common!($ty, $wire_type, $put, $prepend, $get);
        delegate_value_encoding!(
            encoding (Fixed) borrows type ($ty) as owned including distinguished
        );

        impl DistinguishedValueDecoder<Fixed> for $ty {
            const CHECKS_EMPTY: bool = false;

            #[inline(always)]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut $ty,
                buf: Capped<impl Buf + ?Sized>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                ValueDecoder::<Fixed>::decode_value(value, buf, ctx.into_inner())?;
                Ok(Canonicity::Canonical)
            }
        }

        #[cfg(test)]
        mod $test_name {
            use crate::encoding::Fixed;

            crate::encoding::test::check_type_test!(Fixed, relaxed, $ty, WireType::$wire_type);
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
        $prepend:ident,
        $get:ident
    ) => {
        fixed_width_common!($ty, $wire_type, $put, $prepend, $get);
        delegate_value_encoding!(encoding (Fixed) borrows type ($ty) as owned);

        #[cfg(test)]
        mod $test_name {
            use crate::encoding::Fixed;
            crate::encoding::test::check_type_test!(Fixed, relaxed, $ty, WireType::$wire_type);

            mod delegated_from_general {
                use crate::encoding::General;
                crate::encoding::test::check_type_test!(
                    General,
                    relaxed,
                    $ty,
                    WireType::$wire_type
                );
            }
        }
    };
}

macro_rules! fixed_width_array {
    ($test_name:ident, $N:literal, $wire_type:ident) => {
        delegate_value_encoding!(
            encoding (Fixed) borrows type ([u8; $N]) as owned including distinguished
        );

        impl Wiretyped<Fixed> for [u8; $N] {
            const WIRE_TYPE: WireType = WireType::$wire_type;
        }

        impl ValueEncoder<Fixed> for [u8; $N] {
            #[inline(always)]
            fn encode_value<B: BufMut + ?Sized>(value: &[u8; $N], mut buf: &mut B) {
                (&mut buf).put(value.as_slice());
            }

            #[inline(always)]
            fn prepend_value<B: ReverseBuf + ?Sized>(value: &[u8; $N], buf: &mut B) {
                buf.prepend_slice(value.as_slice());
            }

            #[inline(always)]
            fn value_encoded_len(_value: &[u8; $N]) -> usize {
                $N
            }
        }

        impl ValueDecoder<Fixed> for [u8; $N] {
            #[inline(always)]
            fn decode_value<B: Buf + ?Sized>(
                value: &mut [u8; $N],
                mut buf: Capped<B>,
                _ctx: DecodeContext,
            ) -> Result<(), DecodeError> {
                if buf.remaining() < $N {
                    return Err(DecodeError::new(Truncated));
                }
                buf.copy_to_slice(value.as_mut_slice());
                Ok(())
            }
        }

        impl DistinguishedValueDecoder<Fixed> for [u8; $N] {
            const CHECKS_EMPTY: bool = false;

            #[inline(always)]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut [u8; $N],
                buf: Capped<impl Buf + ?Sized>,
                ctx: RestrictedDecodeContext,
            ) -> Result<Canonicity, DecodeError> {
                ValueDecoder::<Fixed>::decode_value(value, buf, ctx.into_inner())?;
                Ok(Canonicity::Canonical)
            }
        }

        #[cfg(test)]
        mod $test_name {
            use crate::encoding::Fixed;
            crate::encoding::test::check_type_test!(Fixed, relaxed, [u8; $N], WireType::$wire_type);
            crate::encoding::test::check_type_test!(
                Fixed,
                distinguished,
                [u8; $N],
                WireType::$wire_type
            );
        }
    };
}

fixed_width_float!(
    f32,
    f32,
    ThirtyTwoBit,
    put_f32_le,
    prepend_f32_le,
    get_f32_le
);
fixed_width_float!(
    f64,
    f64,
    SixtyFourBit,
    put_f64_le,
    prepend_f64_le,
    get_f64_le
);
fixed_width_int!(
    fixed_u32,
    u32,
    ThirtyTwoBit,
    put_u32_le,
    prepend_u32_le,
    get_u32_le
);
fixed_width_int!(
    fixed_u64,
    u64,
    SixtyFourBit,
    put_u64_le,
    prepend_u64_le,
    get_u64_le
);
fixed_width_int!(
    fixed_i32,
    i32,
    ThirtyTwoBit,
    put_i32_le,
    prepend_i32_le,
    get_i32_le
);
fixed_width_int!(
    fixed_i64,
    i64,
    SixtyFourBit,
    put_i64_le,
    prepend_i64_le,
    get_i64_le
);
fixed_width_array!(u8_4, 4, ThirtyTwoBit);
fixed_width_array!(u8_8, 8, SixtyFourBit);
