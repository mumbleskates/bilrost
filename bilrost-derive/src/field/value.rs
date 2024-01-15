use anyhow::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse_str, Expr, Lit, Meta, MetaNameValue, Type};

use crate::field::{set_option, tag_attr};

/// A scalar protobuf field.
#[derive(Clone)]
pub struct Field {
    pub tag: u32,
    pub ty: Type,
    pub encoder: Type,
}

pub(super) fn encoder_attr(attr: &Meta) -> Result<Option<Type>, Error> {
    if !attr.path().is_ident("encoder") {
        return Ok(None);
    }
    match attr {
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            Lit::Str(lit) => parse_str::<Type>(&lit.value())
                .map(Some)
                .map_err(Error::from),
            _ => bail!("invalid tag attribute: {:?}", attr),
        },
        _ => bail!("invalid tag attribute: {:?}", attr),
    }
}

impl Field {
    pub fn new(ty: Type, attrs: &[Meta], inferred_tag: Option<u32>) -> Result<Field, Error> {
        let mut encoder = None;
        let mut tag = None;
        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if let Some(t) = encoder_attr(attr)? {
                set_option(&mut encoder, t, "duplicate encoder attributes")?;
            } else if let Some(t) = tag_attr(attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        match unknown_attrs.len() {
            0 => (),
            1 => bail!("unknown attribute: {:?}", unknown_attrs[0]),
            _ => bail!("unknown attributes: {:?}", unknown_attrs),
        }

        let tag = match tag.or(inferred_tag) {
            Some(tag) => tag,
            None => bail!("missing tag attribute"),
        };

        let encoder = encoder.unwrap_or(parse_str::<Type>("general")?);

        Ok(Field { tag, ty, encoder })
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`.
    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        let tag = self.tag;
        let encoder = &self.encoder;
        quote!(<#encoder as ::bilrost::encoding::Encoder<_>>::encode(#tag, &#ident, buf, tw);)
    }

    /// Returns an expression which evaluates to the result of merging a decoded value into the
    /// field. The given ident must be an &mut that already refers to the destination.
    pub fn decode(&self, ident: TokenStream) -> TokenStream {
        let encoder = &self.encoder;
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

    /// Returns an expression which evaluates to the encoded length of the field.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        let tag = self.tag;
        let encoder = &self.encoder;
        quote!(<#encoder as ::bilrost::encoding::Encoder<_>>::encoded_len(#tag, &#ident, tm))
    }

    /// Returns the where clause constraint terms for the field's encoder.
    pub fn where_clause(&self) -> TokenStream {
        let ty = &self.ty;
        let encoder = &self.encoder;
        quote!(#encoder: ::bilrost::encoding::Encoder<#ty>)
    }

    /// Returns methods to embed in the message.
    // TODO(widders): update this; it should mostly be the same but we want helpers for going back
    //  and forth between u32 or Option<u32> and the specified enum type, whatever its requirements
    //  are.
    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
        let mut ident_str = ident.to_string();
        if ident_str.starts_with("r#") {
            ident_str = ident_str[2..].to_owned();
        }

        // // Prepend `get_` for getter methods of tuple structs.
        // let get = match parse_str::<Index>(&ident_str) {
        //     Ok(index) => {
        //         let get = Ident::new(&format!("get_{}", index.index), Span::call_site());
        //         quote!(#get)
        //     }
        //     Err(_) => quote!(#ident),
        // };

        // TODO(widders): add a different attribute in the field to indicate whether and how the
        //  enumeration helper methods should be added
        None
    }
}
