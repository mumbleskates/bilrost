use anyhow::{bail, Error};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{parse_str, Index, Meta, Type};

use super::{named_attr, set_bool, set_option, tag_attr, word_attr};

/// A scalar protobuf field.
#[derive(Clone)]
pub struct Field {
    pub tag: u32,
    pub ty: Type,
    pub encoder: Type,
    pub enumeration_ty: Option<Type>,
    /// If a field is part of a recursion of messages, currently the chain needs to be broken so
    /// that there is not a cyclic dependency of type constraints on the implementation of `Message`
    /// or `DistinguishedMessage`. When a field is marked with the "recurses" attribute, it will not
    /// be checked in the `where` clause of the implementation, and the type must always be
    /// supported by its encoder.
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
    pub fn new(ty: &Type, attrs: &[Meta], inferred_tag: Option<u32>) -> Result<Field, Error> {
        Field::new_impl(ty, attrs, inferred_tag, false, None)
    }

    pub fn new_in_oneof(
        ty: &Type,
        ident_within_variant: Option<Ident>,
        attrs: &[Meta],
    ) -> Result<Field, Error> {
        Field::new_impl(ty, attrs, None, true, ident_within_variant)
    }

    fn new_impl(
        ty: &Type,
        attrs: &[Meta],
        inferred_tag: Option<u32>,
        in_oneof: bool,
        ident_within_variant: Option<Ident>,
    ) -> Result<Field, Error> {
        let mut tag = None;
        let mut encoder = None;
        let mut enumeration_ty = None;
        let mut recurses = false;
        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if let Some(t) = tag_attr(attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if let Some(t) = named_attr(attr, "encoder")? {
                set_option(&mut encoder, t, "duplicate encoder attributes")?;
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

        let encoder = encoder.unwrap_or(parse_str::<Type>("general")?);

        Ok(Field {
            tag,
            ty: ty.clone(),
            encoder,
            enumeration_ty,
            recurses,
            in_oneof,
            ident_within_variant,
        })
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
        let tag = self.tag;
        let encoder = &self.encoder;
        if self.in_oneof {
            quote! {
                <#encoder as ::bilrost::encoding::FieldEncoder<_>>::encode_field(
                    #tag,
                    &#ident,
                    buf,
                    tw,
                );
            }
        } else {
            quote! {
                <#encoder as ::bilrost::encoding::Encoder<_>>::encode(#tag, &#ident, buf, tw);
            }
        }
    }

    /// Returns an expression which evaluates to the result of merging a decoded value into the
    /// field. The given ident must be an &mut that already refers to the destination.
    pub fn decode(&self, ident: TokenStream) -> TokenStream {
        let encoder = &self.encoder;
        if self.in_oneof {
            quote!(
                <#encoder as ::bilrost::encoding::FieldEncoder<_>>::decode_field(
                    wire_type,
                    #ident,
                    buf,
                    ctx,
                )
            )
        } else {
            quote!(
                <#encoder as ::bilrost::encoding::Encoder<_>>::decode(
                    wire_type,
                    duplicated,
                    #ident,
                    buf,
                    ctx,
                )
            )
        }
    }

    /// Returns an expression which evaluates to the result of decoding a value into the field in
    /// distinguished mode. The given ident must be an &mut that already refers to the destination.
    pub fn decode_distinguished(&self, ident: TokenStream) -> TokenStream {
        let encoder = &self.encoder;
        if self.in_oneof {
            quote!(
                <
                    #encoder as ::bilrost::encoding::DistinguishedFieldEncoder<_>
                >::decode_field_distinguished(
                    wire_type,
                    #ident,
                    buf,
                    ctx,
                )
            )
        } else {
            quote!(
                <#encoder as ::bilrost::encoding::DistinguishedEncoder<_>>::decode_distinguished(
                    wire_type,
                    duplicated,
                    #ident,
                    buf,
                    ctx,
                )
            )
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field. The given ident
    /// must be the location name of the field value, not a reference.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        let tag = self.tag;
        let encoder = &self.encoder;
        if self.in_oneof {
            quote! {
                <#encoder as ::bilrost::encoding::FieldEncoder<_>>::field_encoded_len(
                    #tag,
                    &#ident,
                    tm,
                )
            }
        } else {
            quote! {
                <#encoder as ::bilrost::encoding::Encoder<_>>::encoded_len(#tag, &#ident, tm)
            }
        }
    }

    /// Returns the where clause constraint terms for the field's encoder.
    pub fn encoder_where(&self) -> Option<TokenStream> {
        if self.recurses {
            return None;
        }
        let ty = &self.ty;
        let encoder = &self.encoder;
        Some(if self.in_oneof {
            quote!(#encoder: ::bilrost::encoding::ValueEncoder<#ty>)
        } else {
            quote!(#encoder: ::bilrost::encoding::Encoder<#ty>)
        })
    }

    /// Returns the where clause constraint terms for the field's encoder.
    pub fn distinguished_encoder_where(&self) -> Option<TokenStream> {
        if self.recurses {
            return None;
        }
        let ty = &self.ty;
        let encoder = &self.encoder;
        Some(if self.in_oneof {
            quote!(#encoder: ::bilrost::encoding::DistinguishedValueEncoder<#ty>)
        } else {
            quote!(#encoder: ::bilrost::encoding::DistinguishedEncoder<#ty>)
        })
    }

    /// Returns methods to embed in the message. `ident` must be the name of the field within the
    /// message struct.
    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
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
            ) -> <#enumeration_ty as ::bilrost::encoding::EnumerationHelper<#field_ty>>::Output {
                <
                    #enumeration_ty as ::bilrost::encoding::EnumerationHelper<#field_ty>
                >::help_get(self.#ident)
            }

            fn #set(
                &mut self,
                val: <#enumeration_ty as ::bilrost::encoding::EnumerationHelper<#field_ty>>::Input,
            ) {
                self.#ident = <
                    #enumeration_ty as ::bilrost::encoding::EnumerationHelper<#field_ty>
                >::help_set(val);
            }
        })
    }
}
