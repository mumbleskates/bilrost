use crate::attrs::{set_option_with_display, tag_list_attr, TagList};
use crate::field::traits::{
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    Tagged,
    WhereFor::{self, Decode, Encode, Schema},
};
use crate::Context;
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{format, vec};
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
                    "duplicate oneof attributes",
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

    /// Returns a statement which encodes the oneof field. `target` should be a reference to the
    /// field value.
    pub fn encode(&self, target: TokenStream, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        quote! {
            #crate_::encoding::Oneof::oneof_encode(#target, buf, tw);
        }
    }

    /// Returns a statement which prepends the oneof field. `target` should be a reference to the
    /// field value.
    pub fn prepend(&self, target: TokenStream, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
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
        ctx: &Context,
    ) -> TokenStream {
        let crate_ = &ctx.crate_name;
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
    pub fn encoded_len(&self, target: TokenStream, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        quote!(#crate_::encoding::Oneof::oneof_encoded_len(#target, tm))
    }

    /// Returns an expression which initializes the field's type as a guaranteed empty value with
    /// its encoding.
    pub fn empty(&self, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        let ty = &self.ty;
        quote!(<#ty as #crate_::encoding::Oneof>::empty())
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    /// `target` should be a reference to the field value.
    pub fn is_empty(&self, target: TokenStream, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        let ty = &self.ty;
        quote!(
            <#ty as #crate_::encoding::Oneof>::is_empty(#target)
        )
    }

    /// Returns an expression which resets the field's value to empty with its encoding. `target`
    /// should be a reference to the field value.
    pub fn clear(&self, target: TokenStream, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        let ty = &self.ty;
        quote! {
            <#ty as #crate_::encoding::Oneof>::clear(#target);
        }
    }

    /// Returns an expression which evaluates to an Option<u32> of the tag of the (maybe) present
    /// field in the oneof. `target` should be a reference to the field value.
    pub fn current_tag(&self, target: impl ToTokens, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        quote!(#crate_::encoding::Oneof::oneof_current_tag(#target))
    }

    /// Returns the where clause constraint term for the field really implementing the oneof trait.
    pub fn where_terms(&self, purpose: WhereFor, ctx: &Context) -> Vec<TokenStream> {
        let crate_ = &ctx.crate_name;
        let ty = &self.ty;
        match purpose {
            Encode => vec![quote!(#ty: #crate_::encoding::Oneof)],
            Decode(Owned, Relaxed) => {
                vec![quote!(#ty: #crate_::encoding::OneofDecoder)]
            }
            Decode(Borrowed, Relaxed) => {
                vec![quote!(#ty: #crate_::encoding::OneofBorrowDecoder<'__a>)]
            }
            Decode(Owned, Distinguished) => {
                vec![quote!(#ty: #crate_::encoding::DistinguishedOneofDecoder)]
            }
            Decode(Borrowed, Distinguished) => {
                vec![quote!(#ty: #crate_::encoding::DistinguishedOneofBorrowDecoder<'__a>)]
            }
            Schema => vec![
                quote!(#ty: #crate_::encoding::schema::AddOneofFields),
                quote!(#ty: #crate_::encoding::Oneof),
            ],
        }
    }

    pub fn schema(
        &self,
        field_ident: &TokenStream,
        oneof_field_schema_name: &str,
        ctx: &Context,
    ) -> TokenStream {
        let crate_ = &ctx.crate_name;
        let tags = &self.tags;
        let ty = &self.ty;
        let description = format!(
            "tags don't match for oneof field {field_ident} with type {oneof_ty_name}",
            oneof_ty_name = ty.to_token_stream(),
        );
        quote! {
            const _: () = #crate_::assert_tags_are_equal(
                #description,
                <#ty as #crate_::encoding::Oneof>::FIELD_TAGS,
                &[#(#tags),*],
            );
            fields.add_oneof(#oneof_field_schema_name, &[#(#tags),*]);
            <#ty as #crate_::encoding::schema::AddOneofFields>::add_fields(
                schema,
                fields,
                Some(#oneof_field_schema_name),
            );
        }
    }

    pub fn init_heap(&self, ctx: &Context) -> TokenStream {
        let crate_ = &ctx.crate_name;
        let ty = &self.ty;
        quote!(<#ty as #crate_::encoding::Oneof>::INIT_HEAP)
    }
}

impl Tagged for OneofInclusion {
    fn tags(&self) -> &[u32] {
        &self.tags
    }
}
