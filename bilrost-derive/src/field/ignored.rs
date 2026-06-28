use crate::attrs::{named_attr, word_attr};
use crate::field::ident_string;
use crate::field::traits::{FieldBearer, WhereFor};
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{format, vec};
use eyre::{bail, Report as Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_str, Expr, Generics, Ident, Meta, Type};

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
    Override(Expr),
}

/// This generates the method name for the `_BilrostInitializer<T>` associated function that
/// initializes ignored fields for `T`.
///
/// For oneof enumerations the field must also disambiguate which variant the field is in, so we
/// include the tag of the variant in the method name also.
fn init_method_name(variant_tag: Option<u32>, field_ident: &TokenStream) -> Ident {
    let variant_infix = variant_tag.map(|tag| format!("{tag}_"));
    let complete_name = format!(
        "init_{infix}{field}",
        infix = variant_infix.as_deref().unwrap_or(""),
        field = ident_string(field_ident),
    );
    parse_str(&complete_name).expect("failed to create a valid ident for init method")
}

impl IgnoredField {
    pub fn new(
        ty: &Type,
        attrs: &[Meta],
        default_init_mode: InitMode,
    ) -> Result<Option<Box<Self>>, Error> {
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
            Some(None) => default_init_mode,
            Some(Some(ignore_expr)) => InitMode::Override(ignore_expr),
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

    /// Initializer expression for this field, if we have one
    pub fn initialize(
        &self,
        variant_tag: Option<u32>,
        field_ident: &TokenStream,
    ) -> Option<TokenStream> {
        match &self.init_mode {
            InitMode::FromStructUpdate => None,
            InitMode::DefaultPerField => Some(quote!(::core::default::Default::default())),
            InitMode::Override(..) => {
                let method_name = init_method_name(variant_tag, field_ident);
                Some(quote!(_BilrostInitializer::<Self>::#method_name()))
            }
        }
    }

    /// The method body in `_BilrostInitializer` for initializing this field, if it needs one.
    pub fn initializer_method(
        &self,
        variant_tag: Option<u32>,
        field_ident: &TokenStream,
    ) -> Option<TokenStream> {
        if let InitMode::Override(ignore_expr) = &self.init_mode {
            let method_name = init_method_name(variant_tag, field_ident);
            let ty = &self.ty;
            Some(quote! {
                #[inline]
                fn #method_name() -> #ty {
                    #ignore_expr
                }
            })
        } else {
            None
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

/// Returns the implementation of the `_BilrostInitializer` type we define if we need any
/// initializer expressions for ignored fields.
pub fn initializer_class_definition(
    methods: impl IntoIterator<Item = TokenStream>,
    generics: &Generics,
) -> Option<TokenStream> {
    let all_methods: Vec<_> = methods.into_iter().collect();
    if all_methods.is_empty() {
        return None;
    }
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Some(quote! {
        struct _BilrostInitializer<T>(T);
        impl #impl_generics _BilrostInitializer<__Self #type_generics> #where_clause {
            #(#all_methods)*
        }
    })
}
