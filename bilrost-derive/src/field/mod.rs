mod oneof;
mod value;

use std::any::type_name;
use std::fmt;

use anyhow::{anyhow, bail, Error};
use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::{
    parse, parse2, parse_str, Attribute, Expr, Lit, LitInt, Meta, MetaList, MetaNameValue, Token,
    Type,
};

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

        Ok(if let Some(field) = oneof::Field::new(&ty, &attrs)? {
            Some(Field::Oneof(field))
        } else {
            value::Field::new(&ty, &attrs, inferred_tag)?.map(Field::Value)
        })
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
    pub fn expedient_where_terms(&self) -> Vec<TokenStream> {
        match self {
            Field::Value(field) => field.expedient_where_terms(),
            Field::Oneof(field) => field.expedient_where_terms(),
        }
    }

    /// Returns the where clause condition asserting that this field's encoder encodes its type in
    /// distinguished mode.
    pub fn distinguished_where_terms(&self) -> Vec<TokenStream> {
        match self {
            Field::Value(field) => field.distinguished_where_terms(),
            Field::Oneof(field) => field.distinguished_where_terms(),
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
                    ::bilrost::assert_tags_are_equal(
                        #description,
                        <#oneof_ty as ::bilrost::encoding::Oneof>::FIELD_TAGS,
                        &[#(#tags),*],
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

    /// Returns an expression which evaluates to the result of decoding a value into the field.
    pub fn decode_expedient(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.decode_expedient(ident),
            Field::Oneof(oneof) => oneof.decode_expedient(ident),
        }
    }

    /// Returns an expression which evaluates to the result of decoding a value into the field in
    /// distinguished mode.
    pub fn decode_distinguished(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.decode_distinguished(ident),
            Field::Oneof(oneof) => oneof.decode_distinguished(ident),
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

pub fn set_bool(b: &mut bool, message: &str) -> Result<(), Error> {
    if *b {
        bail!("{}", message);
    } else {
        *b = true;
        Ok(())
    }
}

fn tag_attr(attr: &Meta) -> Result<Option<u32>, Error> {
    if !attr.path().is_ident("tag") {
        return Ok(None);
    }
    match attr {
        // tag(1)
        Meta::List(meta_list) => Ok(Some(meta_list.parse_args::<LitInt>()?.base10_parse()?)),
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            // tag = "1"
            Lit::Str(lit) => lit.value().parse::<u32>().map_err(Error::from).map(Some),
            // tag = 1
            Lit::Int(lit) => Ok(Some(lit.base10_parse()?)),
            _ => bail!("invalid tag attribute: {}", quote!(#attr)),
        },
        _ => bail!("invalid tag attribute: {}", quote!(#attr)),
    }
}

fn named_attr<T: parse::Parse>(attr: &Meta, attr_name: &str) -> Result<Option<T>, Error> {
    if !attr.path().is_ident(attr_name) {
        return Ok(None);
    }
    match attr {
        // encoding(type tokens go here)
        Meta::List(MetaList { tokens, .. }) => parse2(tokens.clone()),
        // encoding = "type tokens go here"
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            Lit::Str(lit) => parse_str::<T>(&lit.value()),
            _ => bail!("invalid {attr_name} attribute: {}", quote!(#attr)),
        },
        _ => bail!("invalid {attr_name} attribute: {}", quote!(#attr)),
    }
    .map(Some)
    .map_err(|_| {
        anyhow!(
            "invalid {attr_name} attribute does not look like a(n) {}: {}",
            type_name::<T>(),
            quote!(#attr),
        )
    })
}

/// Checks if an attribute matches a word.
fn word_attr(attr: &Meta, key: &str) -> bool {
    if let Meta::Path(ref path) = *attr {
        path.is_ident(key)
    } else {
        false
    }
}
