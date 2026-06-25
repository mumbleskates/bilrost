use crate::attrs::{named_attr, word_attr};
use crate::field::traits::{FieldBearer, WhereFor};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Report as Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, Meta, Type};

#[derive(Clone)]
pub struct IgnoredField {
    ty: Type,
    init_mode: InitMode,
}

#[derive(Clone)]
pub enum InitMode {
    /// Field will be initialized via some value of the whole struct
    FromStructUpdate,
    /// Field will be initialized by its own type's Default
    DefaultPerField,
    /// Field will be initialized by the given expression
    Override(TokenStream),
}

impl IgnoredField {
    pub fn new(ty: &Type, attrs: &[Meta], init_mode: InitMode) -> Result<Option<Box<Self>>, Error> {
        let mut ignore_attr = None;
        for attr in attrs {
            let this_attr = if word_attr(attr, "ignore") {
                None
            } else if let Some(ignore_expr) = named_attr::<Expr>(attr, "ignore")? {
                Some(ignore_expr)
            } else {
                continue;
            };
            if ignore_attr.replace(this_attr).is_some() {
                bail!(
                    "duplicated ignore attributes for field: {attrs}",
                    attrs = quote!(#(#attrs),*),
                );
            }
        }
        let field_init_mode = match ignore_attr {
            None => return Ok(None),
            Some(None) => init_mode,
            Some(Some(ignore_expr)) => InitMode::Override(quote!(#ignore_expr)),
        };
        if attrs.len() > 1 {
            bail!(
                "ignore attribute mixed with other attributes on the same field: {attrs}",
                attrs = quote!(#(#attrs),*),
            );
        }
        Ok(Some(Box::new(Self {
            ty: ty.clone(),
            init_mode: field_init_mode,
        })))
    }

    pub fn initialize(&self) -> Option<TokenStream> {
        match &self.init_mode {
            InitMode::FromStructUpdate => None,
            InitMode::DefaultPerField => Some(quote!(::core::default::Default::default())),
            InitMode::Override(token_stream) => Some(token_stream.clone()),
        }
    }

    pub fn uses_struct_update_syntax(&self) -> bool {
        matches!(self.init_mode, InitMode::FromStructUpdate)
    }
}

impl FieldBearer for IgnoredField {
    fn where_terms(&self, _purpose: WhereFor) -> Vec<TokenStream> {
        match self.init_mode {
            InitMode::FromStructUpdate | InitMode::Override(..) => vec![],
            InitMode::DefaultPerField => {
                let ty = &self.ty;
                vec![quote!(#ty: ::core::default::Default)]
            }
        }
    }
}
