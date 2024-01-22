use anyhow::{bail, Error};
use proc_macro2::TokenStream;
use quote::quote;
use syn::punctuated::Punctuated;
use syn::{Expr, ExprLit, Lit, LitInt, Meta, MetaNameValue, Token, Type};

use crate::field::set_option;

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
            if attr.path().is_ident("oneof") {
                let tags = match attr {
                    // oneof(1, 2, 3, 4, 5)
                    Meta::List(meta_list) => meta_list
                        .parse_args_with(Punctuated::<LitInt, Token![,]>::parse_terminated)?
                        .iter()
                        .map(LitInt::base10_parse)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(Error::from),
                    // oneof = "1, 2, 3, 4, 5"
                    Meta::NameValue(MetaNameValue {
                        value:
                            Expr::Lit(ExprLit {
                                lit: Lit::Str(lit), ..
                            }),
                        ..
                    }) => lit
                        .value()
                        .split(',')
                        .map(|s| s.trim().parse::<u32>().map_err(Error::from))
                        .collect::<Result<Vec<u32>, _>>(),
                    _ => bail!("invalid oneof attribute: {}", quote!(#attr)),
                }?;
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
            tags,
        }))
    }

    /// Returns a statement which encodes the oneof field.
    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        quote! {
            ::bilrost::encoding::Oneof::oneof_encode(&#ident, buf, tw);
        }
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode(&self, ident: TokenStream) -> TokenStream {
        quote!(
            ::bilrost::encoding::Oneof::oneof_decode_field(
                #ident,
                tag,
                wire_type,
                duplicated,
                buf,
                ctx,
            )
        )
    }

    /// Returns an expression which evaluates to the result of decoding the oneof field.
    pub fn decode_distinguished(&self, ident: TokenStream) -> TokenStream {
        quote!(
            ::bilrost::encoding::DistinguishedOneof::oneof_decode_field_distinguished(
                #ident,
                tag,
                wire_type,
                duplicated,
                buf,
                ctx,
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
}
