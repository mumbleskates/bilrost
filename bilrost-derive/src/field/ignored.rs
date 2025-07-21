use crate::attrs::word_attr;
use crate::field::traits::{FieldBearer, WhereFor};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Report as Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Meta, Type};

#[derive(Clone)]
pub struct IgnoredField {
    ty: Type,
    init_mode: InitMode,
}

#[derive(Clone)]
pub enum InitMode {
    ParentDefault,
    DefaultPerField,
}

impl IgnoredField {
    pub fn new(ty: &Type, attrs: &[Meta], init_mode: InitMode) -> Result<Option<Box<Self>>, Error> {
        let ignore_attr_count = attrs
            .iter()
            .filter(|attr| word_attr(attr, "ignore"))
            .count();
        if ignore_attr_count == 0 {
            return Ok(None); // Field is not ignored
        }
        if ignore_attr_count > 1 {
            bail!(
                "duplicated ignore attributes for field: {attrs}",
                attrs = quote!(#(#attrs),*),
            );
        }
        if attrs.len() > 1 {
            bail!(
                "ignore attribute mixed with other attributes on the same field: {attrs}",
                attrs = quote!(#(#attrs),*),
            );
        }
        Ok(Some(Box::new(Self {
            ty: ty.clone(),
            init_mode,
        })))
    }

    pub fn initialize(&self) -> Option<TokenStream> {
        match self.init_mode {
            InitMode::ParentDefault => None,
            InitMode::DefaultPerField => Some(quote!(::core::default::Default::default())),
        }
    }
}

impl FieldBearer for IgnoredField {
    fn where_terms(&self, _purpose: WhereFor) -> Vec<TokenStream> {
        match self.init_mode {
            InitMode::ParentDefault => vec![],
            InitMode::DefaultPerField => {
                let ty = &self.ty;
                vec![quote!(#ty: ::core::default::Default)]
            }
        }
    }
}
