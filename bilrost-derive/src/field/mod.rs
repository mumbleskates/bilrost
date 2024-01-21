mod oneof;
mod value;

use std::collections::BTreeMap;
use std::fmt;

use anyhow::{bail, Error};
use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::{parse2, Attribute, LitInt, Meta, Token, Type, WhereClause};

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
    pub fn new(ty: Type, attrs: Vec<Attribute>, inferred_tag: u32) -> Result<Option<Field>, Error> {
        let attrs = bilrost_attrs(attrs)?;

        // TODO: check for ignore attribute.

        Ok(Some(if let Some(field) = oneof::Field::new(&ty, &attrs)? {
            Field::Oneof(field)
        } else {
            Field::Value(value::Field::new(&ty, &attrs, inferred_tag)?)
        }))
    }

    pub fn new_in_oneof(
        ty: Type,
        ident_within_variant: Option<Ident>,
        attrs: Vec<Attribute>,
    ) -> Result<Field, Error> {
        Ok(Field::Value(value::Field::new_in_oneof(
            &ty,
            ident_within_variant,
            &bilrost_attrs(attrs)?,
        )?))
    }

    pub fn tags(&self) -> Vec<u32> {
        match self {
            Field::Value(scalar) => vec![scalar.tag],
            Field::Oneof(oneof) => oneof.tags.clone(),
        }
    }

    /// Returns the tag of this field with the least value
    pub fn first_tag(&self) -> u32 {
        self.tags().into_iter().min().unwrap()
    }

    /// Returns the tag of this field with the greatest value
    pub fn last_tag(&self) -> u32 {
        self.tags().into_iter().max().unwrap()
    }

    /// Returns the where clause condition asserting that this field's encoder encodes its type.
    fn encoder_where(&self) -> Option<TokenStream> {
        match self {
            Field::Value(field) => Some(field.encoder_where()),
            _ => None,
        }
    }

    /// Combines an optional already-existing where clause with additional terms for each field's
    /// encoder to assert that it supports the field's type.
    pub fn append_wheres<'a>(
        where_clause: Option<&WhereClause>,
        fields: impl Iterator<Item = &'a Self>,
    ) -> TokenStream {
        // dedup the where clauses by their String values
        let encoder_wheres: BTreeMap<_, _> = fields
            .flat_map(|f| f.encoder_where())
            .map(|where_| (where_.to_string(), where_))
            .collect();
        let encoder_wheres: Vec<_> = encoder_wheres.values().collect();
        if let Some(where_clause) = where_clause {
            quote! { #where_clause #(, #encoder_wheres)* }
        } else if encoder_wheres.is_empty() {
            quote!() // no where clause terms
        } else {
            quote! { where #(#encoder_wheres),*}
        }
    }

    pub fn tag_list_guard(&self, field_name: String) -> Option<TokenStream> {
        match self {
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
                        <#oneof_ty as ::bilrost::encoding::Oneof>::FIELD_TAGS,
                        &[#(#tags,)*],
                    );
                ))
            }
            _ => None,
        }
    }

    /// Spells a value for the field as an enum variant with the given value.
    pub fn with_value(&self, value: TokenStream) -> TokenStream {
        match self {
            Field::Value(field) => field.with_value(value),
            Field::Oneof(_) => {
                panic!(
                    "trying to spell a field's value within a oneof variant, but the field is a \
                oneof, not part of a oneof"
                );
            }
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

    /// If the field is a oneof, returns an expression which evaluates to an Option<u32> of the tag
    /// of the (maybe) present field in the oneof. Panics if the field is not a oneof.
    pub fn current_tag(&self, ident: TokenStream) -> TokenStream {
        let Field::Oneof(field) = self else {
            panic!("tried to use a value field as a oneof")
        };
        field.current_tag(ident)
    }

    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
        match self {
            Field::Value(scalar) => scalar.methods(ident),
            _ => None,
        }
    }
}

/// Get the items belonging to the 'bilrost' list attribute, e.g. `#[bilrost(foo, bar="baz")]`.
pub(crate) fn bilrost_attrs(attrs: Vec<Attribute>) -> Result<Vec<Meta>, Error> {
    let mut result = Vec::new();
    for attr in attrs.iter() {
        if let Meta::List(meta_list) = &attr.meta {
            if meta_list.path.is_ident("bilrost") {
                // `bilrost(1)` is transformed into `bilrost(tag = 1)` as a shorthand
                if let Ok(short_tag) = parse2::<LitInt>(meta_list.tokens.clone()) {
                    result.push(parse2::<Meta>(quote!(tag = #short_tag)).unwrap());
                } else {
                    result.extend(
                        meta_list
                            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?
                            .into_iter(),
                    );
                }
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
