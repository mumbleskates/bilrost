use crate::attrs::{named_attr, tag_attr, word_attr};
use crate::crate_name;
use crate::field::traits::{
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    FieldBearer,
    WhereFor::{self, Decode, Encode},
};
use crate::field::{bilrost_attrs, set_bool, set_option};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::slice;
use eyre::{bail, Error};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{parse_str, Fields, Ident, Index, Meta, Type, Variant};

/// A field in a bilrost message or oneof
// TODO: message fields should know what the field's ident is
#[derive(Clone)]
pub struct MessageField {
    pub tag: u32,
    pub value: ValueField,
    /// In structs, we can create inherent implementations with helper methods for getting and
    /// setting enumeration values through translated types. This is only possible on messages, and
    /// even when "variant types" are available those types are unlikely to be allowed to have impls
    /// of their own.
    pub enumeration_ty: Option<Type>,
}

#[derive(Clone)]
pub struct ValueField {
    pub ty: Type,
    pub encoding: Type,
    /// If a field is part of a recursion of messages, currently the chain needs to be broken so
    /// that there is not a cyclic dependency of type constraints on the implementation of message
    /// traits. When a field is marked with the "recurses" attribute, it will not be checked in the
    /// `where` clause of the implementation, and the type must always be supported by its encoding.
    pub recurses: bool,
}

#[derive(Clone)]
pub struct OneofVariant {
    pub tag: u32,
    pub variant_ident: Ident,
    pub contents: VariantContents,
}

#[derive(Clone)]
pub enum VariantContents {
    Value(Box<FieldInVariant>),
    Message(Vec<FieldInVariant>),
}

#[derive(Clone)]
pub struct FieldInVariant {
    pub ident_within_variant: Option<Ident>,
    pub value: ValueField,
}

impl MessageField {
    pub fn new(
        ty: &Type,
        attrs: Vec<Meta>,
        inferred_tag: Option<u32>,
    ) -> Result<Box<MessageField>, Error> {
        let mut tag = None;
        let mut enumeration_ty = None;
        let mut remaining_attrs = vec![];

        for attr in attrs {
            if let Some(t) = tag_attr(&attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if let Some(t) = named_attr(&attr, "enumeration")? {
                set_option(&mut enumeration_ty, t, "duplicate enumeration attributes")?;
            } else {
                remaining_attrs.push(attr);
            }
        }

        let value = ValueField::new(ty, remaining_attrs, "general")?;

        let tag = match tag.or(inferred_tag) {
            Some(tag) => tag,
            None => bail!("missing tag attribute"),
        };

        Ok(Box::new(MessageField {
            tag,
            value,
            enumeration_ty,
        }))
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`.
    pub fn encode(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::Encoder<#encoding, #ty>>::encode(#tag, &#target, buf, tw);
        }
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`.
    pub fn prepend(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::Encoder<#encoding, #ty>>::prepend_encode(
                #tag,
                &#target,
                buf,
                tw,
            );
        }
    }

    /// Returns an expression which evaluates to the result of merging a decoded value into the
    /// field. The given ident must be an &mut that already refers to the destination.
    pub fn decode(
        &self,
        target: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        let (decoder_trait, call) = match (lifetime, mode) {
            (Owned, Relaxed) => (quote!(Decoder), quote!(decode)),
            (Borrowed, Relaxed) => (quote!(BorrowDecoder), quote!(borrow_decode)),
            (Owned, Distinguished) => (quote!(DistinguishedDecoder), quote!(decode_distinguished)),
            (Borrowed, Distinguished) => (
                quote!(DistinguishedBorrowDecoder),
                quote!(borrow_decode_distinguished),
            ),
        };
        // We need to check the duplicated status of the field ourselves to attach the right field
        // name to the error while decoding.
        quote! {
            if duplicated {
                ::core::result::Result::Err(#crate_::DecodeError::new(
                    #crate_::DecodeErrorKind::UnexpectedlyRepeated
                ))
            } else {
                <() as #crate_::encoding::#decoder_trait<#encoding, #ty>>::#call(
                    wire_type,
                    &mut #target,
                    buf,
                    ctx,
                )
            }
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field. The given ident
    /// must be the location name of the field value, not a reference.
    pub fn encoded_len(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::Encoder<#encoding, #ty>>::encoded_len(#tag, &#target, tm)
        }
    }

    /// Returns an expression which initializes the field's type as a guaranteed empty value with
    /// its encoding.
    pub fn empty(&self) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote!(<() as #crate_::encoding::EmptyState<#encoding, #ty>>::empty())
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    pub fn is_empty(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote!(<() as #crate_::encoding::EmptyState<#encoding, #ty>>::is_empty(&#target))
    }

    /// Returns an expression which resets the field's value to empty with its encoding.
    pub fn clear(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::EmptyState<#encoding, #ty>>::clear(&mut #target);
        }
    }

    /// Returns the where clause constraint terms for the field's encoder.
    pub fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let crate_ = crate_name();
        if self.value.recurses {
            return vec![];
        }
        let ty = &self.value.ty;
        let encoding = &self.value.encoding;
        vec![
            match purpose {
                Encode => quote!((): #crate_::encoding::Encoder<#encoding, #ty>),
                Decode(Owned, Relaxed) => {
                    quote!((): #crate_::encoding::Decoder<#encoding, #ty>)
                }
                Decode(Borrowed, Relaxed) => {
                    quote!((): #crate_::encoding::BorrowDecoder<'__a, #encoding, #ty>)
                }
                Decode(Owned, Distinguished) => {
                    quote!((): #crate_::encoding::DistinguishedDecoder<#encoding, #ty>)
                }
                Decode(Borrowed, Distinguished) => {
                    quote!(
                        (): #crate_::encoding::DistinguishedBorrowDecoder<'__a, #encoding, #ty>
                    )
                }
            },
            // Message field encoding always requires EmptyState instead of just ForOverwrite
            // because we need to know whether a field is empty to know whether we should write
            // anything; and all the decoding traits imply the encoding trait.
            quote!((): #crate_::encoding::EmptyState<#encoding, #ty>),
        ]
    }

    /// Returns methods to embed in the message. `ident` must be the name of the field within the
    /// message struct.
    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
        let crate_ = crate_name();
        let enumeration_ty = self.enumeration_ty.as_ref()?;

        let ident_str = ident.to_string();
        let ident_str = ident_str.as_str().strip_prefix("r#").unwrap_or(&ident_str);

        // Prepend `get_` for getter methods of tuple structs.
        let get = match parse_str::<Index>(ident_str) {
            Ok(index) => {
                let get = Ident::new(&format!("get_{}", index.index), Span::call_site());
                quote!(#get)
            }
            Err(_) => quote!(#ident),
        };

        let set = Ident::new(&format!("set_{}", ident_str), Span::call_site());

        let field_ty = &self.value.ty;

        Some(quote! {
            fn #get(
                &self
            ) -> <#enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>>::Output {
                <
                    #enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>
                >::help_get(self.#ident)
            }

            fn #set(
                &mut self,
                val: <#enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>>::Input,
            ) {
                self.#ident = <
                    #enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>
                >::help_set(val);
            }
        })
    }
}

impl ValueField {
    fn new(
        ty: &Type,
        attrs: Vec<Meta>,
        implicit_default_encoding: &str,
    ) -> Result<ValueField, Error> {
        let mut encoding = None;
        let mut recurses = false;
        let mut unknown_attrs = Vec::new();

        for attr in &attrs {
            if let Some(t) = named_attr(attr, "encoding")? {
                set_option(&mut encoding, t, "duplicate encoding attributes")?;
            } else if word_attr(attr, "recurses") {
                set_bool(&mut recurses, "duplicate recurses attributes")?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        if !unknown_attrs.is_empty() {
            bail!(
                "unknown attribute(s) for field: {}",
                quote!(#(#unknown_attrs),*)
            )
        }

        let encoding =
            encoding.unwrap_or_else(|| parse_str::<Type>(implicit_default_encoding).unwrap());

        Ok(ValueField {
            ty: ty.clone(),
            encoding,
            recurses,
        })
    }
}

impl OneofVariant {
    /// Parses values specifically for within a Oneof variant, which works differently than fields
    /// within a Message.
    ///
    /// Returns `Ok` for data variants, and `Err` with just the ident for an empty variant.
    pub fn new(variant: Variant) -> Result<Option<OneofVariant>, Error> {
        let mut tag = None; // tag number
        let mut message = false; // whether this variant is marked as a "message" variant
        let mut empty = false; // whether this unit is marked as an "empty" variant
        let mut other_attrs = vec![];
        let our_attrs = bilrost_attrs(&variant.attrs)?;

        for attr in our_attrs {
            if let Some(t) = tag_attr(&attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if word_attr(&attr, "message") {
                set_bool(&mut message, "duplicate message attributes")?;
            } else if word_attr(&attr, "empty") {
                set_bool(&mut empty, "duplicate empty attributes")?;
            } else {
                other_attrs.push(attr);
            }
        }

        match (tag, message, empty) {
            // Implicitly or explicitly empty variant
            (None, false, _) if other_attrs.is_empty() => {
                if match variant.fields {
                    Fields::Named(fields) => fields.named.is_empty(),
                    Fields::Unnamed(fields) => fields.unnamed.is_empty(),
                    Fields::Unit => true,
                } {
                    Ok(None)
                } else {
                    // Return a error message depending on whether the variant is explicitly marked
                    // empty
                    if empty {
                        bail!(
                            "Oneof variant {} is marked 'empty' but it has fields",
                            variant.ident
                        );
                    } else {
                        bail!("missing tag attribute on variant {}", variant.ident);
                    }
                }
            }

            // Empty attribute plus any other attribute
            (_, _, true) => {
                bail!(
                    "the 'empty' attribute is combined with other attributes on variant {}, but it \
                    must always be alone",
                    variant.ident
                );
            }

            // Valid message variant
            (Some(_), true, false) if other_attrs.is_empty() => {
                Ok(None) // TODO: this
            }

            // Invalid message variant
            (_, true, _) => {
                // TODO: maybe better pattern(s) here
                bail!("invalid attributes for message variant"); // TODO: better error
            }

            // Normal value variant, neither empty nor message
            (_, false, false) => {
                let Some(tag) = tag else {
                    bail!("missing tag attribute on variant {}", variant.ident);
                };

                let fields = match &variant.fields {
                    Fields::Named(fields) => &fields.named,
                    Fields::Unnamed(fields) => &fields.unnamed,
                    Fields::Unit => bail!(
                        "Oneof value variants must have exactly one field, but variant {} has no \
                        fields",
                        variant.ident
                    ),
                };
                if fields.len() != 1 {
                    bail!(
                        "Oneof value variants must have exactly one field, but variant {} has {} \
                        fields",
                        variant.ident,
                        fields.len()
                    );
                }
                let field = fields.first().unwrap();
                Ok(Some(OneofVariant {
                    tag,
                    variant_ident: variant.ident.clone(),
                    contents: VariantContents::Value(Box::new(FieldInVariant {
                        value: ValueField::new(&field.ty, other_attrs, "general_packed")?,
                        ident_within_variant: field.ident.clone(),
                    })),
                }))
            }
        }
    }

    pub fn encode(&self, type_ident: &Ident) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let value_in_ident = match &field.ident_within_variant {
                    None => quote!((value)),
                    Some(inner_ident) => quote!( { #inner_ident: value } ),
                };
                quote! {
                    #type_ident::#variant_ident #value_in_ident => {
                        <() as #crate_::encoding::FieldEncoder<#encoding, #ty>>::encode_field(
                            #tag,
                            &value,
                            buf,
                            tw,
                        );
                    }
                }
            }
            VariantContents::Message(..) => todo!(),
        }
    }

    pub fn prepend(&self, type_ident: &Ident) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let value_in_ident = match &field.ident_within_variant {
                    None => quote!((value)),
                    Some(inner_ident) => quote!( { #inner_ident: value } ),
                };
                quote! {
                    #type_ident::#variant_ident #value_in_ident => {
                        <() as #crate_::encoding::FieldEncoder<#encoding, #ty>>::prepend_field(
                            #tag,
                            &value,
                            buf,
                            tw,
                        );
                    }
                }
            }
            VariantContents::Message(..) => todo!(),
        }
    }

    pub fn encoded_len(&self, type_ident: &Ident) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let value_in_ident = match &field.ident_within_variant {
                    None => quote!((value)),
                    Some(inner_field_ident) => quote!( { #inner_field_ident: value } ),
                };
                quote! {
                    #type_ident::#variant_ident #value_in_ident => {
                        <() as #crate_::encoding::FieldEncoder<#encoding, #ty>>::field_encoded_len(
                            #tag,
                            &value,
                            tm,
                        )
                    }
                }
            }
            VariantContents::Message(..) => todo!(),
        }
    }

    pub fn for_overwrite(&self) -> TokenStream {
        let crate_ = crate_name();
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                quote! {
                    let mut value =
                        <() as #crate_::encoding::ForOverwrite<#encoding, #ty>>::for_overwrite();
                }
            }
            VariantContents::Message(..) => todo!(),
        }
    }

    pub fn decode(&self, lifetime: DecodeLifetime, mode: DecodeMode) -> TokenStream {
        let crate_ = crate_name();
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let (decoder_trait, call) = match (lifetime, mode) {
                    (Owned, Relaxed) => (quote!(FieldDecoder), quote!(decode_field)),
                    (Borrowed, Relaxed) => {
                        (quote!(FieldBorrowDecoder), quote!(borrow_decode_field))
                    }
                    (Owned, Distinguished) => (
                        quote!(DistinguishedFieldDecoder),
                        quote!(decode_field_distinguished::<true>), // empty values are ok
                    ),
                    (Borrowed, Distinguished) => (
                        quote!(DistinguishedFieldBorrowDecoder),
                        quote!(borrow_decode_field_distinguished::<true>), // empty values are ok
                    ),
                };
                quote!(
                    <() as #crate_::encoding::#decoder_trait<#encoding, #ty>>::#call(
                        wire_type,
                        &mut value,
                        buf,
                        ctx,
                    )
                )
            }
            VariantContents::Message(..) => todo!(),
        }
    }

    pub fn construct(&self, type_ident: &Ident) -> TokenStream {
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let value_in_ident = match &field.ident_within_variant {
                    None => quote!((value)),
                    Some(inner_ident) => quote!( { #inner_ident: value } ),
                };
                quote!( #type_ident::#variant_ident #value_in_ident )
            }
            VariantContents::Message(..) => todo!(),
        }
    }
}

impl FieldBearer for OneofVariant {
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let crate_ = crate_name();
        let fields: &[FieldInVariant] = match &self.contents {
            VariantContents::Value(field) => slice::from_ref(field),
            VariantContents::Message(fields) => fields.as_slice(),
        };
        let mut res = vec![];
        for field in fields {
            if field.value.recurses {
                continue; // don't generate constraints for this (sub-)field
            }
            let ty = &field.value.ty;
            let encoding = &field.value.encoding;
            res.push(match purpose {
                Encode => quote!((): #crate_::encoding::ValueEncoder<#encoding, #ty>),
                Decode(Owned, Relaxed) => {
                    quote!((): #crate_::encoding::ValueDecoder<#encoding, #ty>)
                }
                Decode(Borrowed, Relaxed) => {
                    quote!((): #crate_::encoding::ValueBorrowDecoder<'__a, #encoding, #ty>)
                }
                Decode(Owned, Distinguished) => {
                    quote!((): #crate_::encoding::DistinguishedValueDecoder<#encoding, #ty>)
                }
                Decode(Borrowed, Distinguished) => {
                    quote!(
                        (): #crate_::encoding::
                            DistinguishedValueBorrowDecoder<'__a, #encoding, #ty>
                    )
                }
            });
            // Encoding or decoding a oneof field always has trivially externally determined
            // presence, and we never need to know whether or not the value is empty; it never
            // needs to implement the empty state.
            res.push(quote!((): #crate_::encoding::ForOverwrite<#encoding, #ty>));
        }
        res
    }
}
