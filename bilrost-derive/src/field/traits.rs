use crate::field::Field;
use alloc::vec;
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

pub trait SinglyTagged {
    fn tag(&self) -> u32;
}

pub trait Tagged {
    fn tags(&self) -> Vec<u32>;

    /// Returns the tag of this field with the least value
    fn first_tag(&self) -> u32 {
        self.tags()
            .into_iter()
            .min()
            .expect("no first tag when there are no tags")
    }

    /// Returns the tag of this field with the greatest value
    fn last_tag(&self) -> u32 {
        self.tags()
            .into_iter()
            .max()
            .expect("no last tag when there are no tags")
    }
}

impl<T: SinglyTagged> Tagged for T {
    fn tags(&self) -> Vec<u32> {
        vec![self.tag()]
    }
}

pub trait FieldTarget {
    type Renamed: FieldTarget;

    fn self_expr(&self) -> TokenStream;
    fn const_field_ref(&self, field: &Field) -> TokenStream;
    fn mut_field_ref(&self, field: &Field) -> TokenStream;
    fn rename(&self, new_instance_ident: TokenStream) -> Self::Renamed;
}

impl<T: FieldTarget> FieldTarget for &T {
    type Renamed = T::Renamed;

    fn self_expr(&self) -> TokenStream {
        (**self).self_expr()
    }

    fn const_field_ref(&self, field: &Field) -> TokenStream {
        (**self).const_field_ref(field)
    }

    fn mut_field_ref(&self, field: &Field) -> TokenStream {
        (**self).mut_field_ref(field)
    }

    fn rename(&self, new_instance_ident: TokenStream) -> T::Renamed {
        (**self).rename(new_instance_ident)
    }
}
