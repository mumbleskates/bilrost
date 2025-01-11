//! These common macros allow deduplication of the code that defines common decoding for different
//! decoding modes.
//!
//! Arguments to use for `other_macro!`:
/*!
```rust
macro_rules! other_macro {
    (
        mode: $mode:ident,
        relaxed: $relaxed:ident::$relaxed_method:ident,
        relaxed_value: $relaxed_value:ident::$relaxed_value_method:ident,
        distinguished: $distinguished:ident::$distinguished_method:ident,
        distinguished_value: $distinguished_value:ident::$distinguished_value_method:ident,
        buf_ty: $buf_ty:ty,
        impl_buf_ty: $impl_buf_ty:ty,
        $(buf_generic: ($($buf_generic:tt)*),)?
        $(lifetime: $lifetime:lifetime,)?
    ) => { .. };
}
```
*/
//! * `mode`: will be `owned` or `borrowed`
//! * `relaxed`, `relaxed_value`, `distinguished`, etc.: traits and the associated decoding method
//!   name
//! * `buf_ty`: type of the buf to use in `Capped< >` when the method has a type generic applied
//! * `impl_buf_ty`: type of the buf to use in `Capped< >` when the method has a different generic
//!   and a generic buf type in Capped might need to be an `impl Trait` generic
//! * `buf_generic`: trait generics to use for any decoding method that doesn't have a different
//!   generic, like the `ALLOW_EMPTY` const generic for `DistinguishedValueDecoder`
//! * `lifetime`: lifetime required for the impl and for all listed decoding traits in addition to
//!   the encoder

macro_rules! invoke {
    ($other_macro:ident, owned) => {
        $other_macro!(
            mode: owned,
            relaxed: Decoder::decode,
            relaxed_value: ValueDecoder::decode_value,
            distinguished: DistinguishedDecoder::decode_distinguished,
            distinguished_value: DistinguishedValueDecoder::decode_value_distinguished,
            buf_ty: B,
            impl_buf_ty: impl Buf + ?Sized,
            buf_generic: (<B: Buf + ?Sized>),
        );
    };
    ($other_macro:ident, borrowed) => {
        $other_macro!(
            mode: borrowed,
            relaxed: BorrowDecoder::borrow_decode,
            relaxed_value: ValueBorrowDecoder::borrow_decode_value,
            distinguished: DistinguishedBorrowDecoder::borrow_decode_distinguished,
            distinguished_value: DistinguishedValueBorrowDecoder::borrow_decode_value_distinguished,
            buf_ty: &'a [u8],
            impl_buf_ty: &'a [u8],
            lifetime: 'a,
        );
    };
}
pub(crate) use invoke;
