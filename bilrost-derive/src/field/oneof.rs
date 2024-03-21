use alloc::vec;
use alloc::vec::Vec;

use anyhow::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Meta, Type};

use crate::attrs::tag_list_attr;
use crate::field::set_option;

#[derive(Clone)]
pub struct Field {
    pub ty: Type,
    pub tags: Vec<u32>,
}

impl Field {
    pub fn new(ty: &Type, attrs: &[Meta]) -> Result<Option<Field>, Error> {
        let mut oneof_tags = None;
        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if let Some(tags) = tag_list_attr("oneof", Some(100), attr)? {
                set_option(&mut oneof_tags, tags, "duplicate oneof attribute")?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        let Some(tags) = oneof_tags else {
            return Ok(None); // Not a oneof field
        };

        if !unknown_attrs.is_empty() {
            bail!(
                "unknown attribute(s) for oneof field: {}",
                quote!(#(#unknown_attrs),*)
            );
        }

        Ok(Some(Field {
            ty: ty.clone(),
            tags: tags.iter_tags().collect(),
        }))
    }

    /// Returns a statement which encodes the oneof field.
    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        quote! {
            ::bilrost::encoding::Oneof::oneof_encode(&#ident, buf, tw);
        }
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode_expedient(&self, ident: TokenStream) -> TokenStream {
        quote!(
            ::bilrost::encoding::Oneof::oneof_decode_field(
                #ident,
                tag,
                wire_type,
                duplicated,
                buf,
                ctx,
            )
        )
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode_distinguished(&self, ident: TokenStream) -> TokenStream {
        quote!(
            ::bilrost::encoding::DistinguishedOneof::oneof_decode_field_distinguished(
                #ident,
                tag,
                wire_type,
                duplicated,
                buf,
                ctx,
            )
        )
    }

    /// Returns an expression which evaluates to the encoded length of the oneof field.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        quote!(::bilrost::encoding::Oneof::oneof_encoded_len(&#ident, tm))
    }

    /// Returns an expression which evaluates to an Option<u32> of the tag of the (maybe) present
    /// field in the oneof.
    pub fn current_tag(&self, ident: TokenStream) -> TokenStream {
        quote!(::bilrost::encoding::Oneof::oneof_current_tag(&#ident))
    }

    /// Returns the where clause constraint term for the field really implementing the oneof trait.
    pub fn expedient_where_terms(&self) -> Vec<TokenStream> {
        let ty = &self.ty;
        vec![quote!(#ty: ::bilrost::encoding::Oneof)]
    }

    /// Returns the where clause constraint term for the field really implementing the oneof trait.
    pub fn distinguished_where_terms(&self) -> Vec<TokenStream> {
        let ty = &self.ty;
        vec![quote!(#ty: ::bilrost::encoding::DistinguishedOneof)]
    }
}
