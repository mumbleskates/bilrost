use crate::Context;
use alloc::vec::Vec;
use core::ops::Deref;
use proc_macro2::TokenStream;
use syn::Lifetime;

#[derive(Copy, Clone)]
pub enum DecodeMode {
    Relaxed,
    Distinguished,
}

#[derive(Clone)]
pub enum DecodeLifetime {
    Owned,
    Borrowed(Lifetime),
}

#[derive(Clone)]
pub enum WhereFor {
    Encode,
    Decode(DecodeLifetime, DecodeMode),
    Schema,
}

pub trait FieldBearer {
    /// Returns any and all where clause conditions asserting that this field has the given
    /// capability.
    fn where_terms(&self, purpose: WhereFor, ctx: &Context) -> Vec<TokenStream>;
}

/// Auto-flattening impl by slice
impl<T> FieldBearer for [T]
where
    T: FieldBearer,
{
    fn where_terms(&self, purpose: WhereFor, ctx: &Context) -> Vec<TokenStream> {
        self.iter()
            .flat_map(|bearer| bearer.where_terms(purpose.clone(), ctx))
            .collect()
    }
}

/// Auto-deref impl
impl<T, F: ?Sized> FieldBearer for T
where
    T: Deref<Target = F>,
    F: FieldBearer,
{
    fn where_terms(&self, purpose: WhereFor, ctx: &Context) -> Vec<TokenStream> {
        self.deref().where_terms(purpose, ctx)
    }
}

pub trait SinglyTagged {
    fn ref_tag(&self) -> &u32;

    fn tag(&self) -> u32 {
        *self.ref_tag()
    }
}

impl<T: SinglyTagged> SinglyTagged for &T {
    fn ref_tag(&self) -> &u32 {
        (**self).ref_tag()
    }
}

pub trait Tagged {
    fn tags(&self) -> &[u32];

    /// Returns the tag of this field with the least value
    fn first_tag(&self) -> u32 {
        self.tags()
            .iter()
            .copied()
            .min()
            .expect("no first tag when there are no tags")
    }

    /// Returns the tag of this field with the greatest value
    fn last_tag(&self) -> u32 {
        self.tags()
            .iter()
            .copied()
            .max()
            .expect("no last tag when there are no tags")
    }
}

impl<T: SinglyTagged> Tagged for T {
    fn tags(&self) -> &[u32] {
        core::slice::from_ref(self.ref_tag())
    }
}
