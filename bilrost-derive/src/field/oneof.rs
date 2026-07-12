use crate::attrs::{set_option_with_display, tag_list_attr, TagList};
use crate::crate_name;
use crate::field::traits::{
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    Tagged,
    WhereFor::{self, Decode, Encode, Schema},
};
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Result};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{Meta, Type};

#[derive(Clone)]
pub struct OneofInclusion {
    pub ty: Type,
    pub tags: Vec<u32>,
}

impl OneofInclusion {
    pub fn new(ty: &Type, attrs: &[Meta]) -> Result<Option<Box<OneofInclusion>>> {
        let mut oneof_tags = None;
        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if let Some(tags) = tag_list_attr(attr, "oneof", Some(100))? {
                set_option_with_display(
                    &mut oneof_tags,
                    tags,
                    "duplicate oneof attribute",
                    TagList::display,
                )?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        let Some(tags) = oneof_tags else {
            return Ok(None); // Not a oneof field
        };

        if !unknown_attrs.is_empty() {
            bail!(
                "unknown attribute(s) for oneof field: {attrs}",
                attrs = quote!(#(#unknown_attrs),*),
            );
        }

        Ok(Some(Box::new(OneofInclusion {
            ty: ty.clone(),
            tags: tags.iter_tags().collect(),
        })))
    }

    pub fn ty(&self) -> &Type {
        &self.ty
    }

    /// Returns a statement which encodes the oneof field. `target` should be a reference to the
    /// field value.
    pub fn encode(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote! {
            #crate_::encoding::Oneof::oneof_encode(#target, buf, tw);
        }
    }

    /// Returns a statement which prepends the oneof field. `target` should be a reference to the
    /// field value.
    pub fn prepend(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote! {
            #crate_::encoding::Oneof::oneof_prepend(#target, buf, tw);
        }
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field. `target`
    /// should be a mutable reference to the field value.
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
        quote!(#crate_::encoding::#trait_name::#call(#target, tag, wire_type, buf, ctx))
    }

    /// Returns an expression which evaluates to the encoded length of the oneof field. `target`
    /// should be a reference to the field value.
    pub fn encoded_len(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        quote!(#crate_::encoding::Oneof::oneof_encoded_len(#target, tm))
    }

    /// Returns an expression which initializes the field's type as a guaranteed empty value with
    /// its encoding.
    pub fn empty(&self) -> TokenStream {
        let crate_ = crate_name();
        let ty = &self.ty;
        quote!(<#ty as #crate_::encoding::Oneof>::empty())
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    /// `target` should be a reference to the field value.
    pub fn is_empty(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let ty = &self.ty;
        quote!(
            <#ty as #crate_::encoding::Oneof>::is_empty(#target)
        )
    }

    /// Returns an expression which resets the field's value to empty with its encoding. `target`
    /// should be a reference to the field value.
    pub fn clear(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let ty = &self.ty;
        quote! {
            <#ty as #crate_::encoding::Oneof>::clear(#target);
        }
    }

    /// Returns an expression which evaluates to an Option<u32> of the tag of the (maybe) present
    /// field in the oneof. `target` should be a reference to the field value.
    pub fn current_tag(&self, target: impl ToTokens) -> TokenStream {
        let crate_ = crate_name();
        quote!(#crate_::encoding::Oneof::oneof_current_tag(#target))
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
            Schema => quote!(#ty: #crate_::encoding::schema::AddOneofFields),
        }]
    }

    pub fn schema(&self, oneof_name: &str) -> TokenStream {
        let crate_ = crate_name();
        let tags = &self.tags;
        let ty = &self.ty;
        quote! {
            fields.add_oneof(#oneof_name, &[#(#tags),*]);
            <#ty as #crate_::encoding::schema::AddOneofFields>::add_fields(schema, fields, Some(#oneof_name));
        }
    }
}

impl Tagged for OneofInclusion {
    fn tags(&self) -> Vec<u32> {
        self.tags.clone()
    }
}
