use crate::crate_name;
use alloc::boxed::Box;
use alloc::fmt::Debug;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Error};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::{parse2, Attribute, LitInt, Meta, Token, Type};
use traits::{DecodeLifetime, DecodeMode, WhereFor};

mod ignored;
mod oneof;
pub mod traits;
mod value;

pub use value::{FieldInVariant, OneofVariant, VariantContents};
use crate::field::traits::FieldBearer;

#[derive(Clone)]
pub enum Field {
    /// A single-value message field.
    Value(Box<value::MessageField>),
    /// A oneof field.
    Oneof(Box<oneof::OneofInclusion>),
    /// An ignored field.
    Ignored(Box<ignored::IgnoredField>),
}

impl Field {
    /// Creates a new `Field` from an iterator of field attributes.
    ///
    /// If the meta items are invalid, an error will be returned.
    /// If the field should be ignored, `None` is returned.
    pub fn new(ty: Type, attrs: &[Attribute], inferred_tag: Option<u32>) -> Result<Field, Error> {
        let attrs = bilrost_attrs(attrs)?;

        Ok(if let Some(field) = ignored::IgnoredField::new(&ty, &attrs)? {
            Field::Ignored(field)
        } else if let Some(field) = oneof::OneofInclusion::new(&ty, &attrs)? {
            Field::Oneof(field)
        } else {
            Field::Value(value::MessageField::new(&ty, &attrs, inferred_tag)?)
        })
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self, Field::Ignored(_))
    }

    pub fn tags(&self) -> Vec<u32> {
        match self {
            Field::Value(scalar) => vec![scalar.tag],
            Field::Oneof(oneof) => oneof.tags.clone(),
            Field::Ignored(_) => panic!("field is ignored"),
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

    pub fn tag_list_guard(&self, field_name: String) -> Option<TokenStream> {
        let crate_ = crate_name();
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
                    #crate_::assert_tags_are_equal(
                        #description,
                        <#oneof_ty as #crate_::encoding::Oneof>::FIELD_TAGS,
                        &[#(#tags),*],
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
            Field::Ignored(_) => panic!("field is ignored"),
        }
    }

    /// Returns a statement which prepends the field.
    pub fn prepend(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.prepend(ident),
            Field::Oneof(oneof) => oneof.prepend(ident),
            Field::Ignored(_) => panic!("field is ignored"),
        }
    }

    /// Returns an expression which evaluates to the result of decoding a value into the field.
    pub fn decode(
        &self,
        ident: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.decode(ident, lifetime, mode),
            Field::Oneof(oneof) => oneof.decode(ident, lifetime, mode),
            Field::Ignored(_) => panic!("field is ignored"),
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.encoded_len(ident),
            Field::Oneof(oneof) => oneof.encoded_len(ident),
            Field::Ignored(_) => panic!("field is ignored"),
        }
    }

    /// Returns an expression which initializes the field's type with its encoding with a guaranteed
    /// empty value.
    pub fn empty(&self) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.empty(),
            Field::Oneof(oneof) => oneof.empty(),
            Field::Ignored(ignored) => ignored.initialize(),
        }
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    pub fn is_empty(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.is_empty(ident),
            Field::Oneof(oneof) => oneof.is_empty(ident),
            Field::Ignored(_) => panic!("field is ignored"),
        }
    }

    /// Returns an expression which resets the field's value to empty with its encoding.
    pub fn clear(&self, ident: TokenStream) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.clear(ident),
            Field::Oneof(oneof) => oneof.clear(ident),
            Field::Ignored(_) => panic!("field is ignored"),
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

impl FieldBearer for Field {
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        match self {
            Field::Value(field) => field.where_terms(purpose),
            Field::Oneof(field) => field.where_terms(purpose),
            Field::Ignored(field) => field.where_terms(purpose),
        }
    }
}

impl FieldBearer for (TokenStream, Field) {
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        self.1.where_terms(purpose)
    }
}

/// Get the items belonging to the 'bilrost' list attribute, e.g. `#[bilrost(foo, bar="baz")]`.
pub fn bilrost_attrs(attrs: &[Attribute]) -> Result<Vec<Meta>, Error> {
    let mut result = Vec::new();
    for attr in attrs {
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
    T: Debug,
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
