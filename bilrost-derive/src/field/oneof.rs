use alloc::vec;
use alloc::vec::Vec;

use eyre::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Meta, Type};

use crate::attrs::tag_list_attr;
use crate::field::{
    set_option,
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    WhereFor::{self, Decode, Encode},
};

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
    pub fn decode(
        &self,
        ident: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        match (lifetime, mode) {
            (Owned, Relaxed) => quote!(
                ::bilrost::encoding::OneofDecode::oneof_decode_field(
                    #ident,
                    tag,
                    wire_type,
                    buf,
                    ctx,
                )
            ),
            (Borrowed, Relaxed) => quote!(
                ::bilrost::encoding::OneofBorrowDecode::oneof_borrow_decode_field(
                    #ident,
                    tag,
                    wire_type,
                    buf,
                    ctx,
                )
            ),
            (Owned, Distinguished) => quote!(
                ::bilrost::encoding::DistinguishedOneofDecode::oneof_decode_field_distinguished(
                    #ident,
                    tag,
                    wire_type,
                    buf,
                    ctx.clone(),
                )
            ),
            (Borrowed, Distinguished) => quote!(
                ::bilrost::encoding::DistinguishedOneofBorrowDecode::
                    oneof_borrow_decode_field_distinguished
                (
                    #ident,
                    tag,
                    wire_type,
                    buf,
                    ctx.clone(),
                )
            ),
        }
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
    pub fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let ty = &self.ty;
        vec![match purpose {
            Encode => quote!(#ty: ::bilrost::encoding::Oneof),
            Decode(Owned, Relaxed) => {
                quote!(#ty: ::bilrost::encoding::OneofDecoder)
            }
            Decode(Borrowed, Relaxed) => {
                quote!(#ty: ::bilrost::encoding::OneofBorrowDecoder<'__a>)
            }
            Decode(Owned, Distinguished) => {
                quote!(#ty: ::bilrost::encoding::DistinguishedOneofDecoder)
            }
            Decode(Borrowed, Distinguished) => {
                quote!(#ty: ::bilrost::encoding::DistinguishedOneofBorrowDecoder<'__a>)
            }
        }]
    }
}
