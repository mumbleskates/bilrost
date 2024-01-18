#![doc(html_root_url = "https://docs.rs/bilrost-derive/0.12.3")]
// The `quote!` macro requires deep recursion.
#![recursion_limit = "4096"]

use std::collections::BTreeSet;
use std::mem::take;
use std::ops::Deref;

use anyhow::{bail, Error};
use itertools::Itertools;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    parse_str, Data, DataEnum, DataStruct, DeriveInput, Expr, Fields, FieldsNamed, FieldsUnnamed,
    Ident, Index, Variant,
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

/// Defines the common aliases for encoder types available to every bilrost derive.
///
/// The standard encoders are all made available in scope with lower-cased names, making them
/// simultaneously easier to spell when writing the field attributes and making them less likely to
/// shadow custom encoder types.
fn encoder_alias_header() -> TokenStream {
    quote! {
        use ::bilrost::encoding::{
            Fixed as fixed,
            General as general,
            Map as map,
            Packed as packed,
            Unpacked as unpacked,
            VecBlob as vecblob,
        };
    }
}

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;

    let ident = input.ident;

    // TODO(widders): should we do this via a derive for `DistinguishedMessage` instead? that seems
    //  nicer since it means the trait is imported
    syn::custom_keyword!(distinguished);
    let _distinguished = input
        .attrs
        .iter()
        .any(|a| a.path().is_ident("bilrost") && a.parse_args::<distinguished>().is_ok());

    // TODO(widders): test coverage for completed features:
    //  * do prop-testing for stronger round-trip guarantees now that the encoding is better
    //    distinguished

    // TODO(widders): distinguished features
    //  * unknown fields are forbidden

    let variant_data = match input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => bail!("Message can not be derived for an enum"),
        Data::Union(..) => bail!("Message can not be derived for a union"),
    };

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let fields: Vec<syn::Field> = match variant_data {
        DataStruct {
            fields: Fields::Named(FieldsNamed { named: fields, .. }),
            ..
        } => fields.into_iter().collect(),
        DataStruct {
            fields:
                Fields::Unnamed(FieldsUnnamed {
                    unnamed: fields, ..
                }),
            ..
        } => fields.into_iter().collect(),
        DataStruct {
            fields: Fields::Unit,
            ..
        } => Vec::new(),
    };

    // TODO(widders): forbid implicit next_tag? hmmmmmm
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
            match Field::new(field.ty, field.attrs, next_tag) {
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
                        let each_len = fields.iter().cloned().map(|(field_ident, field)| {
                            field.encoded_len(quote!(instance.#field_ident))
                        });
                        quote! {
                            parts[nparts] = (#first_tag, Some(|instance, tm| {
                                0 #(+ #each_len)*
                            }));
                            nparts += 1;
                        }
                    }
                    Oneof((field_ident, field)) => {
                        let current_tag = field.current_tag(quote!(self.#field_ident));
                        let encoded_len = field.encoded_len(quote!(instance.#field_ident));
                        quote! {
                            if let Some(tag) = #current_tag {
                                parts[nparts] = (tag, Some(|instance, tm| {
                                    #encoded_len
                                }));
                                nparts += 1;
                            }
                        }
                    }
                })
                .collect();
            let max_parts = parts.len();
            // TODO(widders): when there are many parts, use Vec instead of array
            quote! {
                {
                    let mut parts = [
                        (0u32, ::core::option::Option::None::<
                                   fn(&Self, &mut ::bilrost::encoding::TagMeasurer) -> usize
                               >);
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
                        let each_field = fields.iter().cloned().map(|(field_ident, field)| {
                            field.encode(quote!(instance.#field_ident))
                        });
                        quote! {
                            parts[nparts] = (#first_tag, Some(|instance, buf, tw| {
                                #(#each_field)*
                            }));
                            nparts += 1;
                        }
                    }
                    Oneof((field_ident, field)) => {
                        let current_tag = field.current_tag(quote!(self.#field_ident));
                        let encode = field.encode(quote!(instance.#field_ident));
                        quote! {
                            if let Some(tag) = #current_tag {
                                parts[nparts] = (tag, Some(|instance, buf, tw| {
                                    #encode
                                }));
                                nparts += 1;
                            }
                        }
                    }
                })
                .collect();
            let max_parts = parts.len();
            // TODO(widders): when there are many parts, use Vec instead of array
            quote! {
                {
                    let mut parts = [
                        (0u32, ::core::option::Option::None::<
                                   fn(&Self, &mut __B, &mut ::bilrost::encoding::TagWriter)
                               >);
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

    let decode = unsorted_fields.iter().map(|(field_ident, field)| {
        let decode = field.decode(quote!(value));
        let tags = field.tags().into_iter().map(|tag| quote!(#tag));
        let tags = Itertools::intersperse(tags, quote!(|));

        quote! {
            #(#tags)* => {
                let mut value = &mut self.#field_ident;
                #decode.map_err(|mut error| {
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

    // Append the requirement that every field's type is encodable with its corresponding encoder
    // to our where clause.
    let where_clause =
        Field::append_wheres(where_clause, unsorted_fields.iter().map(|(_, field)| field));

    let static_guards = unsorted_fields
        .iter()
        .filter_map(|(field_ident, field)| field.tag_list_guard(field_ident.to_string()));

    let expanded = quote! {
        #(#static_guards)*

        impl #impl_generics ::bilrost::RawMessage for #ident #ty_generics #where_clause {
            #[allow(unused_variables)]
            fn raw_encode<__B>(&self, buf: &mut __B)
            where
                __B: ::bilrost::bytes::BufMut + ?Sized,
            {
                let tw = &mut ::bilrost::encoding::TagWriter::new();
                #(#encode)*
            }

            #[allow(unused_variables)]
            fn raw_decode_field<__B>(
                &mut self,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                duplicated: bool,
                buf: ::bilrost::encoding::Capped<__B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::bilrost::DecodeError>
            where
                __B: ::bilrost::bytes::Buf + ?Sized,
            {
                #struct_name
                match tag {
                    #(#decode)*
                    _ => ::bilrost::encoding::skip_field(wire_type, buf),
                }
            }

            #[inline]
            fn raw_encoded_len(&self) -> usize {
                let tm = &mut ::bilrost::encoding::TagMeasurer::new();
                0 #(+ #encoded_len)*
            }
        }
    };

    let impl_wrapper_const_ident = parse_str::<Ident>(
        &("__BILROST_DERIVED_IMPL_MESSAGE_FOR_".to_owned() + &ident.to_string()),
    )?;
    let aliases = encoder_alias_header();
    let expanded = quote! {
        const #impl_wrapper_const_ident: () = {
            #aliases

            #expanded

            #methods

            ()
        };
    };

    Ok(expanded)
}

#[proc_macro_derive(Message, attributes(bilrost))]
pub fn message(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_message(input.into()).unwrap().into()
}

fn try_enumeration(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;
    let ident = input.ident;

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

    let is_valid = variants.iter().map(|(_, value)| quote!(#value => true));

    let try_from = variants
        .iter()
        .map(|(variant, value)| quote!(#value => ::core::result::Result::Ok(#ident::#variant)));

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
                    _ => ::core::result::Result::Err(
                        ::bilrost::DecodeError::new("invalid enumeration value")),
                }
            }
        }

        impl #impl_generics ::bilrost::encoding::Wiretyped<#ident>
        for ::bilrost::encoding::General {
            const WIRE_TYPE: ::bilrost::encoding::WireType = ::bilrost::encoding::WireType::Varint;
        }

        impl #impl_generics ::bilrost::encoding::ValueEncoder<#ident>
        for ::bilrost::encoding::General {
            #[inline]
            fn encode_value<B: ::bilrost::bytes::BufMut + ?Sized>(value: &#ident, buf: &mut B) {
                ::bilrost::encoding::encode_varint(u32::from(value.clone()) as u64, buf);
            }

            #[inline]
            fn value_encoded_len(value: &#ident) -> usize {
                ::bilrost::encoding::encoded_len_varint(u32::from(value.clone()) as u64)
            }

            #[inline]
            fn decode_value<B: ::bilrost::bytes::Buf + ?Sized>(
                value: &mut #ident,
                buf: ::bilrost::encoding::Capped<B>,
                _ctx: ::bilrost::encoding::DecodeContext,
            ) -> Result<(), ::bilrost::DecodeError> {
                let int_value = u32::try_from(buf.decode_varint()?)
                    .map_err(|_| ::bilrost::DecodeError::new("varint overflows range of u32"))?;
                *value = #ident::try_from(int_value)?;
                Ok(())
            }
        }

        impl ::bilrost::encoding::DistinguishedValueEncoder<#ident>
        for ::bilrost::encoding::General {
            #[inline]
            fn decode_value_distinguished<B: ::bilrost::bytes::Buf + ?Sized>(
                value: &mut #ident,
                buf: ::bilrost::encoding::Capped<B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> Result<(), ::bilrost::DecodeError> {
                <Self as ::bilrost::encoding::ValueEncoder<#ident>>::decode_value(value, buf, ctx)
            }
        }
    };

    Ok(expanded)
}

#[proc_macro_derive(Enumeration, attributes(bilrost))]
pub fn enumeration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_enumeration(input.into()).unwrap().into()
}

fn try_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = syn::parse2(input)?;

    let ident = input.ident;

    let variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => variants,
        Data::Struct(..) => bail!("Oneof can not be derived for a struct"),
        Data::Union(..) => bail!("Oneof can not be derived for a union"),
    };

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    // Map the variants into 'fields'.
    let fields: Vec<(Ident, Field)> = variants
        .into_iter()
        .map(
            |Variant {
                 attrs,
                 ident: variant_ident,
                 fields: variant_fields,
                 ..
             }| {
                let ty = match variant_fields {
                    // TODO(widders): support a single empty default Unit variant in lieu of the
                    //  Option<T> implementation
                    Fields::Unit => bail!("Oneof enum variants must have a single field"),
                    Fields::Named(FieldsNamed { named: fields, .. })
                    | Fields::Unnamed(FieldsUnnamed {
                        unnamed: fields, ..
                    }) if fields.len() != 1 => {
                        bail!("Oneof enum variants must have a single field")
                    }
                    Fields::Named(FieldsNamed { named: fields, .. })
                    | Fields::Unnamed(FieldsUnnamed {
                        unnamed: fields, ..
                    }) => fields.first().unwrap().ty.clone(),
                };
                Ok((variant_ident, Field::new_in_oneof(ty, attrs)?))
            },
        )
        .collect::<Result<_, _>>()?;

    if fields.iter().any(|(_, field)| field.tags().len() > 1) {
        panic!("variant with multiple tags");
    }
    let sorted_tags: Vec<u32> = fields
        .iter()
        .flat_map(|(_, field)| field.tags())
        .sorted_unstable()
        .collect();
    if let Some((duplicate_tag, _)) = sorted_tags.iter().tuple_windows().find(|(a, b)| a == b) {
        bail!(
            "invalid oneof {}: multiple variants have tag {}",
            ident,
            duplicate_tag
        );
    }

    // Append the requirement that every field's type is encodable with its corresponding encoder
    // to our where clause.
    let where_clause = Field::append_wheres(where_clause, fields.iter().map(|(_, field)| field));

    let encode = fields.iter().map(|(variant_ident, field)| {
        // TODO(widders): THIS IS NOT QUITE IT! oneof fields must always encode if they have a
        //  value! right now, present oneof fields with a General encoder will absent when default,
        //  and that's simply not right; we might need another (third?) kind of encoding surety(?)
        //  in addition to encoder and value-encoder.
        //  however, this also messes with distinguished decoding (or rather, en-coding) of
        //  unpacked oneof variants, which when present and empty are indistinguishable from absent.
        //  do we have to just enforce that oneofs only value-encode their fields? that may be the
        //  only thing that makes sense here.
        let encode = field.encode(quote!(*value));
        quote!(#ident::#variant_ident(value) => { #encode })
    });

    let decode = fields.iter().map(|(variant_ident, field)| {
        let tag = field.first_tag();
        let decode = field.decode(quote!(value));
        quote! {
            #tag => {
                match field {
                    ::core::option::Option::None => {
                        let #ident::#variant_ident(value) = field.insert(#ident::#variant_ident(
                            ::bilrost::encoding::NewForOverwrite::new_for_overwrite()
                        )) else { panic!("unreachable") };
                        #decode
                    }
                    ::core::option::Option::Some(#ident::#variant_ident(value)) => {
                        #decode
                    }
                    _ => Err(::bilrost::DecodeError::new("conflicting fields in oneof")),
                }
            }
        }
    });

    let encoded_len = fields.iter().map(|(variant_ident, field)| {
        let encoded_len = field.encoded_len(quote!(*value));
        quote!(#ident::#variant_ident(value) => #encoded_len)
    });

    let current_tag = fields.iter().map(|(variant_ident, field)| {
        let tag = field.tags()[0];
        quote!(#ident::#variant_ident(_) => #tag)
    });

    let impl_wrapper_const_ident =
        parse_str::<Ident>(&("__BILROST_DERIVED_IMPL_ONEOF_FOR_".to_owned() + &ident.to_string()))?;
    let aliases = encoder_alias_header();
    let expanded = quote! {
        const #impl_wrapper_const_ident: () = {
            #aliases

            impl #impl_generics ::bilrost::encoding::NonEmptyOneof
            for #ident #ty_generics
            #where_clause
            {
                const FIELD_TAGS: &'static [u32] = &[#(#sorted_tags),*];

                fn oneof_encode<__B: ::bilrost::bytes::BufMut + ?Sized>(
                    &self,
                    buf: &mut __B,
                    tw: &mut ::bilrost::encoding::TagWriter,
                ) {
                    match self {
                        #(#encode,)*
                    }
                }

                fn oneof_encoded_len(&self, tm: &mut ::bilrost::encoding::TagMeasurer) -> usize {
                    match self {
                        #(#encoded_len,)*
                    }
                }

                fn oneof_current_tag(&self) -> u32 {
                    match self {
                        #(#current_tag,)*
                    }
                }

                fn oneof_decode_field<__B: ::bilrost::bytes::Buf + ?Sized>(
                    field: &mut ::core::option::Option<Self>,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    duplicated: bool,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::DecodeContext,
                ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
                    match tag {
                        #(#decode,)*
                        _ => unreachable!(concat!("invalid ", stringify!(#ident), " tag: {}"), tag),
                    }
                }
            }

            ()
        };
    };

    Ok(expanded)
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
        let output = try_message(quote! {
            struct Invalid {
                #[bilrost(tag = "1")]
                a: bool,
                #[bilrost(oneof(4, 5, 1))]
                b: Option<super::Whatever>,
            }
        });
        assert!(output.is_err());
        assert_eq!(
            output.unwrap_err().to_string(),
            "message Invalid has duplicate tag 1"
        );
    }

    #[test]
    fn test_rejects_colliding_oneof_variants() {
        let output = try_oneof(quote! {
            pub enum Invalid {
                #[bilrost(tag = "1")]
                A(bool),
                #[bilrost(tag = "1")]
                B(bool),
            }
        });
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
                #[bilrost(tag = 3)]
                pub fields: BTreeMap<String, i64>,
                #[bilrost(tag = 0)]
                pub foo: String,
                #[bilrost(tag = 1)]
                pub bar: i64,
                #[bilrost(tag = 2)]
                pub baz: bool,
            }
        });
        output.unwrap();
    }

    #[test]
    fn test_attribute_forms_are_equivalent() {
        let one = try_message(quote! {
            struct A (
                #[bilrost(tag = "1")] bool,
                #[bilrost(oneof = "2, 3")] B,
            );
        })
        .unwrap()
        .to_string();
        let two = try_message(quote! {
            struct A (
                #[bilrost(tag = 1)] bool,
                #[bilrost(oneof = "2, 3")] B,
            );
        })
        .unwrap()
        .to_string();
        let three = try_message(quote! {
            struct A (
                #[bilrost(tag(1))] bool,
                #[bilrost(oneof(2, 3))] B,
            );
        })
        .unwrap()
        .to_string();
        assert_eq!(one, two);
        assert_eq!(one, three);
    }

    #[test]
    fn test_tuple_message() {
        let output = try_message(quote! {
            struct Tuple(
                #[bilrost(tag = 5)] bool,
                #[bilrost(tag = 0)] String,
                i64,
            );
        });
        output.unwrap();
    }

    #[test]
    fn test_overlapping_message() {
        let output = try_message(quote! {
            struct Struct {
                #[bilrost(tag = 0)]
                zero: bool,
                #[bilrost(oneof(1, 10, 20))]
                a: Option<A>,
                #[bilrost(tag = 4)]
                four: bool,
                #[bilrost(tag = 5)]
                five: bool,
                #[bilrost(oneof(9, 11))]
                b: Option<B>,
                twelve: bool, // implicitly tagged 12
                #[bilrost(oneof(13, 16, 22))]
                c: Option<C>,
                #[bilrost(tag = 14)]
                fourteen: bool,
                fifteen: bool, // implicitly tagged 15
                #[bilrost(tag = 17)]
                seventeen: bool,
                #[bilrost(oneof(18, 19))]
                d: Option<D>,
                #[bilrost(tag = 21)]
                twentyone: bool,
                #[bilrost(tag = 50)]
                fifty: bool,
            }
        });
        output.unwrap();
    }
}
