use crate::attrs::word_attr;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Meta, Type};

#[derive(Clone)]
pub struct Field {
    ty: Type,
}

impl Field {
    pub fn new(ty: &Type, attrs: &[Meta]) -> Result<Option<Box<Self>>, Error> {
        let ignore_attr_count = attrs
            .iter()
            .filter(|attr| word_attr(attr, "ignore"))
            .count();
        if ignore_attr_count == 0 {
            return Ok(None); // Field is not ignored
        }
        if ignore_attr_count > 1 {
            bail!(
                "duplicated ignore attributes for field: {}",
                quote!(#(#attrs),*)
            );
        }
        if attrs.len() > 1 {
            bail!(
                "ignore attribute mixed with other attributes on the same field: {}",
                quote!(#(#attrs),*)
            );
        }
        Ok(Some(Box::new(Self { ty: ty.clone() })))
    }

    pub fn initialize(&self) -> TokenStream {
        quote!(::core::default::Default::default())
    }

    pub fn where_terms(&self) -> Vec<TokenStream> {
        let ty = &self.ty;
        vec![quote!(#ty: ::core::default::Default)]
    }
}
