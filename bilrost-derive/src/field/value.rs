use anyhow::{anyhow, bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, parse_str, Expr, Lit, LitInt, Meta, MetaList, MetaNameValue, Type};

use crate::field::set_option;

/// A scalar protobuf field.
#[derive(Clone)]
pub struct Field {
    pub tag: u32,
    pub ty: Type,
    pub encoder: Type,
    /// When a value field is in a oneof, it must always encode a nonzero amount of data. The
    /// encoder must be a ValueEncoder to satisfy this; effectively, Oneof types are much like
    /// several fields whose values are each wrapped in an `Option`, but at most one of them can be
    /// `Some`.
    pub in_oneof: bool,
}

pub(super) fn encoder_attr(attr: &Meta) -> Result<Option<Type>, Error> {
    if !attr.path().is_ident("encoder") {
        return Ok(None);
    }
    match attr {
        // encoder(type tokens go here)
        Meta::List(MetaList { tokens, .. }) => parse2(tokens.clone()),
        // encoder = "type tokens go here"
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            Lit::Str(lit) => parse_str::<Type>(&lit.value()),
            _ => bail!("invalid encoder attribute: {}", quote!(#attr)),
        },
        _ => bail!("invalid encoder attribute: {}", quote!(#attr)),
    }
    .map(Some)
    .map_err(|_| anyhow!("invalid encoder attribute does not look like a type: {}", quote!(#attr)))
}

impl Field {
    pub fn new(ty: &Type, attrs: &[Meta], inferred_tag: u32) -> Result<Field, Error> {
        Field::new_impl(ty, attrs, Some(inferred_tag), false)
    }

    pub fn new_in_oneof(ty: &Type, attrs: &[Meta]) -> Result<Field, Error> {
        Field::new_impl(ty, attrs, None, true)
    }

    fn new_impl(
        ty: &Type,
        attrs: &[Meta],
        inferred_tag: Option<u32>,
        in_oneof: bool,
    ) -> Result<Field, Error> {
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

        if !unknown_attrs.is_empty() {
            bail!("unknown attribute(s) for field: {}", quote!(#(#unknown_attrs),*))
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
            in_oneof,
        })
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
    pub fn encoder_where(&self) -> TokenStream {
        let ty = &self.ty;
        let encoder = &self.encoder;
        if self.in_oneof {
            quote!(#encoder: ::bilrost::encoding::ValueEncoder<#ty>)
        } else {
            quote!(#encoder: ::bilrost::encoding::Encoder<#ty>)
        }
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

pub(super) fn tag_attr(attr: &Meta) -> Result<Option<u32>, Error> {
    if !attr.path().is_ident("tag") {
        return Ok(None);
    }
    match attr {
        // tag(1)
        Meta::List(meta_list) => Ok(Some(meta_list.parse_args::<LitInt>()?.base10_parse()?)),
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            // tag = "1"
            Lit::Str(lit) => lit.value().parse::<u32>().map_err(Error::from).map(Some),
            // tag = 1
            Lit::Int(lit) => Ok(Some(lit.base10_parse()?)),
            _ => bail!("invalid tag attribute: {}", quote!(#attr)),
        },
        _ => bail!("invalid tag attribute: {}", quote!(#attr)),
    }
}
