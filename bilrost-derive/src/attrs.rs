use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::type_name;
use core::fmt::Debug;
use core::ops::RangeInclusive;
use eyre::{bail, eyre as err, Result};
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::ParseStream;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::{
    parse, parse2, Attribute, BinOp, Expr, ExprBinary, ExprLit, ExprRange, Lit, LitInt, LitStr,
    Meta, MetaList, MetaNameValue, Pat, RangeLimits, Token,
};

/// Get the items belonging to the 'bilrost' list attribute, e.g. `#[bilrost(foo, bar="baz")]`.
/// If a shorthand is provided and it transforms the whole contents of the attribute into one Meta,
/// then that transformation is used instead.
pub fn bilrost_attrs(
    attrs: &[Attribute],
    try_shorthand: Option<fn(&TokenStream) -> Option<Meta>>,
) -> Result<Vec<Meta>> {
    let mut result = Vec::new();
    for attr in attrs {
        if !attr.path().is_ident("bilrost") {
            continue;
        }
        match &attr.meta {
            Meta::List(meta_list) => {
                if let Some(try_shorthand) = &try_shorthand {
                    if let Some(replacement) = try_shorthand(&meta_list.tokens) {
                        result.push(replacement);
                        continue;
                    }
                }
                result.extend(
                    meta_list
                        .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                        .map_err(|err| {
                            err!(
                                "couldn't parse bilrost attributes {meta_list}: {err}",
                                meta_list = quote!(#meta_list),
                            )
                        })?,
                );
            }
            Meta::NameValue(meta_name_value) => {
                let Some(try_shorthand) = &try_shorthand else {
                    bail!(
                        "couldn't parse bilrost attribute {meta_name_value}: no shorthand for this location",
                        meta_name_value = quote!(#meta_name_value),
                    );
                };
                let Some(replacement) = try_shorthand(&meta_name_value.value.to_token_stream())
                else {
                    bail!(
                        "couldn't parse bilrost attribute {meta_name_value}: unrecognized shorthand",
                        meta_name_value = quote!(#meta_name_value),
                    );
                };
                result.push(replacement);
            }
            _ => {
                // we don't do anything with bare #[bilrost] attrs
            }
        }
    }
    Ok(result)
}

pub fn shorthand_tag(tokens: &TokenStream) -> Option<Meta> {
    parse2::<LitInt>(tokens.clone())
        .ok()
        .map(|short_tag| parse2::<Meta>(quote!(tag = #short_tag)).unwrap())
}

pub fn shorthand_enum_val(tokens: &TokenStream) -> Option<Meta> {
    if let Ok(expr) = parse2::<Expr>(tokens.clone()) {
        if syn::parse::Parser::parse2(Pat::parse_single, expr.to_token_stream()).is_ok() {
            return Some(parse2::<Meta>(quote!(val = #expr)).unwrap());
        }
    }
    None
}

#[test]
fn test_shorthand_enum_val() {
    assert!(shorthand_enum_val(&quote!(name = "abc")).is_none());
    // This version of the "name" attribute is still a potentially valid const expression for a
    // match pattern, so it is still turned into a "val" shorthand.
    assert!(shorthand_enum_val(&quote!(name("abc"))).is_some());
    assert!(shorthand_enum_val(&quote!(123)).is_some());
    assert!(shorthand_enum_val(&quote!(CONST)).is_some());
}

pub fn tag_attr(attr: &Meta) -> Result<Option<u32>> {
    numeric_attr(attr, "tag")
}

fn numeric_attr(attr: &Meta, key: &str) -> Result<Option<u32>> {
    if !attr.path().is_ident(key) {
        return Ok(None);
    }
    Ok(Some(match attr {
        // key(1)
        Meta::List(meta_list) => meta_list.parse_args::<LitInt>()?.base10_parse()?,
        Meta::NameValue(MetaNameValue {
            value: Expr::Lit(expr),
            ..
        }) => match &expr.lit {
            // key = "1"
            Lit::Str(lit) => lit.parse::<LitInt>()?.base10_parse()?,
            // key = 1
            Lit::Int(lit) => lit.base10_parse()?,
            _ => bail!("invalid {key} attribute: {attr}", attr = quote!(#attr)),
        },
        _ => bail!("invalid {key} attribute: {attr}", attr = quote!(#attr)),
    }))
}

#[derive(Debug, Default)]
pub struct TagList(Vec<RangeInclusive<u32>>);

impl TagList {
    fn validate(&mut self, range_size_limit: Option<usize>) -> Result<()> {
        for range in &self.0 {
            if range.is_empty() {
                bail!(
                    "invalid tag range {start}-{end}",
                    start = range.start(),
                    end = range.end()
                );
            }
            if let Some(limit) = range_size_limit {
                if usize::try_from(range.end() - range.start())?
                    .checked_add(1)
                    .unwrap()
                    >= limit
                {
                    bail!(
                        "too-large tag range {start}-{end}; use smaller ranges",
                        start = range.start(),
                        end = range.end()
                    );
                }
            }
        }
        self.0.sort_by_key(|r| (*r.start(), *r.end()));
        for (lower, higher) in self.0.iter().tuple_windows() {
            if lower.end() >= higher.start() {
                bail!(
                    "tag {start} is duplicated in tag list",
                    start = higher.start()
                );
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

    pub fn display(&self) -> String {
        use core::fmt::Write;
        let mut res = String::new();
        write!(&mut res, "(").unwrap();
        for (i, range) in self.0.iter().enumerate() {
            if i > 0 {
                write!(&mut res, ", ").unwrap();
            }
            if range.start() == range.end() {
                write!(&mut res, "{n}", n = range.start()).unwrap();
            } else {
                write!(&mut res, "{range:?}").unwrap();
            }
        }
        write!(&mut res, ")").unwrap();
        res
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
                        // One tag number prefixed by a `..=`
                        Expr::Range(ExprRange {
                            start: None,
                            limits: RangeLimits::Closed(..),
                            end: Some(right),
                            ..
                        }) => 0..=lit_u32(&right)?,
                        // One tag number suffixed by a `..`
                        Expr::Range(ExprRange {
                            start: Some(left),
                            limits: RangeLimits::HalfOpen(..),
                            end: None,
                            ..
                        }) => lit_u32(&left)?..=u32::MAX,
                        _ => {
                            return Err(syn::Error::new(
                                input.span(),
                                "expected either a single tag number (N), a range separated by \
                            a dash (N-M), a range-from (N..), or a range-to (..=N)",
                            ))
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
}

pub fn tag_list_attr(
    attr: &Meta,
    name: &str,
    range_size_limit: Option<usize>,
) -> Result<Option<TagList>> {
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
        }) => lit.parse(),
        _ => bail!("invalid {name} attribute: {attr}", attr = quote!(#attr)),
    }?;
    tag_list.validate(range_size_limit)?;
    Ok(Some(tag_list))
}

pub fn named_attr<T: parse::Parse>(attr: &Meta, attr_name: &str) -> Result<Option<T>> {
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
            Lit::Str(lit) => lit.parse(),
            _ => bail!(
                "invalid {attr_name} attribute: {attr}",
                attr = quote!(#attr)
            ),
        },
        _ => bail!(
            "invalid {attr_name} attribute: {attr}",
            attr = quote!(#attr)
        ),
    }
    .map(Some)
    .map_err(|_| {
        err!(
            "invalid {attr_name} attribute does not look like a(n) {ty}: {attr}",
            ty = type_name::<T>(),
            attr = quote!(#attr),
        )
    })
}

/// Get the numeric variant value for an enumeration from attrs.
pub fn enum_val_attr(attr: &Meta) -> Result<Option<Expr>> {
    if !attr.path().is_ident("val") {
        return Ok(None);
    }
    // attribute values for enumerations don't have to be exactly numeric literals, but they
    // will need to be used both as a literal-equivalent u32 value and as the match pattern
    // for the variant's corresponding value.
    let expr: Expr = match attr {
        // val(expr)
        Meta::List(list) => parse2(list.tokens.clone())?,
        // val = expr
        Meta::NameValue(name_value) => name_value.value.clone(),
        _ => bail!("invalid val attribute: {attr}", attr = quote!(#attr)),
    };

    // it's a valid expression; also make sure that it parses successfully as a
    // single-variant pattern
    if !syn::parse::Parser::parse2(Pat::parse_single, expr.to_token_stream()).is_ok() {
        bail!(
            "attribute on enumeration variant's 'val' attribute must be valid as both an \
            expression and a match pattern for u32"
        );
    };
    Ok(Some(expr))
}

/// Checks if an attribute matches a word.
pub fn word_attr(attr: &Meta, key: &str) -> bool {
    if let Meta::Path(ref path) = *attr {
        path.is_ident(key)
    } else {
        false
    }
}

/// Checks if an attribute provides a string literal
pub fn string_attr(attr: &Meta, key: &str) -> Result<Option<String>> {
    if !attr.path().is_ident(key) {
        return Ok(None);
    }
    match attr {
        // name("string value here")
        Meta::List(MetaList { tokens, .. }) => {
            let lit_str: LitStr = parse2(tokens.clone())?;
            Ok(Some(lit_str.value()))
        }
        // name = "string value here"
        Meta::NameValue(MetaNameValue {
            value:
                Expr::Lit(ExprLit {
                    lit: Lit::Str(lit_str),
                    ..
                }),
            ..
        }) => Ok(Some(lit_str.value())),
        _ => bail!("invalid {key} attribute: {attr}", attr = quote!(attr)),
    }
}

pub fn set_option<T: Debug>(option: &mut Option<T>, value: T, message: &str) -> Result<()> {
    set_option_with_display(option, value, message, |val| format!("{val:?}"))
}

pub fn set_option_with_display<T>(
    option: &mut Option<T>,
    value: T,
    message: &str,
    display: impl Fn(&T) -> String,
) -> Result<()>
where
    T: Debug,
{
    if let Some(existing) = option {
        bail!(
            "{message}: {existing} and {value}",
            existing = display(existing),
            value = display(&value),
        );
    }
    *option = Some(value);
    Ok(())
}

pub fn set_bool(b: &mut bool, message: &str) -> Result<()> {
    if *b {
        bail!("{message}");
    } else {
        *b = true;
        Ok(())
    }
}
