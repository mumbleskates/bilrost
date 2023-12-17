#![doc(html_root_url = "https://docs.rs/bilrost-derive/0.12.2")]
// The `quote!` macro requires deep recursion.
#![recursion_limit = "4096"]

extern crate alloc;
extern crate proc_macro;

use alloc::collections::BTreeSet;
use core::mem::take;
use core::ops::Deref;

use anyhow::{bail, Error};
use itertools::Itertools;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Data, DataEnum, DataStruct, DeriveInput, Expr, Fields, FieldsNamed, FieldsUnnamed, Ident,
    Index, Variant,
};

use crate::field::Field;

mod field;

/// Helper type to ensure a value is used at runtime.
struct MustMove<T>(Option<T>);

impl<T> MustMove<T> {
    fn new(t: T) -> Self {
        Self(Some(t))
    }

    fn into_inner(mut self) -> T {
        take(&mut self.0).unwrap()
    }
}

impl<T> Drop for MustMove<T> {
    fn drop(&mut self) {
        if self.0.is_some() {
            panic!("a must-use value was dropped!");
        }
    }
}

impl<T> Deref for MustMove<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0.as_ref().unwrap()
    }
}

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;

    let ident = input.ident;

    // TODO(widders): distinguished structs

    syn::custom_keyword!(skip_debug);
    let skip_debug = input
        .attrs
        .iter()
        .any(|a| a.path().is_ident("bilrost") && a.parse_args::<skip_debug>().is_ok());

    syn::custom_keyword!(distinguished);
    let _distinguished = input
        .attrs
        .iter()
        .any(|a| a.path().is_ident("bilrost") && a.parse_args::<distinguished>().is_ok());

    // TODO(widders): universal features
    //  * there must be a mode to embed enum values directly inside an option
    //  * map keys must not recur

    // TODO(widders): test coverage for completed features:
    //  * non-repeated fields must only occur once

    // TODO(widders): distinguished features
    //  * unknown fields are forbidden
    //  * "required" is forbidden
    //  * present standard (non-optional) fields with default values are forbidden
    //  * HashMap is forbidden
    //  * map keys must be sorted ascending
    //  * repeated fields must have matching packed-ness
    //  * message typed fields must also be distinguished (unsafe trait?)

    let variant_data = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => bail!("Message can not be derived for an enum"),
        Data::Union(..) => bail!("Message can not be derived for a union"),
    };

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let (is_struct, fields): (bool, Vec<syn::Field>) = match variant_data {
        DataStruct {
            fields: Fields::Named(FieldsNamed { named: fields, .. }),
            ..
        } => (true, fields.into_iter().collect()),
        DataStruct {
            fields:
                Fields::Unnamed(FieldsUnnamed {
                    unnamed: fields, ..
                }),
            ..
        } => (false, fields.into_iter().collect()),
        DataStruct {
            fields: Fields::Unit,
            ..
        } => (false, Vec::new()),
    };

    let mut next_tag: u32 = 0;
    let unsorted_fields: Vec<(TokenStream, Field)> = fields
        .into_iter()
        .enumerate()
        .flat_map(|(i, field)| {
            let field_ident = field.ident.map(|x| quote!(#x)).unwrap_or_else(|| {
                let index = Index {
                    index: i as u32,
                    span: Span::call_site(),
                };
                quote!(#index)
            });
            match Field::new(field.attrs, Some(next_tag)) {
                Ok(Some(field)) => {
                    next_tag = field.tags().iter().max().map(|t| t + 1).unwrap_or(next_tag);
                    Some(Ok((field_ident, field)))
                }
                Ok(None) => None,
                Err(err) => Some(Err(
                    err.context(format!("invalid message field {}.{}", ident, field_ident))
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    if let Some((duplicate_tag, _)) = unsorted_fields
        .iter()
        .flat_map(|(_, field)| field.tags())
        .sorted_unstable()
        .tuple_windows()
        .find(|(a, b)| a == b)
    {
        bail!("message {} has duplicate tag {}", ident, duplicate_tag)
    };

    enum SortGroupPart {
        // A set of fields that can be sorted by any of their tags, as they are always contiguous
        Contiguous(Vec<(TokenStream, Field)>),
        // A oneof field that needs to be sorted based on its current value's tag
        Oneof((TokenStream, Field)),
    }
    use SortGroupPart::*;
    enum FieldChunk {
        // A field that does not need to be sorted
        AlwaysOrdered((TokenStream, Field)),
        // A set of fields that must be sorted before emitting
        SortGroup(Vec<SortGroupPart>),
    }
    use FieldChunk::*;
    let mut chunks = Vec::<FieldChunk>::new();
    let mut fields = unsorted_fields
        .iter()
        .cloned()
        .sorted_unstable_by_key(|(_, field)| field.first_tag())
        .peekable();
    // Current vecs we are building for FieldChunk::SortGroup and SortGroupPart::Contiguous
    let mut current_contiguous_group: Vec<(TokenStream, Field)> = vec![];
    let mut current_sort_group: Vec<SortGroupPart> = vec![];
    // Set of oneof tags that are interspersed with other fields, so we know when we're able to
    // put multiple fields into the same ordered group.
    let mut sort_group_oneof_tags = BTreeSet::<u32>::new();
    while let (Some(this_field), next_field) = (fields.next(), fields.peek()) {
        // The following logic is a bit involved, so ensure that we can't forget to use the values.
        let this_field = MustMove::new(this_field);
        let (_, field) = this_field.deref();
        let first_tag = field.first_tag();
        let last_tag = field.last_tag();
        // Check if this field is a oneof with tags interleaved with other fields' tags. If true,
        // this field must always be emitted into a sort group.
        let overlaps = next_field.is_some_and(|(_, next_field)| last_tag > next_field.first_tag());
        // Check if this field is already in a range we know requires runtime sorting.
        let in_current_sort_group = sort_group_oneof_tags
            .last()
            .is_some_and(|end| *end > first_tag);

        if in_current_sort_group {
            // We're still building a sort group.
            if overlaps {
                // This field overlaps others and must always be emitted independently.
                // Emit any current ordered group, then emit this field as another part on its own.
                if !current_contiguous_group.is_empty() {
                    current_sort_group.push(Contiguous(take(&mut current_contiguous_group)));
                }
                sort_group_oneof_tags.extend(field.tags());
                current_sort_group.push(Oneof(this_field.into_inner()));
            } else if sort_group_oneof_tags
                .range(first_tag..=last_tag)
                .next()
                .is_some()
            {
                // This field is a oneof that is itself interleaved by other oneofs and must always
                // be emitted independently. Emit any current ordered group, then emit this field as
                // another part on its own.
                if !current_contiguous_group.is_empty() {
                    current_sort_group.push(Contiguous(take(&mut current_contiguous_group)));
                }
                // In this case we don't need to add this field's tags to `sort_group_oneof_tags`,
                // because it doesn't itself overlap (we know that every field after this has a tag
                // greater than this field's last tag).
                current_sort_group.push(Oneof(this_field.into_inner()));
            } else {
                // This field doesn't overlap with anything so we just add it to the current group
                // of already-ordered fields.
                if let Some((_, previous_field)) = current_contiguous_group.last() {
                    if sort_group_oneof_tags
                        .range(previous_field.last_tag()..=first_tag)
                        .next()
                        .is_some()
                    {
                        // One of the overlapping oneofs in this sort group may emit a tag between
                        // the previous field in the ordered group and this one, so split the
                        // ordered group here.
                        current_sort_group.push(Contiguous(take(&mut current_contiguous_group)));
                    }
                }
                current_contiguous_group.push(this_field.into_inner());
            }
        } else {
            // We are not already in a sort group.
            if overlaps {
                // This field requires sorting with others. Begin a new sort group.
                sort_group_oneof_tags = field.tags().into_iter().collect();
                current_sort_group.push(Oneof(this_field.into_inner()));
            } else {
                // This field doesn't need to be sorted.
                chunks.push(AlwaysOrdered(this_field.into_inner()));
            }
        }

        if let Some(sort_group_end) = sort_group_oneof_tags.last().copied() {
            if !next_field.is_some_and(|(_, next_field)| next_field.first_tag() < sort_group_end) {
                // We've been building a sort group, but we just reached the end.
                if !current_contiguous_group.is_empty() {
                    current_sort_group.push(Contiguous(take(&mut current_contiguous_group)));
                }
                assert!(
                    !current_sort_group.is_empty(),
                    "emitting a sort group but there are no fields"
                );
                chunks.push(SortGroup(take(&mut current_sort_group)));
                sort_group_oneof_tags.clear();
            }
        }
    }
    assert!(
        current_sort_group.into_iter().next().is_none(),
        "fields left over after chunking"
    );
    assert!(
        current_contiguous_group.into_iter().next().is_none(),
        "fields left over after chunking"
    );
    drop(sort_group_oneof_tags);
    let fields = chunks;

    let encoded_len = fields.iter().map(|chunk| match chunk {
        AlwaysOrdered((field_ident, field)) => field.encoded_len(quote!(self.#field_ident)),
        SortGroup(parts) => {
            let parts: Vec<TokenStream> = parts
                .iter()
                .map(|part| match part {
                    Contiguous(fields) => {
                        let Some((_, first_field)) = fields.first() else {
                            panic!("empty contiguous field group");
                        };
                        let first_tag = first_field.first_tag();
                        let each_len = fields
                            .iter()
                            .cloned()
                            .map(|(field_ident, field)| {
                                field.encoded_len(quote!(instance.#field_ident))
                            });
                        quote! {
                            parts[nparts] = (#first_tag, Some(|instance, tm| {
                                0 #(+ #each_len)*
                            }));
                            nparts += 1;
                        }
                    }
                    Oneof((field_ident, _)) => quote! {
                        if let Some(oneof) = self.#field_ident.as_ref() {
                            parts[nparts] = (oneof.current_tag(), Some(|instance, tm| {
                                instance.#field_ident.as_ref().unwrap().encoded_len(tm)
                            }));
                            nparts += 1;
                        }
                    },
                })
                .collect();
            let max_parts = parts.len();
            // TODO(widders): when there are many parts, use Vec instead of array
            quote! {
                {
                    let mut parts = [
                        (0u32, None::<fn(&Self, &mut ::bilrost::encoding::TagMeasurer) -> usize>);
                        #max_parts
                    ];
                    let mut nparts = 0usize;
                    #(#parts)*
                    let parts = &mut parts[..nparts];
                    parts.sort_unstable_by_key(|(tag, _)| *tag);
                    parts.iter().map(|(_, len_func)| (len_func.unwrap())(self, tm)).sum::<usize>()
                }
            }
        }
    });

    let encode = fields.iter().map(|chunk| match chunk {
        AlwaysOrdered((field_ident, field)) => field.encode(quote!(self.#field_ident)),
        SortGroup(parts) => {
            let parts: Vec<TokenStream> = parts
                .iter()
                .map(|part| match part {
                    Contiguous(fields) => {
                        let Some((_, first_field)) = fields.first() else {
                            panic!("empty contiguous field group");
                        };
                        let first_tag = first_field.first_tag();
                        let each_field = fields
                            .iter()
                            .cloned()
                            .map(|(field_ident, field)| {
                                field.encode(quote!(instance.#field_ident))
                            });
                        quote! {
                            parts[nparts] = (#first_tag, Some(|instance, buf, tw| {
                                #(#each_field)*
                            }));
                            nparts += 1;
                        }
                    }
                    Oneof((field_ident, _)) => quote! {
                        if let Some(oneof) = self.#field_ident.as_ref() {
                            parts[nparts] = (oneof.current_tag(), Some(|instance, buf, tw| {
                                instance.#field_ident.as_ref().unwrap().encode(buf, tw)
                            }));
                            nparts += 1;
                        }
                    },
                })
                .collect();
            let max_parts = parts.len();
            // TODO(widders): when there are many parts, use Vec instead of array
            quote! {
                {
                    let mut parts = [
                        (0u32, None::<fn(&Self, &mut __B, &mut ::bilrost::encoding::TagWriter)>);
                        #max_parts
                    ];
                    let mut nparts = 0usize;
                    #(#parts)*
                    let parts = &mut parts[..nparts];
                    parts.sort_unstable_by_key(|(tag, _)| *tag);
                    parts.iter().for_each(|(_, encode_func)| (encode_func.unwrap())(self, buf, tw));
                }
            }
        }
    });

    let merge = unsorted_fields.iter().map(|(field_ident, field)| {
        let merge = field.merge(quote!(value));
        let tags = field.tags().into_iter().map(|tag| quote!(#tag));
        let tags = Itertools::intersperse(tags, quote!(|));

        quote! {
            #(#tags)* => {
                let mut value = &mut self.#field_ident;
                #merge.map_err(|mut error| {
                    error.push(STRUCT_NAME, stringify!(#field_ident));
                    error
                })
            },
        }
    });

    let struct_name = if unsorted_fields.is_empty() {
        quote!()
    } else {
        quote!(
            const STRUCT_NAME: &'static str = stringify!(#ident);
        )
    };

    let clear = unsorted_fields
        .iter()
        .map(|(field_ident, field)| field.clear(quote!(self.#field_ident)));

    let default = if is_struct {
        let default = unsorted_fields.iter().map(|(field_ident, field)| {
            let value = field.default();
            quote!(#field_ident: #value,)
        });
        quote! {#ident {
            #(#default)*
        }}
    } else {
        let default = unsorted_fields.iter().map(|(_, field)| {
            let value = field.default();
            quote!(#value,)
        });
        quote! {#ident (
            #(#default)*
        )}
    };

    let methods = unsorted_fields
        .iter()
        .flat_map(|(field_ident, field)| field.methods(field_ident))
        .collect::<Vec<_>>();
    let methods = if methods.is_empty() {
        quote!()
    } else {
        quote! {
            #[allow(dead_code)]
            impl #impl_generics #ident #ty_generics #where_clause {
                #(#methods)*
            }
        }
    };

    let expanded = quote! {
        impl #impl_generics ::bilrost::Message for #ident #ty_generics #where_clause {
            #[allow(unused_variables)]
            fn encode_raw<__B>(&self, buf: &mut __B) where __B: ::bilrost::bytes::BufMut {
                let tw = &mut ::bilrost::encoding::TagWriter::new();
                #(#encode)*
            }

            #[allow(unused_variables)]
            fn merge_field<__B>(
                &mut self,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                duplicated: bool,
                buf: &mut ::bilrost::encoding::Capped<__B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::bilrost::DecodeError>
            where
                __B: ::bilrost::bytes::Buf
            {
                #struct_name
                match tag {
                    #(#merge)*
                    _ => ::bilrost::encoding::skip_field(wire_type, buf),
                }
            }

            #[inline]
            fn encoded_len(&self) -> usize {
                let tm = &mut ::bilrost::encoding::TagMeasurer::new();
                0 #(+ #encoded_len)*
            }

            fn clear(&mut self) {
                #(#clear;)*
            }
        }

        impl #impl_generics ::core::default::Default for #ident #ty_generics #where_clause {
            fn default() -> Self {
                #default
            }
        }
    };
    let expanded = if skip_debug {
        expanded
    } else {
        let debugs = unsorted_fields.iter().map(|(field_ident, field)| {
            let wrapper = field.debug(quote!(self.#field_ident));
            let call = if is_struct {
                quote!(builder.field(stringify!(#field_ident), &wrapper))
            } else {
                quote!(builder.field(&wrapper))
            };
            quote! {
                 let builder = {
                     let wrapper = #wrapper;
                     #call
                 };
            }
        });
        let debug_builder = if is_struct {
            quote!(f.debug_struct(stringify!(#ident)))
        } else {
            quote!(f.debug_tuple(stringify!(#ident)))
        };
        quote! {
            #expanded

            impl #impl_generics ::core::fmt::Debug for #ident #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    let mut builder = #debug_builder;
                    #(#debugs;)*
                    builder.finish()
                }
            }
        }
    };

    let expanded = quote! {
        #expanded

        #methods
    };

    Ok(expanded.into())
}

#[proc_macro_derive(Message, attributes(bilrost))]
pub fn message(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_message(input.into()).unwrap().into()
}

fn try_enumeration(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;
    let ident = input.ident;

    syn::custom_keyword!(skip_debug);
    let skip_debug = input
        .attrs
        .into_iter()
        .any(|a| a.path().is_ident("bilrost") && a.parse_args::<skip_debug>().is_ok());

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let punctuated_variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        Data::Struct(_) => bail!("Enumeration can not be derived for a struct"),
        Data::Union(..) => bail!("Enumeration can not be derived for a union"),
    };

    // Map the variants into 'fields'.
    let mut variants: Vec<(Ident, Expr)> = Vec::new();
    for Variant {
        ident,
        fields,
        discriminant,
        ..
    } in punctuated_variants
    {
        match fields {
            Fields::Unit => (),
            Fields::Named(_) | Fields::Unnamed(_) => {
                bail!("Enumeration variants may not have fields")
            }
        }

        match discriminant {
            Some((_, expr)) => variants.push((ident, expr)),
            None => bail!("Enumeration variants must have a discriminant"),
        }
    }

    if variants.is_empty() {
        panic!("Enumeration must have at least one variant");
    }

    let default = variants[0].0.clone();

    let is_valid = variants
        .iter()
        .map(|(_, value)| quote!(#value => true));

    let try_from = variants.iter().map(
        |(variant, value)| quote!(#value => ::core::result::Result::Ok(#ident::#variant)),
    );

    let is_valid_doc = format!("Returns `true` if `value` is a variant of `{}`.", ident);

    let expanded = quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            #[doc=#is_valid_doc]
            pub fn is_valid(value: u32) -> bool {
                match value {
                    #(#is_valid,)*
                    _ => false,
                }
            }
        }

        impl #impl_generics ::core::default::Default for #ident #ty_generics #where_clause {
            fn default() -> #ident {
                #ident::#default
            }
        }

        impl #impl_generics ::core::convert::From::<#ident> for u32 #ty_generics #where_clause {
            fn from(value: #ident) -> u32 {
                value as u32
            }
        }

        impl #impl_generics ::core::convert::TryFrom::<u32> for #ident #ty_generics #where_clause {
            type Error = ::bilrost::DecodeError;

            fn try_from(value: u32) -> ::core::result::Result<#ident, ::bilrost::DecodeError> {
                match value {
                    #(#try_from,)*
                    _ => ::core::result::Result::Err(::bilrost::DecodeError::new("invalid enumeration value")),
                }
            }
        }
    };

    let expanded = if skip_debug {
        expanded
    } else {
        let debug = variants.iter().map(|(variant_ident, _)| {
            quote!(#ident::#variant_ident => stringify!(#variant_ident))
        });
        quote! {
            #expanded

            impl #impl_generics ::core::fmt::Debug for #ident #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    f.write_str(match self {
                        #(#debug,)*
                    })
                }
            }
        }
    };

    Ok(expanded.into())
}

#[proc_macro_derive(Enumeration, attributes(bilrost))]
pub fn enumeration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_enumeration(input.into()).unwrap().into()
}

fn try_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;

    let ident = input.ident;

    syn::custom_keyword!(skip_debug);
    let skip_debug = input
        .attrs
        .into_iter()
        .any(|a| a.path().is_ident("bilrost") && a.parse_args::<skip_debug>().is_ok());

    let variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        Data::Struct(..) => bail!("Oneof can not be derived for a struct"),
        Data::Union(..) => bail!("Oneof can not be derived for a union"),
    };

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Map the variants into 'fields'.
    let mut fields: Vec<(Ident, Field)> = Vec::new();
    for Variant {
        attrs,
        ident: variant_ident,
        fields: variant_fields,
        ..
    } in variants
    {
        if match variant_fields {
            Fields::Unit => 0,
            Fields::Named(FieldsNamed { named: fields, .. })
            | Fields::Unnamed(FieldsUnnamed {
                unnamed: fields, ..
            }) => fields.len(),
        } != 1
        {
            bail!("Oneof enum variants must have a single field");
        }
        match Field::new_oneof(attrs)? {
            Some(field) => fields.push((variant_ident, field)),
            None => bail!("invalid oneof variant: oneof variants may not be ignored"),
        }
    }

    if fields.iter().any(|(_, field)| field.tags().len() > 1) {
        panic!("variant with multiple tags");
    }
    if let Some((duplicate_tag, _)) = fields
        .iter()
        .flat_map(|(_, field)| field.tags())
        .sorted_unstable()
        .tuple_windows()
        .find(|(a, b)| a == b)
    {
        bail!(
            "invalid oneof {}: multiple variants have tag {}",
            ident,
            duplicate_tag
        );
    }

    let encode = fields.iter().map(|(variant_ident, field)| {
        let encode = field.encode(quote!(*value));
        quote!(#ident::#variant_ident(ref value) => { #encode })
    });

    let merge = fields.iter().map(|(variant_ident, field)| {
        let tag = field.tags()[0];
        let merge = field.merge(quote!(value));
        quote! {
            #tag => {
                match field {
                    ::core::option::Option::Some(#ident::#variant_ident(ref mut value)) => {
                        #merge
                    },
                    _ => {
                        let mut owned_value = ::core::default::Default::default();
                        let value = &mut owned_value;
                        #merge.map(|_| *field = ::core::option::Option::Some(#ident::#variant_ident(owned_value)))
                    },
                }
            }
        }
    });

    let encoded_len = fields.iter().map(|(variant_ident, field)| {
        let encoded_len = field.encoded_len(quote!(*value));
        quote!(#ident::#variant_ident(ref value) => #encoded_len)
    });

    let current_tag = fields.iter().map(|(variant_ident, field)| {
        let tag = field.tags()[0];
        quote!(#ident::#variant_ident(_) => #tag)
    });

    let expanded = quote! {
        impl #impl_generics #ident #ty_generics #where_clause {
            /// Encodes the message to a buffer.
            pub fn encode<__B>(
                &self,
                buf: &mut __B,
                tw: &mut ::bilrost::encoding::TagWriter,
            )
            where
                __B: ::bilrost::bytes::BufMut
            {
                match *self {
                    #(#encode,)*
                }
            }

            /// Decodes an instance of the message from a buffer, and merges it into self.
            pub fn merge<__B>(
                field: &mut ::core::option::Option<#ident #ty_generics>,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                duplicated: bool,
                buf: &mut ::bilrost::encoding::Capped<__B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::bilrost::DecodeError>
            where
                __B: ::bilrost::bytes::Buf
            {
                if duplicated {
                    return Err(::bilrost::DecodeError::new(
                        "multiple occurrences of non-repeated field"
                    ));
                }
                match tag {
                    #(#merge,)*
                    _ => unreachable!(concat!("invalid ", stringify!(#ident), " tag: {}"), tag),
                }
            }

            /// Returns the encoded length of the message without a length delimiter.
            #[inline]
            pub fn encoded_len(&self, tm: &mut ::bilrost::encoding::TagMeasurer) -> usize {
                match *self {
                    #(#encoded_len,)*
                }
            }

            /// Returns the tag id that will be encoded by the current value.
            #[inline]
            pub fn current_tag(&self) -> u32 {
                match *self {
                    #(#current_tag,)*
                }
            }
        }

    };
    let expanded = if skip_debug {
        expanded
    } else {
        let debug = fields.iter().map(|(variant_ident, field)| {
            let wrapper = field.debug(quote!(*value));
            quote!(#ident::#variant_ident(ref value) => {
                let wrapper = #wrapper;
                f.debug_tuple(stringify!(#variant_ident))
                    .field(&wrapper)
                    .finish()
            })
        });
        quote! {
            #expanded

            impl #impl_generics ::core::fmt::Debug for #ident #ty_generics #where_clause {
                fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                    match *self {
                        #(#debug,)*
                    }
                }
            }
        }
    };

    Ok(expanded.into())
}

#[proc_macro_derive(Oneof, attributes(bilrost))]
pub fn oneof(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_oneof(input.into()).unwrap().into()
}

#[cfg(test)]
mod test {
    use crate::{try_message, try_oneof};
    use quote::quote;

    #[test]
    fn test_rejects_colliding_message_fields() {
        let output = try_message(
            quote! {
                struct Invalid {
                    #[bilrost(bool, tag = "1")]
                    a: bool,
                    #[bilrost(oneof = "super::Whatever", tags = "4, 5, 1")]
                    b: Option<super::Whatever>,
                }
            }
            .into(),
        );
        assert!(output.is_err());
        assert_eq!(
            output.unwrap_err().to_string(),
            "message Invalid has duplicate tag 1"
        );
    }

    #[test]
    fn test_rejects_colliding_oneof_variants() {
        let output = try_oneof(
            quote! {
                pub enum Invalid {
                    #[bilrost(bool, tag = "1")]
                    A(bool),
                    #[bilrost(bool, tag = "1")]
                    B(bool),
                }
            }
            .into(),
        );
        assert!(output.is_err());
        assert_eq!(
            output.unwrap_err().to_string(),
            "invalid oneof Invalid: multiple variants have tag 1"
        );
    }

    #[test]
    fn test_basic_message() {
        let output = try_message(quote! {
            pub struct Struct {
                #[bilrost(btree_map = "string, sint64", tag = 3)]
                pub fields: BTreeMap<String, i64>,
                #[bilrost(string, tag = 0)]
                pub foo: String,
                #[bilrost(sint64, tag = 1)]
                pub bar: i64,
                #[bilrost(bool, tag = 2)]
                pub baz: bool,
            }
        });
        output.unwrap();
    }

        #[test]
    fn test_tuple_message() {
        let output = try_message(quote! {
            struct Tuple(
                #[bilrost(bool, tag = 5)] bool,
                #[bilrost(string, tag = 0)] String,
                #[bilrost(sint64)] i64,
            );
        });
        output.unwrap();
    }

    #[test]
    fn test_overlapping_message() {
        let output = try_message(quote! {
            struct Struct {
                #[bilrost(bool, tag = 0)]
                zero: bool,
                #[bilrost(oneof = "A", tags = "1, 10, 20")]
                a: Option<A>,
                #[bilrost(bool, tag = 4)]
                four: bool,
                #[bilrost(bool, tag = 5)]
                five: bool,
                #[bilrost(oneof = "B", tags = "9, 11")]
                b: Option<B>,
                #[bilrost(bool)] // implicitly tagged 12
                twelve: bool,
                #[bilrost(oneof = "C", tags = "13, 16, 22")]
                c: Option<C>,
                #[bilrost(bool, tag = 14)]
                fourteen: bool,
                #[bilrost(bool)] // implicitly tagged 15
                fifteen: bool,
                #[bilrost(bool, tag = 17)]
                seventeen: bool,
                #[bilrost(oneof = "D", tags = "18, 19")]
                d: Option<D>,
                #[bilrost(bool, tag = 21)]
                twentyone: bool,
                #[bilrost(bool, tag = 50)]
                fifty: bool,
            }
        });
        output.unwrap();
    }
}
