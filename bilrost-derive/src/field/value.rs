use crate::attrs::{named_attr, tag_attr, word_attr};
use crate::crate_name;
use crate::field::{
    set_bool, set_option,
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    WhereFor::{self, Decode, Encode},
};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, Error};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_str, Index, Meta, Type};

/// A field in a bilrost message or oneof
#[derive(Clone)]
pub struct Field {
    pub tag: u32,
    pub ty: Type,
    pub encoding: Type,
    pub enumeration_ty: Option<Type>,
    /// If a field is part of a recursion of messages, currently the chain needs to be broken so
    /// that there is not a cyclic dependency of type constraints on the implementation of message
    /// traits. When a field is marked with the "recurses" attribute, it will not be checked in the
    /// `where` clause of the implementation, and the type must always be supported by its encoder.
    pub recurses: bool,
    /// When a value field is in a oneof, it must always encode a nonzero amount of data. The
    /// encoder must be a ValueEncoder to satisfy this; effectively, Oneof types are much like
    /// several fields whose values are each wrapped in an `Option`, but at most one of them can be
    /// `Some`.
    pub in_oneof: bool,
    /// When a value is a oneof enum's variant member and that variant is a struct, it has a field
    /// name that we have to use and accessing it is spelled differently.
    pub ident_within_variant: Option<Ident>,
}

impl Field {
    pub fn new(ty: &Type, attrs: &[Meta], inferred_tag: Option<u32>) -> Result<Box<Field>, Error> {
        Field::new_impl(ty, attrs, inferred_tag, false, None)
    }

    pub fn new_in_oneof(
        ty: &Type,
        ident_within_variant: Option<Ident>,
        attrs: &[Meta],
    ) -> Result<Box<Field>, Error> {
        Field::new_impl(ty, attrs, None, true, ident_within_variant)
    }

    fn new_impl(
        ty: &Type,
        attrs: &[Meta],
        inferred_tag: Option<u32>,
        in_oneof: bool,
        ident_within_variant: Option<Ident>,
    ) -> Result<Box<Field>, Error> {
        let mut tag = None;
        let mut encoding = None;
        let mut enumeration_ty = None;
        let mut recurses = false;
        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if let Some(t) = tag_attr(attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if let Some(t) = named_attr(attr, "encoding")? {
                set_option(&mut encoding, t, "duplicate encoding attributes")?;
            } else if let Some(t) = named_attr(attr, "enumeration")? {
                set_option(&mut enumeration_ty, t, "duplicate enumeration attributes")?;
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

        let tag = match tag.or(inferred_tag) {
            Some(tag) => tag,
            None => bail!("missing tag attribute"),
        };

        let encoding = encoding.unwrap_or(parse_str::<Type>(if in_oneof {
            "general_packed"
        } else {
            "general"
        })?);

        Ok(Box::new(Field {
            tag,
            ty: ty.clone(),
            encoding,
            enumeration_ty,
            recurses,
            in_oneof,
            ident_within_variant,
        }))
    }

    /// Spells a value for the field as an enum variant with the given value.
    pub fn with_value(&self, value: TokenStream) -> TokenStream {
        if !self.in_oneof {
            panic!(
                "trying to spell a field's value within a oneof variant, but the field is not part \
                of a oneof"
            );
        }
        match &self.ident_within_variant {
            None => quote!( (#value) ),
            Some(inner_ident) => quote!( { #inner_ident: #value } ),
        }
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`.
    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoder = &self.encoding;
        let ty = &self.ty;
        if self.in_oneof {
            quote! {
                <() as #crate_::encoding::FieldEncoder<#encoder, #ty>>::encode_field(
                    #tag,
                    &#ident,
                    buf,
                    tw,
                );
            }
        } else {
            quote! {
                <() as #crate_::encoding::Encoder<#encoder, #ty>>::encode(#tag, &#ident, buf, tw);
            }
        }
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`.
    pub fn prepend(&self, ident: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoder = &self.encoding;
        let ty = &self.ty;
        if self.in_oneof {
            quote! {
                <() as #crate_::encoding::FieldEncoder<#encoder, #ty>>::prepend_field(
                    #tag,
                    &#ident,
                    buf,
                    tw,
                );
            }
        } else {
            quote! {
                <() as #crate_::encoding::Encoder<#encoder, #ty>>::prepend_encode(
                    #tag,
                    &#ident,
                    buf,
                    tw,
                );
            }
        }
    }

    /// Returns an expression which evaluates to the result of merging a decoded value into the
    /// field. The given ident must be an &mut that already refers to the destination.
    pub fn decode(
        &self,
        ident: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.encoding;
        let ty = &self.ty;
        let (decoder_trait, call) = if self.in_oneof {
            match (lifetime, mode) {
                (Owned, Relaxed) => (quote!(FieldDecoder), quote!(decode_field)),
                (Borrowed, Relaxed) => (quote!(FieldBorrowDecoder), quote!(borrow_decode_field)),
                (Owned, Distinguished) => (
                    quote!(DistinguishedFieldDecoder),
                    quote!(decode_field_distinguished::<true>), // empty values are ok
                ),
                (Borrowed, Distinguished) => (
                    quote!(DistinguishedFieldBorrowDecoder),
                    quote!(borrow_decode_field_distinguished::<true>), // empty values are ok
                ),
            }
        } else {
            match (lifetime, mode) {
                (Owned, Relaxed) => (quote!(Decoder), quote!(decode)),
                (Borrowed, Relaxed) => (quote!(BorrowDecoder), quote!(borrow_decode)),
                (Owned, Distinguished) => {
                    (quote!(DistinguishedDecoder), quote!(decode_distinguished))
                }
                (Borrowed, Distinguished) => (
                    quote!(DistinguishedBorrowDecoder),
                    quote!(borrow_decode_distinguished),
                ),
            }
        };
        let decode = quote!(
            <() as #crate_::encoding::#decoder_trait<#encoding, #ty>>::#call(
                wire_type,
                #ident,
                buf,
                ctx,
            )
        );
        if self.in_oneof {
            decode
        } else {
            // When not in a oneof, we need to check the duplicated status of the field ourselves to
            // attach the right field name to the error while decoding.
            quote! {
                if duplicated {
                    ::core::result::Result::Err(#crate_::DecodeError::new(
                        #crate_::DecodeErrorKind::UnexpectedlyRepeated
                    ))
                } else {
                    #decode
                }
            }
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field. The given ident
    /// must be the location name of the field value, not a reference.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoder = &self.encoding;
        let ty = &self.ty;
        if self.in_oneof {
            quote! {
                <() as #crate_::encoding::FieldEncoder<#encoder, #ty>>::field_encoded_len(
                    #tag,
                    &#ident,
                    tm,
                )
            }
        } else {
            quote! {
                <() as #crate_::encoding::Encoder<#encoder, #ty>>::encoded_len(#tag, &#ident, tm)
            }
        }
    }

    /// Returns an expression which initializes the field's type with its encoding.
    pub fn for_overwrite(&self) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.encoding;
        let ty = &self.ty;
        quote!(<() as #crate_::encoding::ForOverwrite<#encoding, #ty>>::for_overwrite())
    }

    /// Returns an expression which initializes the field's type as a guaranteed empty value with
    /// its encoding.
    pub fn empty(&self) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.encoding;
        let ty = &self.ty;
        quote!(<() as #crate_::encoding::EmptyState<#encoding, #ty>>::empty())
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    pub fn is_empty(&self, ident: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.encoding;
        let ty = &self.ty;
        quote!(<() as #crate_::encoding::EmptyState<#encoding, #ty>>::is_empty(#ident))
    }

    /// Returns an expression which resets the field's value to empty with its encoding.
    pub fn clear(&self, ident: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.encoding;
        let ty = &self.ty;
        quote! {
            <() as #crate_::encoding::EmptyState<#encoding, #ty>>::clear(#ident);
        }
    }

    /// Returns the where clause constraint terms for the field's encoder.
    pub fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let crate_ = crate_name();
        if self.recurses {
            return vec![];
        }
        let ty = &self.ty;
        let encoding = &self.encoding;
        if self.in_oneof {
            vec![
                match purpose {
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
                },
                // Encoding or decoding a oneof field always has trivially externally determined
                // presence, and we never need to know whether or not the value is empty; it never
                // needs to implement the empty state.
                quote!((): #crate_::encoding::ForOverwrite<#encoding, #ty>),
            ]
        } else {
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

        let field_ty = &self.ty;

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
