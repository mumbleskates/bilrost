use alloc::vec::Vec;
use core::ops::Deref;
use proc_macro2::TokenStream;

#[derive(Copy, Clone)]
pub enum DecodeMode {
    Relaxed,
    Distinguished,
}

#[derive(Copy, Clone)]
pub enum DecodeLifetime {
    Owned,
    Borrowed,
}

#[derive(Copy, Clone)]
pub enum WhereFor {
    Encode,
    Decode(DecodeLifetime, DecodeMode),
}

pub trait FieldBearer {
    /// Returns any and all where clause conditions asserting that this field has the given
    /// capability.
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream>;
}

/// Auto-flattening impl by slice
impl<T> FieldBearer for [T]
where
    T: FieldBearer,
{
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        self.iter()
            .flat_map(|bearer| bearer.where_terms(purpose))
            .collect()
    }
}

/// Auto-deref impl
impl<T, F: ?Sized> FieldBearer for T
where
    T: Deref<Target = F>,
    F: FieldBearer,
{
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        self.deref().where_terms(purpose)
    }
}
