use crate::crate_name;
use alloc::fmt::Debug;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Error};
use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use syn::punctuated::Punctuated;
use syn::{parse2, Attribute, LitInt, Meta, Token, Type};

mod ignored;
mod oneof;
mod value;

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Field {
    /// A scalar field.
    Value(value::Field),
    /// A oneof field.
    Oneof(oneof::Field),
    /// An ignored field.
    Ignored(ignored::Field),
}

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

impl Field {
    /// Creates a new `Field` from an iterator of field attributes.
    ///
    /// If the meta items are invalid, an error will be returned.
    /// If the field should be ignored, `None` is returned.
    pub fn new(ty: Type, attrs: Vec<Attribute>, inferred_tag: Option<u32>) -> Result<Field, Error> {
        let attrs = bilrost_attrs(attrs)?;

        Ok(if let Some(field) = ignored::Field::new(&ty, &attrs)? {
            Field::Ignored(field)
        } else if let Some(field) = oneof::Field::new(&ty, &attrs)? {
            Field::Oneof(field)
        } else {
            Field::Value(value::Field::new(&ty, &attrs, inferred_tag)?)
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

    /// Returns the where clause condition asserting that this field has the given capability.
    pub fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        match self {
            Field::Value(field) => field.where_terms(purpose),
            Field::Oneof(field) => field.where_terms(purpose),
            Field::Ignored(field) => field.where_terms(),
        }
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
            Field::Ignored(_) => {
                panic!("field is ignored");
            }
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

    /// Returns an expression which initializes the field's type with its encoding.
    pub fn for_overwrite(&self) -> TokenStream {
        match self {
            Field::Value(scalar) => scalar.for_overwrite(),
            Field::Oneof(oneof) => oneof.for_overwrite(),
            Field::Ignored(ignored) => ignored.initialize(),
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
