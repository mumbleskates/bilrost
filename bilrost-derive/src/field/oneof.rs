use crate::attrs::tag_list_attr;
use crate::crate_name;
use crate::field::set_option;
use crate::field::traits::{
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    WhereFor::{self, Decode, Encode},
};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Meta, Type};

#[derive(Clone)]
pub struct OneofInclusion {
    pub ty: Type,
    pub tags: Vec<u32>,
}

impl OneofInclusion {
    pub fn new(ty: &Type, attrs: &[Meta]) -> Result<Option<Box<OneofInclusion>>, Error> {
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

        Ok(Some(Box::new(OneofInclusion {
            ty: ty.clone(),
            tags: tags.iter_tags().collect(),
        })))
    }

    /// Returns a statement which encodes the oneof field.
    pub fn encode(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote! {
            #crate_::encoding::Oneof::oneof_encode(&#target, buf, tw);
        }
    }

    /// Returns a statement which prepends the oneof field.
    pub fn prepend(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote! {
            #crate_::encoding::Oneof::oneof_prepend(&#target, buf, tw);
        }
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode(
        &self,
        target: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let crate_ = crate_name();
        let (trait_name, call) = match (lifetime, mode) {
            (Owned, Relaxed) => (quote!(OneofDecoder), quote!(oneof_decode_field)),
            (Borrowed, Relaxed) => (
                quote!(OneofBorrowDecoder),
                quote!(oneof_borrow_decode_field),
            ),
            (Owned, Distinguished) => (
                quote!(DistinguishedOneofDecoder),
                quote!(oneof_decode_field_distinguished),
            ),
            (Borrowed, Distinguished) => (
                quote!(DistinguishedOneofBorrowDecoder),
                quote!(oneof_borrow_decode_field_distinguished),
            ),
        };
        quote!(#crate_::encoding::#trait_name::#call(&mut #target, tag, wire_type, buf, ctx))
    }

    /// Returns an expression which evaluates to the encoded length of the oneof field.
    pub fn encoded_len(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote!(#crate_::encoding::Oneof::oneof_encoded_len(&#target, tm))
    }

    /// Returns an expression which initializes the field's type as a guaranteed empty value with
    /// its encoding.
    pub fn empty(&self) -> TokenStream {
        let crate_ = crate_name();
        let ty = &self.ty;
        quote!(<#ty as #crate_::encoding::Oneof>::empty())
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    pub fn is_empty(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let ty = &self.ty;
        quote!(
            <#ty as #crate_::encoding::Oneof>::is_empty(&#target)
        )
    }

    /// Returns an expression which resets the field's value to empty with its encoding.
    pub fn clear(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let ty = &self.ty;
        quote! {
            <#ty as #crate_::encoding::Oneof>::clear(&mut #target);
        }
    }

    /// Returns an expression which evaluates to an Option<u32> of the tag of the (maybe) present
    /// field in the oneof.
    pub fn current_tag(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote!(#crate_::encoding::Oneof::oneof_current_tag(&#target))
    }

    /// Returns the where clause constraint term for the field really implementing the oneof trait.
    pub fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let crate_ = crate_name();
        let ty = &self.ty;
        vec![match purpose {
            Encode => quote!(#ty: #crate_::encoding::Oneof),
            Decode(Owned, Relaxed) => {
                quote!(#ty: #crate_::encoding::OneofDecoder)
            }
            Decode(Borrowed, Relaxed) => {
                quote!(#ty: #crate_::encoding::OneofBorrowDecoder<'__a>)
            }
            Decode(Owned, Distinguished) => {
                quote!(#ty: #crate_::encoding::DistinguishedOneofDecoder)
            }
            Decode(Borrowed, Distinguished) => {
                quote!(#ty: #crate_::encoding::DistinguishedOneofBorrowDecoder<'__a>)
            }
        }]
    }
}
