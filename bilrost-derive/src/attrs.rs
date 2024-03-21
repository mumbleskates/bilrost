use alloc::vec::Vec;
use core::any::type_name;
use core::ops::RangeInclusive;

use anyhow::{anyhow, bail, Error};
use itertools::Itertools;
use quote::quote;
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    parse, parse2, parse_str, BinOp, Expr, ExprBinary, ExprLit, Lit, LitInt, Meta, MetaList,
    MetaNameValue,
};

pub fn tag_attr(attr: &Meta) -> Result<Option<u32>, Error> {
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

#[derive(Debug, Default)]
pub struct TagList(Vec<RangeInclusive<u32>>);

impl TagList {
    fn validate(&mut self, range_size_limit: Option<usize>) -> Result<(), Error> {
        for range in &self.0 {
            if range.is_empty() {
                bail!("invalid tag range {}-{}", range.start(), range.end());
            }
            if let Some(limit) = range_size_limit {
                if (range.end() - range.start()) as usize + 1 >= limit {
                    bail!(
                        "too-large tag range {}-{}; use smaller ranges",
                        range.start(),
                        range.end()
                    );
                }
            }
        }
        self.0.sort_by_key(|r| (*r.start(), *r.end()));
        for (lower, higher) in self.0.iter().tuple_windows() {
            if lower.end() >= higher.start() {
                bail!("tag {} is duplicated in tag list", lower.end());
            }
        }
        Ok(())
    }

    pub fn iter_tags(&self) -> impl '_ + Iterator<Item = u32> {
        self.0.iter().cloned().flatten()
    }

    pub fn iter_tag_ranges(&self) -> impl '_ + Iterator<Item = RangeInclusive<u32>> {
        self.0.iter().cloned()
    }
}

impl parse::Parse for TagList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let lit_u32 = |expr: &Expr| match expr {
            Expr::Lit(ExprLit {
                lit: Lit::Int(lit), ..
            }) => lit.base10_parse::<u32>(),
            _ => Err(syn::Error::new(input.span(), "not an integer literal")),
        };

        Ok(Self(
            Punctuated::<Expr, Comma>::parse_terminated(input)?
                .into_iter()
                .map(|item| {
                    Ok(match item {
                        // Single tag number
                        Expr::Lit(lit) => {
                            let n = lit_u32(&Expr::Lit(lit))?;
                            n..=n
                        }
                        // Two tag numbers separated by a dash
                        Expr::Binary(ExprBinary {
                            left,
                            op: BinOp::Sub(_),
                            right,
                            ..
                        }) => {
                            let (left, right) = (lit_u32(&left)?, lit_u32(&right)?);
                            left..=right
                        }
                        _ => return Err(syn::Error::new(
                            input.span(),
                            "expected either a single tag number or a range separated by a dash",
                        )),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

pub fn tag_list_attr(
    name: &str,
    range_size_limit: Option<usize>,
    attr: &Meta,
) -> Result<Option<TagList>, Error> {
    if !attr.path().is_ident(name) {
        return Ok(None);
    }
    let mut tag_list: TagList = match attr {
        // attr(1, 2, 3, 4, 5)
        Meta::List(meta_list) => meta_list.parse_args(),
        // attr = "1, 2, 3, 4, 5"
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(ExprLit {
                lit: Lit::Str(lit), ..
            }),
            ..
        }) => parse_str(&lit.value()),
        _ => bail!("invalid {name} attribute: {}", quote!(#attr)),
    }?;
    tag_list.validate(range_size_limit)?;
    Ok(Some(tag_list))
}

pub fn named_attr<T: parse::Parse>(attr: &Meta, attr_name: &str) -> Result<Option<T>, Error> {
    if !attr.path().is_ident(attr_name) {
        return Ok(None);
    }
    match attr {
        // encoding(type tokens go here)
        Meta::List(MetaList { tokens, .. }) => parse2(tokens.clone()),
        // encoding = "type tokens go here"
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            Lit::Str(lit) => parse_str::<T>(&lit.value()),
            _ => bail!("invalid {attr_name} attribute: {}", quote!(#attr)),
        },
        _ => bail!("invalid {attr_name} attribute: {}", quote!(#attr)),
    }
    .map(Some)
    .map_err(|_| {
        anyhow!(
            "invalid {attr_name} attribute does not look like a(n) {}: {}",
            type_name::<T>(),
            quote!(#attr),
        )
    })
}

/// Checks if an attribute matches a word.
pub fn word_attr(attr: &Meta, key: &str) -> bool {
    if let Meta::Path(ref path) = *attr {
        path.is_ident(key)
    } else {
        false
    }
}
