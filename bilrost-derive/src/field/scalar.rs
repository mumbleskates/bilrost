use std::convert::TryFrom;
use std::fmt;

use anyhow::{anyhow, bail, Error};
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{parse_str, Expr, ExprLit, Ident, Index, Lit, Meta, MetaNameValue, Path};

use crate::field::{bool_attr, set_option, tag_attr, Label};

/// A scalar protobuf field.
#[derive(Clone)]
pub struct Field {
    pub ty: Ty,
    pub kind: Kind,
    pub tag: u32,
}

impl Field {
    pub fn new(attrs: &[Meta], inferred_tag: Option<u32>) -> Result<Option<Field>, Error> {
        let mut ty = None;
        let mut label = None;
        let mut packed = None;
        let mut tag = None;

        let mut unknown_attrs = Vec::new();

        for attr in attrs {
            if let Some(t) = Ty::from_attr(attr)? {
                set_option(&mut ty, t, "duplicate type attributes")?;
            } else if let Some(p) = bool_attr("packed", attr)? {
                set_option(&mut packed, p, "duplicate packed attributes")?;
            } else if let Some(t) = tag_attr(attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if let Some(l) = Label::from_attr(attr) {
                set_option(&mut label, l, "duplicate label attributes")?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        let ty = match ty {
            Some(ty) => ty,
            None => return Ok(None),
        };

        match unknown_attrs.len() {
            0 => (),
            1 => bail!("unknown attribute: {:?}", unknown_attrs[0]),
            _ => bail!("unknown attributes: {:?}", unknown_attrs),
        }

        let tag = match tag.or(inferred_tag) {
            Some(tag) => tag,
            None => bail!("missing tag attribute"),
        };

        let kind = match (label, packed, ty.is_numeric()) {
            (None, Some(true), _)
            | (Some(Label::Optional), Some(true), _) => {
                bail!("packed attribute may only be applied to repeated fields");
            }
            (_, Some(true), false) => {
                bail!("packed attribute may only be applied to numeric types");
            }

            (None, _, _) => Kind::Plain,
            (Some(Label::Optional), _, _) => Kind::Optional,
            (Some(Label::Repeated), Some(true), _)
            | (Some(Label::Repeated), None, true) => Kind::Packed,
            (Some(Label::Repeated), _, _) => Kind::Repeated,
        };

        Ok(Some(Field { ty, kind, tag }))
    }

    pub fn new_oneof(attrs: &[Meta]) -> Result<Option<Field>, Error> {
        if let Some(mut field) = Field::new(attrs, None)? {
            match field.kind {
                Kind::Plain => Ok(Some(field)),
                Kind::Optional => bail!("invalid optional attribute on oneof field"),
                Kind::Packed | Kind::Repeated => bail!("invalid repeated attribute on oneof field"),
            }
        } else {
            Ok(None)
        }
    }

    pub fn encode(&self, ident: TokenStream) -> TokenStream {
        let module = self.ty.module();
        let encode_fn = match self.kind {
            Kind::Plain | Kind::Optional => quote!(encode),
            Kind::Repeated => quote!(encode_repeated),
            Kind::Packed => quote!(encode_packed),
        };
        let encode_fn = quote!(::bilrost::encoding::#module::#encode_fn);
        let tag = self.tag;

        match &self.kind {
            Kind::Plain => {
                let zero = self.ty.zero_value();
                quote! {
                    if #ident != #zero {
                        #encode_fn(#tag, &#ident, buf, tw);
                    }
                }
            }
            Kind::Optional => quote! {
                if let ::core::option::Option::Some(value) = &#ident {
                    #encode_fn(#tag, value, buf, tw);
                }
            },
            Kind::Repeated | Kind::Packed => quote! {
                #encode_fn(#tag, &#ident, buf, tw);
            },
        }
    }

    /// Returns an expression which evaluates to the result of merging a decoded
    /// scalar value into the field.
    pub fn merge(&self, ident: TokenStream) -> TokenStream {
        let module = self.ty.module();
        let merge_fn = match self.kind {
            Kind::Plain | Kind::Optional => quote!(merge),
            Kind::Repeated | Kind::Packed => quote!(merge_repeated),
        };
        let merge_fn = quote!(::bilrost::encoding::#module::#merge_fn);

        match self.kind {
            Kind::Plain | Kind::Repeated | Kind::Packed => quote! {
                #merge_fn(wire_type, #ident, buf, ctx)
            },
            Kind::Optional => quote! {
                #merge_fn(wire_type,
                          #ident.get_or_insert_with(::core::default::Default::default),
                          buf,
                          ctx)
            },
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field.
    pub fn encoded_len(&self, ident: TokenStream) -> TokenStream {
        let module = self.ty.module();
        let encoded_len_fn = match self.kind {
            Kind::Plain | Kind::Optional => quote!(encoded_len),
            Kind::Repeated => quote!(encoded_len_repeated),
            Kind::Packed => quote!(encoded_len_packed),
        };
        let encoded_len_fn = quote!(::bilrost::encoding::#module::#encoded_len_fn);
        let tag = self.tag;

        match &self.kind {
            Kind::Plain => {
                let zero = self.ty.zero_value();
                quote! {
                    if #ident != #zero {
                        #encoded_len_fn(#tag, &#ident, tm)
                    } else {
                        0
                    }
                }
            }
            Kind::Optional => quote! {
                #ident.as_ref().map_or(0, |value| #encoded_len_fn(#tag, value, tm))
            },
            Kind::Repeated | Kind::Packed => quote! {
                #encoded_len_fn(#tag, &#ident, tm)
            },
        }
    }

    pub fn clear(&self, ident: TokenStream) -> TokenStream {
        match &self.kind {
            Kind::Plain => {
                let zero = self.ty.zero_value();
                match self.ty {
                    Ty::String | Ty::Bytes(..) => quote!(#ident.clear()),
                    _ => quote!(#ident = #zero),
                }
            }
            Kind::Optional => quote!(#ident = ::core::option::Option::None),
            Kind::Repeated | Kind::Packed => quote!(#ident.clear()),
        }
    }

    /// Returns an expression which evaluates to the default value of the field.
    pub fn default(&self) -> TokenStream {
        match self.kind {
            Kind::Plain => self.ty.zero_value(),
            Kind::Optional => quote!(::core::option::Option::None),
            Kind::Repeated | Kind::Packed => quote!(::bilrost::alloc::vec::Vec::new()),
        }
    }

    /// An inner debug wrapper, around the base type.
    fn debug_inner(&self, wrap_name: TokenStream) -> TokenStream {
        if let Ty::Enumeration(ty) = &self.ty {
            quote! {
                struct #wrap_name<'a>(&'a u32);
                impl<'a> ::core::fmt::Debug for #wrap_name<'a> {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                        let res: ::core::result::Result<#ty, _> = ::core::convert::TryFrom::try_from(*self.0);
                        match res {
                            Err(_) => ::core::fmt::Debug::fmt(&self.0, f),
                            Ok(en) => ::core::fmt::Debug::fmt(&en, f),
                        }
                    }
                }
            }
        } else {
            quote! {
                #[allow(non_snake_case)]
                fn #wrap_name<T>(v: T) -> T { v }
            }
        }
    }

    /// Returns a fragment for formatting the field `ident` in `Debug`.
    pub fn debug(&self, wrapper_name: TokenStream) -> TokenStream {
        let wrapper = self.debug_inner(quote!(Inner));
        let inner_ty = self.ty.owned_type();
        match self.kind {
            Kind::Plain => self.debug_inner(wrapper_name),
            Kind::Optional => quote! {
                struct #wrapper_name<'a>(&'a ::core::option::Option<#inner_ty>);
                impl<'a> ::core::fmt::Debug for #wrapper_name<'a> {
                    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                        #wrapper
                        ::core::fmt::Debug::fmt(&self.0.as_ref().map(Inner), f)
                    }
                }
            },
            Kind::Repeated | Kind::Packed => {
                quote! {
                    struct #wrapper_name<'a>(&'a ::bilrost::alloc::vec::Vec<#inner_ty>);
                    impl<'a> ::core::fmt::Debug for #wrapper_name<'a> {
                        fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                            let mut vec_builder = f.debug_list();
                            for v in self.0 {
                                #wrapper
                                vec_builder.entry(&Inner(v));
                            }
                            vec_builder.finish()
                        }
                    }
                }
            }
        }
    }

    /// Returns methods to embed in the message.
    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
        let mut ident_str = ident.to_string();
        if ident_str.starts_with("r#") {
            ident_str = ident_str[2..].to_owned();
        }

        // Prepend `get_` for getter methods of tuple structs.
        let get = match parse_str::<Index>(&ident_str) {
            Ok(index) => {
                let get = Ident::new(&format!("get_{}", index.index), Span::call_site());
                quote!(#get)
            }
            Err(_) => quote!(#ident),
        };

        if let Ty::Enumeration(ty) = &self.ty {
            let set = Ident::new(&format!("set_{}", ident_str), Span::call_site());
            let set_doc = format!("Sets `{}` to the provided enum value.", ident_str);
            Some(match &self.kind {
                Kind::Plain => {
                    // TODO(widders): wtf do we do with this. just don't make a method right idk
                    let get_doc = format!(
                        "Returns the enum value of `{}`, \
                         or the default if the field is set to an invalid enum value.",
                        ident_str,
                    );
                    let zero = self.ty.zero_value();
                    quote! {
                        #[doc=#get_doc]
                        pub fn #get(&self) -> #ty {
                            ::core::convert::TryFrom::try_from(self.#ident).unwrap_or(#zero)
                        }

                        #[doc=#set_doc]
                        pub fn #set(&mut self, value: #ty) {
                            self.#ident = value as u32;
                        }
                    }
                }
                Kind::Optional => {
                    // TODO(widders): exact same thing here
                    let get_doc = format!(
                        "Returns the enum value of `{}`, \
                         or the default if the field is unset or set to an invalid enum value.",
                        ident_str,
                    );
                    let zero = self.ty.zero_value();
                    quote! {
                        #[doc=#get_doc]
                        pub fn #get(&self) -> ::core::option::Option<#ty {
                            self.#ident.and_then(|x| {
                                let result: ::core::result::Result<#ty, _> = ::core::convert::TryFrom::try_from(x);
                                result.ok()
                            }).unwrap_or(#zero)
                        }

                        #[doc=#set_doc]
                        pub fn #set(&mut self, value: #ty) {
                            self.#ident = ::core::option::Option::Some(value as u32);
                        }
                    }
                }
                Kind::Repeated | Kind::Packed => {
                    let iter_doc = format!(
                        "Returns an iterator which yields the valid enum values contained in `{}`.",
                        ident_str,
                    );
                    let push = Ident::new(&format!("push_{}", ident_str), Span::call_site());
                    let push_doc = format!("Appends the provided enum value to `{}`.", ident_str);
                    quote! {
                        #[doc=#iter_doc]
                        pub fn #get(&self) -> ::core::iter::FilterMap<
                            ::core::iter::Cloned<::core::slice::Iter<u32>>,
                            fn(u32) -> ::core::option::Option<#ty>,
                        > {
                            self.#ident.iter().cloned().filter_map(|x| {
                                let result: ::core::result::Result<#ty, _> = ::core::convert::TryFrom::try_from(x);
                                result.ok()
                            })
                        }
                        #[doc=#push_doc]
                        pub fn #push(&mut self, value: #ty) {
                            self.#ident.push(value as u32);
                        }
                    }
                }
            })
        } else if let Kind::Optional = &self.kind {
            let ty = self.ty.ref_type();

            let match_some = if self.ty.is_numeric() {
                quote!(::core::option::Option::Some(&val) => val,)
            } else {
                quote!(::core::option::Option::Some(val) => &val[..],)
            };

            let get_doc = format!("Returns the value of `{ident_str}`.");

            Some(quote! {
                #[doc=#get_doc]
                pub fn #get(&self) -> ::core::option::Option<#ty> {
                    self.#ident.as_ref().map(|val| #match_some)
                }
            })
        } else {
            None
        }
    }
}

/// A scalar protobuf field type.
#[derive(Clone, PartialEq, Eq)]
pub enum Ty {
    Float32,
    Float64,
    Uint32,
    Uint64,
    Sint32,
    Sint64,
    Ufixed32,
    Ufixed64,
    Sfixed32,
    Sfixed64,
    Bool,
    String,
    Bytes(BytesTy),
    Enumeration(Path),
    // TODO(widders): implement this as in-place enum value that impls TryFrom<u32>
    // ExactEnumeration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BytesTy {
    Vec,
    Bytes,
}

impl BytesTy {
    fn try_from_str(s: &str) -> Result<Self, Error> {
        match s {
            "vec" => Ok(BytesTy::Vec),
            "bytes" => Ok(BytesTy::Bytes),
            _ => bail!("Invalid bytes type: {}", s),
        }
    }

    fn rust_type(&self) -> TokenStream {
        match self {
            BytesTy::Vec => quote! { ::bilrost::alloc::vec::Vec<u8> },
            BytesTy::Bytes => quote! { ::bilrost::bytes::Bytes },
        }
    }
}

impl Ty {
    pub fn from_attr(attr: &Meta) -> Result<Option<Ty>, Error> {
        let ty = match attr {
            Meta::Path(name) if name.is_ident("float32") => Ty::Float32,
            Meta::Path(name) if name.is_ident("float64") => Ty::Float64,
            Meta::Path(name) if name.is_ident("uint32") => Ty::Uint32,
            Meta::Path(name) if name.is_ident("uint64") => Ty::Uint64,
            Meta::Path(name) if name.is_ident("sint32") => Ty::Sint32,
            Meta::Path(name) if name.is_ident("sint64") => Ty::Sint64,
            Meta::Path(name) if name.is_ident("ufixed32") => Ty::Ufixed32,
            Meta::Path(name) if name.is_ident("ufixed64") => Ty::Ufixed64,
            Meta::Path(name) if name.is_ident("sfixed32") => Ty::Sfixed32,
            Meta::Path(name) if name.is_ident("sfixed64") => Ty::Sfixed64,
            Meta::Path(name) if name.is_ident("bool") => Ty::Bool,
            Meta::Path(name) if name.is_ident("string") => Ty::String,
            Meta::Path(name) if name.is_ident("bytes") => Ty::Bytes(BytesTy::Vec),
            Meta::NameValue(MetaNameValue {
                path,
                value:
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(l),
                        ..
                    }),
                ..
            }) if path.is_ident("bytes") => Ty::Bytes(BytesTy::try_from_str(&l.value())?),
            Meta::NameValue(MetaNameValue {
                path,
                value:
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(l),
                        ..
                    }),
                ..
            }) if path.is_ident("enumeration") => Ty::Enumeration(parse_str::<Path>(&l.value())?),
            Meta::List(meta_list) if meta_list.path.is_ident("enumeration") => {
                Ty::Enumeration(meta_list.parse_args::<Path>()?)
            }
            _ => return Ok(None),
        };
        Ok(Some(ty))
    }

    pub fn from_str(s: &str) -> Result<Ty, Error> {
        let enumeration_len = "enumeration".len();
        let error = Err(anyhow!("invalid type: {}", s));
        let ty = match s.trim() {
            "float32" => Ty::Float32,
            "float64" => Ty::Float64,
            "uint32" => Ty::Uint32,
            "uint64" => Ty::Uint64,
            "sint32" => Ty::Sint32,
            "sint64" => Ty::Sint64,
            "ufixed32" => Ty::Ufixed32,
            "ufixed64" => Ty::Ufixed64,
            "sfixed32" => Ty::Sfixed32,
            "sfixed64" => Ty::Sfixed64,
            "bool" => Ty::Bool,
            "string" => Ty::String,
            "bytes" => Ty::Bytes(BytesTy::Vec),
            s if s.len() > enumeration_len && &s[..enumeration_len] == "enumeration" => {
                let s = &s[enumeration_len..].trim();
                match s.chars().next() {
                    Some('<') | Some('(') => (),
                    _ => return error,
                }
                match s.chars().next_back() {
                    Some('>') | Some(')') => (),
                    _ => return error,
                }

                Ty::Enumeration(parse_str::<Path>(s[1..s.len() - 1].trim())?)
            }
            _ => return error,
        };
        Ok(ty)
    }

    /// Returns the type as it appears in protobuf field declarations.
    pub fn as_str(&self) -> &'static str {
        match *self {
            Ty::Float32 => "float32",
            Ty::Float64 => "float64",
            Ty::Uint32 => "uint32",
            Ty::Uint64 => "uint64",
            Ty::Sint32 => "sint32",
            Ty::Sint64 => "sint64",
            Ty::Ufixed32 => "ufixed32",
            Ty::Ufixed64 => "ufixed64",
            Ty::Sfixed32 => "sfixed32",
            Ty::Sfixed64 => "sfixed64",
            Ty::Bool => "bool",
            Ty::String => "string",
            Ty::Bytes(..) => "bytes",
            Ty::Enumeration(..) => "enum",
        }
    }

    pub fn owned_type(&self) -> TokenStream {
        match self {
            Ty::String => quote!(::bilrost::alloc::string::String),
            Ty::Bytes(ty) => ty.rust_type(),
            _ => self.ref_type(),
        }
    }

    pub fn ref_type(&self) -> TokenStream {
        match self {
            Ty::Float32 => quote!(f32),
            Ty::Float64 => quote!(f64),
            Ty::Uint32 | Ty::Ufixed32 | Ty::Enumeration(..) => quote!(u32),
            Ty::Uint64 | Ty::Ufixed64 => quote!(u64),
            Ty::Sint32 | Ty::Sfixed32 => quote!(i32),
            Ty::Sint64 | Ty::Sfixed64 => quote!(i64),
            Ty::Bool => quote!(bool),
            Ty::String => quote!(&str),
            Ty::Bytes(..) => quote!(&[u8]),
        }
    }

    pub fn zero_value(&self) -> TokenStream {
        match self {
            Ty::Float32 => quote!(0f32),
            Ty::Float64 => quote!(0f64),
            Ty::Uint32 | Ty::Ufixed32 | Ty::Enumeration(..) => quote!(0u32),
            Ty::Uint64 | Ty::Ufixed64 => quote!(0u64),
            Ty::Sint32 | Ty::Sfixed32 => quote!(0i32),
            Ty::Sint64 | Ty::Sfixed64 => quote!(0i64),
            Ty::Bool => quote!(false),
            Ty::String => quote!(""),
            Ty::Bytes(..) => quote!(b"" as &[u8]),
        }
    }

    pub fn module(&self) -> Ident {
        match *self {
            Ty::Enumeration(..) => Ident::new("uint32", Span::call_site()),
            _ => Ident::new(self.as_str(), Span::call_site()),
        }
    }

    /// Returns false if the scalar type is length delimited (i.e., `string` or `bytes`).
    pub fn is_numeric(&self) -> bool {
        !matches!(self, Ty::String | Ty::Bytes(..))
    }
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Scalar Protobuf field types.
#[derive(Clone)]
pub enum Kind {
    /// A plain proto3 scalar field.
    Plain,
    /// An optional scalar field.
    Optional,
    /// A repeated scalar field.
    Repeated,
    /// A packed repeated scalar field.
    Packed,
}
