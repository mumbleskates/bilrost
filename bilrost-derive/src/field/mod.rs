mod oneof;
mod value;

use std::fmt;

use anyhow::{bail, Error};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, ExprLit, Lit, LitInt, Meta, MetaNameValue, Token, Type};

#[derive(Clone)]
pub enum Field {
    /// A scalar field.
    Value(value::Field),
    /// A oneof field.
    Oneof(oneof::Field),
}

impl Field {
    /// Creates a new `Field` from an iterator of field attributes.
    ///
    /// If the meta items are invalid, an error will be returned.
    /// If the field should be ignored, `None` is returned.
    pub fn new(
        ty: Type,
        attrs: Vec<Attribute>,
        inferred_tag: Option<u32>,
    ) -> Result<Option<Field>, Error> {
        let attrs = bilrost_attrs(attrs)?;

        // TODO: check for ignore attribute.

        Ok(Some(if let Some(field) = oneof::Field::new(&attrs)? {
            Field::Oneof(field)
        } else {
            Field::Value(value::Field::new(ty, &attrs, inferred_tag)?)
        }))
    }

    pub fn tags(&self) -> Vec<u32> {
        match self {
            Field::Value(scalar) => vec![scalar.tag],
            Field::Oneof(oneof) => oneof.tags.clone(),
        }
    }

    pub fn first_tag(&self) -> u32 {
        self.tags().into_iter().min().unwrap()
    }

    pub fn last_tag(&self) -> u32 {
        self.tags().into_iter().max().unwrap()
    }

    pub fn tag_list_guard(&self, field_name: String) -> Option<TokenStream> {
        match &self {
            Field::Oneof(field) => {
                let mut tags = self.tags();
                tags.sort();
                let oneof_ty = &field.ty;
                let oneof_ty_name = oneof_ty.to_token_stream().to_string();
                let description = format!(
                    "tags don't match for oneof field {field_name} with type {oneof_ty_name}"
                );
                let description = description.as_str();
                // Static assertion pattern borrowed from static_assertions crate.
                Some(quote!(
                    const _: () = ::bilrost::assert_tags_are_equal(
                        #description,
                        #oneof_ty::FIELD_TAGS,
                        [#(#tags,)*]
                    );
                ))
            }
            _ => None,
        }
    }

    /// Returns a statement which encodes the field.
    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.encode(ident),
            Field::Oneof(oneof) => oneof.encode(ident),
        }
    }

    /// Returns an expression which evaluates to the result of merging a decoded
    /// value into the field.
    pub fn decode(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.decode(ident),
            Field::Oneof(oneof) => oneof.decode(ident),
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.encoded_len(ident),
            Field::Oneof(oneof) => oneof.encoded_len(ident),
        }
    }

    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
        match self {
            Field::Value(scalar) => scalar.methods(ident),
            _ => None,
        }
    }
}

/// Get the items belonging to the 'bilrost' list attribute, e.g. `#[bilrost(foo, bar="baz")]`.
fn bilrost_attrs(attrs: Vec<Attribute>) -> Result<Vec<Meta>, Error> {
    let mut result = Vec::new();
    for attr in attrs.iter() {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("bilrost") {
                result.extend(
                    meta_list
                        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
                        .into_iter(),
                )
            }
        }
    }
    Ok(result)
}

pub fn set_option<T>(option: &mut Option<T>, value: T, message: &str) -> Result<(), Error>
where
    T: fmt::Debug,
{
    if let Some(existing) = option {
        bail!("{}: {:?} and {:?}", message, existing, value);
    }
    *option = Some(value);
    Ok(())
}

pub(super) fn tag_attr(attr: &Meta) -> Result<Option<u32>, Error> {
    if !attr.path().is_ident("tag") {
        return Ok(None);
    }
    match attr {
        Meta::List(meta_list) => Ok(Some(meta_list.parse_args::<LitInt>()?.base10_parse()?)),
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            Lit::Str(lit) => lit.value().parse::<u32>().map_err(Error::from).map(Some),
            Lit::Int(lit) => Ok(Some(lit.base10_parse()?)),
            _ => bail!("invalid tag attribute: {:?}", attr),
        },
        _ => bail!("invalid tag attribute: {:?}", attr),
    }
}

fn tags_attr(attr: &Meta) -> Result<Option<Vec<u32>>, Error> {
    if !attr.path().is_ident("tags") {
        return Ok(None);
    }
    match attr {
        Meta::List(meta_list) => Ok(Some(
            meta_list
                .parse_args_with(Punctuated::<LitInt, Token![,]>::parse_terminated)?
                .iter()
                .map(LitInt::base10_parse)
                .collect::<Result<Vec<_>, _>>()?,
        )),
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(ExprLit {
                lit: Lit::Str(lit), ..
            }),
            ..
        }) => lit
            .value()
            .split(',')
            .map(|s| s.trim().parse::<u32>().map_err(Error::from))
            .collect::<Result<Vec<u32>, _>>()
            .map(Some),
        _ => bail!("invalid tag attribute: {:?}", attr),
    }
}
