use alloc::vec;
use alloc::vec::Vec;

use eyre::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Meta, Type};

use crate::attrs::tag_list_attr;
use crate::field::{set_option, WhereFor};

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
            if let Some(tags) = tag_list_attr(attr, "oneof", Some(100))? {
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

    /// Returns a statement which prepends the oneof field.
    pub fn prepend(&self, ident: TokenStream) -> TokenStream {
        quote! {
            ::bilrost::encoding::Oneof::oneof_prepend(&#ident, buf, tw);
        }
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode_expedient(&self, ident: TokenStream) -> TokenStream {
        quote!(
            ::bilrost::encoding::OneofDecode::oneof_decode_field(
                #ident,
                tag,
                wire_type,
                buf,
                ctx,
            )
        )
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode_distinguished(&self, ident: TokenStream) -> TokenStream {
        quote!(
            ::bilrost::encoding::DistinguishedOneofDecode::oneof_decode_field_distinguished(
                #ident,
                tag,
                wire_type,
                buf,
                ctx.clone(),
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
    pub fn expedient_where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let ty = &self.ty;
        match purpose {
            WhereFor::Encode => vec![quote!(#ty: ::bilrost::encoding::Oneof)],
            WhereFor::DecodeOwned => vec![quote!(#ty: ::bilrost::encoding::OneofDecode)],
            WhereFor::DecodeBorrowed => vec![quote!(#ty: ::bilrost::encoding::OneofBorrowDecode)],
        }
    }

    /// Returns the where clause constraint term for the field really implementing the oneof trait.
    pub fn distinguished_where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let ty = &self.ty;
        match purpose {
            WhereFor::Encode => vec![quote!(#ty: ::bilrost::encoding::DistinguishedOneof)],
            WhereFor::DecodeOwned => {
                vec![quote!(#ty: ::bilrost::encoding::DistinguishedOneofDecode)]
            }
            WhereFor::DecodeBorrowed => {
                vec![quote!(#ty: ::bilrost::encoding::DistinguishedOneofBorrowDecode)]
            }
        }
    }
}
