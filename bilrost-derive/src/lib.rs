#![doc(html_root_url = "https://docs.rs/bilrost-derive/0.1004.0-dev")]
// The `quote!` macro requires deep recursion.
#![recursion_limit = "4096"]

//! This crate contains the derive macro implementations for the
//! [`bilrost`][bilrost] crate; see the documentation in that crate for usage and
//! details.
//!
//! [bilrost]: https://docs.rs/bilrost

use std::collections::{BTreeMap, BTreeSet};
use std::mem::take;
use std::ops::Deref;

use anyhow::{anyhow, bail, Error};
use itertools::Itertools;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    parse2, Attribute, Data, DataEnum, DataStruct, DeriveInput, Expr, Fields, FieldsNamed,
    FieldsUnnamed, Ident, ImplGenerics, Index, Meta, MetaList, MetaNameValue, TypeGenerics,
    Variant, WhereClause,
};

use self::field::{bilrost_attrs, Field};

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
            PlainBytes as plainbytes,
            Unpacked as unpacked,
            Varint as varint,
        };
    }
}

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

use crate::field::set_option;
use FieldChunk::*;

struct PreprocessedMessage<'a> {
    ident: Ident,
    impl_generics: ImplGenerics<'a>,
    ty_generics: TypeGenerics<'a>,
    where_clause: Option<&'a WhereClause>,
    unsorted_fields: Vec<(TokenStream, Field)>,
    has_ignored_fields: bool,
}

fn preprocess_message(input: &DeriveInput) -> Result<PreprocessedMessage, Error> {
    let ident = input.ident.clone();

    let variant_data = match &input.data {
        Data::Struct(variant_data) => variant_data,
        // TODO(widders): ...make it possible to derive Message for an enum. this would be exactly
        //  equivalent to a message with one field which is a oneof with the same fields.
        Data::Enum(..) => bail!("Message can not be derived for an enum"),
        Data::Union(..) => bail!("Message can not be derived for a union"),
    };

    let fields: Vec<syn::Field> = match variant_data {
        DataStruct {
            fields: Fields::Named(FieldsNamed { named: fields, .. }),
            ..
        }
        | DataStruct {
            fields:
                Fields::Unnamed(FieldsUnnamed {
                    unnamed: fields, ..
                }),
            ..
        } => fields.into_iter().cloned().collect(),
        DataStruct {
            fields: Fields::Unit,
            ..
        } => Vec::new(),
    };

    let mut next_tag = Some(1);
    let mut has_ignored_fields = false;
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
                    next_tag = field.last_tag().checked_add(1);
                    Some(Ok((field_ident, field)))
                }
                Ok(None) => {
                    // Field is ignored
                    has_ignored_fields = true;
                    None
                }
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

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(PreprocessedMessage {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        unsorted_fields,
        has_ignored_fields,
    })
}

/// Sorts a vec of unsorted fields into discrete chunks that may be ordered together at runtime to
/// ensure that all their fields are encoded in sorted order.
fn sort_fields(unsorted_fields: Vec<(TokenStream, Field)>) -> Vec<FieldChunk> {
    let mut chunks = Vec::<FieldChunk>::new();
    let mut fields = unsorted_fields
        .into_iter()
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
        let overlaps =
            matches!(next_field, Some((_, next_field)) if last_tag > next_field.first_tag());
        // Check if this field is already in a range we know requires runtime sorting.
        // MSRV: can't use .last()
        let in_current_sort_group =
            matches!(sort_group_oneof_tags.iter().next_back(), Some(&end) if end > first_tag);

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

        // MSRV: can't use .last()
        if let Some(&sort_group_end) = sort_group_oneof_tags.iter().next_back() {
            if !matches!(
                next_field,
                Some((_, next_field)) if next_field.first_tag() < sort_group_end
            ) {
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

    chunks
}

/// Combines an optional already-existing where clause with additional terms for each field's
/// encoder to assert that it supports the field's type.
fn impl_append_wheres(
    where_clause: Option<&WhereClause>,
    self_where: Option<TokenStream>,
    field_wheres: impl Iterator<Item = TokenStream>,
) -> TokenStream {
    // dedup the where clauses by their String values
    let encoder_wheres: BTreeMap<_, _> = field_wheres
        .map(|where_| (where_.to_string(), where_))
        .collect();
    let appended_wheres: Vec<_> = self_where.iter().chain(encoder_wheres.values()).collect();
    if let Some(where_clause) = where_clause {
        quote! { #where_clause #(, #appended_wheres)* }
    } else if appended_wheres.is_empty() {
        quote!() // no where clause terms
    } else {
        quote! { where #(#appended_wheres),*}
    }
}

fn append_expedient_encoder_wheres<T>(
    where_clause: Option<&WhereClause>,
    self_where: Option<TokenStream>,
    fields: &[(T, Field)],
) -> TokenStream {
    impl_append_wheres(
        where_clause,
        self_where,
        fields
            .iter()
            .flat_map(|(_, field)| field.expedient_where_terms()),
    )
}

fn append_distinguished_encoder_wheres<T>(
    where_clause: Option<&WhereClause>,
    self_where: Option<TokenStream>,
    fields: &[(T, Field)],
) -> TokenStream {
    impl_append_wheres(
        where_clause,
        self_where,
        fields
            .iter()
            .flat_map(|(_, field)| field.distinguished_where_terms()),
    )
}

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;

    // TODO(widders): allow explicit custom default with an attr; perhaps only for a single field?

    let PreprocessedMessage {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        unsorted_fields,
        has_ignored_fields,
    } = preprocess_message(&input)?;
    let fields = sort_fields(unsorted_fields.clone());
    let where_clause = append_expedient_encoder_wheres(where_clause, None, &unsorted_fields);

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
        let decode = field.decode_expedient(quote!(value));
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

    let static_guards = unsorted_fields
        .iter()
        .filter_map(|(field_ident, field)| field.tag_list_guard(field_ident.to_string()));

    let field_idents: Vec<_> = unsorted_fields
        .iter()
        .map(|(field_ident, _)| field_ident)
        .collect();

    let initialize_ignored = if has_ignored_fields {
        quote!(..::core::default::Default::default())
    } else {
        quote!()
    };

    let expanded = quote! {
        impl #impl_generics ::bilrost::RawMessage for #ident #ty_generics #where_clause {
            const __ASSERTIONS: () = { #(#static_guards)* };

            #[allow(unused_variables)]
            fn raw_encode<__B>(&self, buf: &mut __B)
            where
                __B: ::bilrost::bytes::BufMut + ?Sized,
            {
                let tw = &mut ::bilrost::encoding::TagWriter::new();
                #(#encode)*
            }

            #[allow(unused_variables)]
            #[inline]
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

        impl #impl_generics ::bilrost::encoding::EmptyState
        for #ident #ty_generics #where_clause {
            fn empty() -> Self {
                Self {
                    #(#field_idents: ::bilrost::encoding::EmptyState::empty(),)*
                    #initialize_ignored
                }
            }

            fn is_empty(&self) -> bool {
                true #(&& ::bilrost::encoding::EmptyState::is_empty(&self.#field_idents))*
            }

            fn clear(&mut self) {
                #(::bilrost::encoding::EmptyState::clear(&mut self.#field_idents);)*
            }
        }
    };

    let aliases = encoder_alias_header();
    let expanded = quote! {
        const _: () = {
            #aliases

            #expanded

            #methods
        };
    };

    Ok(expanded)
}

#[proc_macro_derive(Message, attributes(bilrost))]
pub fn message(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_message(input.into()).unwrap().into()
}

fn try_distinguished_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;

    let PreprocessedMessage {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        unsorted_fields,
        has_ignored_fields,
    } = preprocess_message(&input)?;

    if has_ignored_fields {
        bail!("messages with ignored fields cannot be distinguished");
    }

    let where_clause = append_distinguished_encoder_wheres(
        where_clause,
        Some(quote!(Self: ::core::cmp::Eq)),
        &unsorted_fields,
    );

    let decode = unsorted_fields.iter().map(|(field_ident, field)| {
        let decode = field.decode_distinguished(quote!(value));
        let tags = field.tags().into_iter().map(|tag| quote!(#tag));
        let tags = Itertools::intersperse(tags, quote!(|));

        quote! {
            #(#tags)* => {
                let mut value = &mut self.#field_ident;
                canon.update(#decode.map_err(|mut error| {
                    error.push(STRUCT_NAME, stringify!(#field_ident));
                    error
                })?);
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

    let expanded = quote! {
        impl #impl_generics ::bilrost::RawDistinguishedMessage
        for #ident #ty_generics #where_clause {
            #[allow(unused_variables)]
            fn raw_decode_field_distinguished<__B>(
                &mut self,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                duplicated: bool,
                buf: ::bilrost::encoding::Capped<__B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError>
            where
                __B: ::bilrost::bytes::Buf + ?Sized,
            {
                #struct_name
                let mut canon = ::bilrost::Canonicity::Canonical;
                match tag {
                    #(#decode)*
                    _ => {
                        canon.update(::bilrost::Canonicity::HasExtensions);
                        ::bilrost::encoding::skip_field(wire_type, buf)?;
                    }
                }
                Ok(canon)
            }
        }
    };

    let aliases = encoder_alias_header();
    let expanded = quote! {
        const _: () = {
            #aliases

            #expanded
        };
    };

    Ok(expanded)
}

#[proc_macro_derive(DistinguishedMessage, attributes(bilrost))]
pub fn distinguished_message(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_distinguished_message(input.into()).unwrap().into()
}

fn try_enumeration(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;
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
    let mut zero_variant_ident = None;
    for Variant {
        attrs,
        ident,
        fields,
        discriminant,
        ..
    } in punctuated_variants
    {
        match fields {
            Fields::Unit => {}
            Fields::Named(_) | Fields::Unnamed(_) => {
                bail!("Enumeration variants may not have fields")
            }
        }

        let expr = variant_attr(&attrs)?
            .or(discriminant.map(|(_, expr)| expr))
            .ok_or_else(|| {
                anyhow!(
                    "Enumeration variants must have a discriminant or a #[bilrost(..)] \
                    attribute with a constant value"
                )
            })?;
        if is_zero_discriminant(&expr) {
            zero_variant_ident = Some(ident.clone());
        }
        variants.push((ident, expr));
    }

    if variants.is_empty() {
        bail!("Enumeration must have at least one variant");
    }

    let is_valid = variants.iter().map(|(_, value)| quote!(#value => true));

    let to_u32 = variants
        .iter()
        .map(|(variant, value)| quote!(#ident::#variant => #value));

    let try_from = variants
        .iter()
        .map(|(variant, value)| quote!(#value => #ident::#variant));

    // When the type has a zero-valued variant, we implement `EmptyState`. When it doesn't, we
    // need an alternate way to create a value to be overwritten, so we impl `NewForOverwrite`
    // directly.
    let creation_impl = if let Some(zero) = &zero_variant_ident {
        quote! {
            impl #impl_generics ::bilrost::encoding::EmptyState
            for #ident #ty_generics #where_clause {
                #[inline]
                fn empty() -> Self {
                    Self::#zero
                }

                #[inline]
                fn is_empty(&self) -> bool {
                    matches!(self, Self::#zero)
                }

                #[inline]
                fn clear(&mut self) {
                    *self = Self::empty();
                }
            }
        }
    } else {
        let (first_variant, _) = variants.first().unwrap();
        quote! {
            impl #impl_generics ::bilrost::encoding::NewForOverwrite
            for #ident #ty_generics #where_clause {
                fn new_for_overwrite() -> Self {
                    Self::#first_variant
                }
            }
        }
    };

    let check_empty = if zero_variant_ident.is_some() {
        quote! {
            if !allow_empty && ::bilrost::encoding::EmptyState::is_empty(value) {
                return Ok(::bilrost::Canonicity::NotCanonical);
            }
        }
    } else {
        quote!()
    };

    let expanded = quote! {
        impl #impl_generics ::bilrost::Enumeration for #ident #ty_generics #where_clause {
            #[inline]
            fn to_number(&self) -> u32 {
                match self {
                    #(#to_u32,)*
                }
            }

            #[inline]
            fn try_from_number(value: u32) -> ::core::result::Result<#ident, u32> {
                #[forbid(unreachable_patterns)]
                ::core::result::Result::Ok(match value {
                    #(#try_from,)*
                    _ => ::core::result::Result::Err(value)?,
                })
            }

            #[inline]
            fn is_valid(__n: u32) -> bool {
                #[forbid(unreachable_patterns)]
                match __n {
                    #(#is_valid,)*
                    _ => false,
                }
            }
        }

        #creation_impl

        impl #impl_generics ::bilrost::encoding::Wiretyped<::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            const WIRE_TYPE: ::bilrost::encoding::WireType = ::bilrost::encoding::WireType::Varint;
        }

        impl #impl_generics ::bilrost::encoding::ValueEncoder<::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            #[inline]
            fn encode_value<__B: ::bilrost::bytes::BufMut + ?Sized>(value: &Self, buf: &mut __B) {
                ::bilrost::encoding::encode_varint(
                    ::bilrost::Enumeration::to_number(value) as u64,
                    buf,
                );
            }

            #[inline]
            fn value_encoded_len(value: &Self) -> usize {
                ::bilrost::encoding::encoded_len_varint(
                    ::bilrost::encoding::Enumeration::to_number(value) as u64
                )
            }

            #[inline]
            fn decode_value<__B: ::bilrost::bytes::Buf + ?Sized>(
                value: &mut Self,
                mut buf: ::bilrost::encoding::Capped<__B>,
                _ctx: ::bilrost::encoding::DecodeContext,
            ) -> Result<(), ::bilrost::DecodeError> {
                let decoded = buf.decode_varint()?;
                let in_range = u32::try_from(decoded)
                    .map_err(|_| ::bilrost::DecodeErrorKind::OutOfDomainValue)?;
                *value = <Self as ::bilrost::Enumeration>::try_from_number(in_range)
                    .map_err(|_| ::bilrost::DecodeErrorKind::OutOfDomainValue)?;
                Ok(())
            }
        }

        impl #impl_generics
        ::bilrost::encoding::DistinguishedValueEncoder<::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            #[inline]
            fn decode_value_distinguished<__B: ::bilrost::bytes::Buf + ?Sized>(
                value: &mut Self,
                buf: ::bilrost::encoding::Capped<__B>,
                allow_empty: bool,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                ::bilrost::encoding::ValueEncoder::<::bilrost::encoding::General>::decode_value(
                    value,
                    buf,
                    ctx,
                )?;
                #check_empty
                Ok(::bilrost::Canonicity::Canonical)
            }
        }
    };

    Ok(expanded)
}

#[proc_macro_derive(Enumeration, attributes(bilrost))]
pub fn enumeration(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_enumeration(input.into()).unwrap().into()
}

/// Detects whether the given expression, denoting the discriminant of an enumeration variant, is
/// definitely zero.
fn is_zero_discriminant(expr: &Expr) -> bool {
    expr.to_token_stream().to_string() == "0"
}

/// Get the numeric variant value for an enumeration from attrs.
fn variant_attr(attrs: &Vec<Attribute>) -> Result<Option<Expr>, Error> {
    let mut result: Option<Expr> = None;
    for attr in attrs {
        if attr.meta.path().is_ident("bilrost") {
            let expr = match &attr.meta {
                Meta::List(MetaList { tokens, .. }) => parse2(tokens.clone())?,
                Meta::NameValue(MetaNameValue { value, .. }) => value.clone(),
                _ => bail!("attribute on enumeration variant should be its represented value"),
            };
            set_option(
                &mut result,
                expr,
                "duplicate value attributes on enumeration variant",
            )?;
        }
    }
    Ok(result)
}

struct PreprocessedOneof<'a> {
    ident: Ident,
    impl_generics: ImplGenerics<'a>,
    ty_generics: TypeGenerics<'a>,
    where_clause: Option<&'a WhereClause>,
    fields: Vec<(Ident, Field)>,
    empty_variant: Option<Ident>,
}

fn preprocess_oneof(input: &DeriveInput) -> Result<PreprocessedOneof, Error> {
    let ident = input.ident.clone();

    let variants = match &input.data {
        Data::Enum(DataEnum { variants, .. }) => variants.clone(),
        Data::Struct(..) => bail!("Oneof can not be derived for a struct"),
        Data::Union(..) => bail!("Oneof can not be derived for a union"),
    };

    // Oneof enums have either zero or one unit variant. If there is no such variant, the Oneof
    // trait is implemented on `Option<T>`, and `None` stands in for no fields being set. If there
    // is such a variant, it becomes the empty state for the type and stands in for no fields being
    // set.
    let mut empty_variant: Option<Ident> = None;
    let mut fields: Vec<(Ident, Field)> = Vec::new();
    // Map the variants into 'fields'.
    for Variant {
        attrs,
        ident: variant_ident,
        fields: variant_fields,
        ..
    } in variants
    {
        match variant_fields {
            Fields::Unit => {
                if empty_variant.replace(variant_ident).is_some() {
                    bail!("Oneofs may have at most one empty enum variant");
                }
                let attrs = bilrost_attrs(attrs)?;
                if !attrs.is_empty() {
                    bail!(
                        "Unknown attribute(s) on empty Oneof variant: {}",
                        quote!(#(#attrs),*)
                    );
                }
            }
            Fields::Named(FieldsNamed {
                named: variant_fields,
                ..
            })
            | Fields::Unnamed(FieldsUnnamed {
                unnamed: variant_fields,
                ..
            }) => match variant_fields.len() {
                0 => {
                    if empty_variant.replace(variant_ident).is_some() {
                        bail!("Oneofs may have at most one empty enum variant");
                    }
                    let attrs = bilrost_attrs(attrs)?;
                    if !attrs.is_empty() {
                        bail!(
                            "Unknown attribute(s) on empty Oneof variant: {}",
                            quote!(#(#attrs),*)
                        );
                    }
                }
                1 => {
                    let field = variant_fields.first().unwrap();
                    fields.push((
                        variant_ident,
                        Field::new_in_oneof(field.ty.clone(), field.ident.clone(), attrs)?,
                    ));
                }
                _ => bail!("Oneof enum variants must have at most a single field"),
            },
        };
    }

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    Ok(PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        fields,
        empty_variant,
    })
}

fn try_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;

    let PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        fields,
        empty_variant,
    } = preprocess_oneof(&input)?;

    let where_clause = append_expedient_encoder_wheres(where_clause, None, &fields);

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

    let encode = fields.iter().map(|(variant_ident, field)| {
        let encode = field.encode(quote!(*value));
        let with_value = field.with_value(quote!(value));
        quote!(#ident::#variant_ident #with_value => { #encode })
    });

    let encoded_len = fields.iter().map(|(variant_ident, field)| {
        let encoded_len = field.encoded_len(quote!(*value));
        let with_value = field.with_value(quote!(value));
        quote!(#ident::#variant_ident #with_value => #encoded_len)
    });

    let expanded = if let Some(empty_ident) = empty_variant {
        let current_tag = fields.iter().map(|(variant_ident, field)| {
            let tag = field.tags()[0];
            let ignored = field.with_value(quote!(_));
            quote!(#ident::#variant_ident #ignored => ::core::option::Option::Some(#tag))
        });

        let decode = fields.iter().map(|(variant_ident, field)| {
            let tag = field.first_tag();
            let decode = field.decode_expedient(quote!(value));
            let with_new_value = field.with_value(quote!(new_value));
            let with_value = field.with_value(quote!(value));
            quote! {
                #tag => match self {
                    #ident::#empty_ident => {
                        let mut new_value =
                            ::bilrost::encoding::NewForOverwrite::new_for_overwrite();
                        let mut value = &mut new_value;
                        #decode?;
                        *self = #ident::#variant_ident #with_new_value;
                        Ok(())
                    }
                    #ident::#variant_ident #with_value => {
                        #decode
                    }
                    _ => Err(::bilrost::DecodeError::new(
                        ::bilrost::DecodeErrorKind::ConflictingFields
                    )),
                }
            }
        });

        quote! {
            impl #impl_generics ::bilrost::encoding::Oneof
            for #ident #ty_generics #where_clause
            {
                const FIELD_TAGS: &'static [u32] = &[#(#sorted_tags),*];

                fn oneof_encode<__B: ::bilrost::bytes::BufMut + ?Sized>(
                    &self,
                    buf: &mut __B,
                    tw: &mut ::bilrost::encoding::TagWriter,
                ) {
                    match self {
                        #ident::#empty_ident => {}
                        #(#encode,)*
                    }
                }

                fn oneof_encoded_len(
                    &self,
                    tm: &mut ::bilrost::encoding::TagMeasurer,
                ) -> usize {
                    match self {
                        #ident::#empty_ident => 0,
                        #(#encoded_len,)*
                    }
                }

                fn oneof_current_tag(&self) -> ::core::option::Option<u32> {
                    match self {
                        #ident::#empty_ident => ::core::option::Option::None,
                        #(#current_tag,)*
                    }
                }

                fn oneof_decode_field<__B: ::bilrost::bytes::Buf + ?Sized>(
                    &mut self,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    duplicated: bool,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::DecodeContext,
                ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
                    match tag {
                        #(#decode,)*
                        _ => unreachable!(
                            concat!("invalid ", stringify!(#ident), " tag: {}"), tag,
                        ),
                    }
                }
            }

            impl #impl_generics ::bilrost::encoding::EmptyState
            for #ident #ty_generics #where_clause {
                #[inline]
                fn empty() -> Self {
                    #ident::#empty_ident
                }

                #[inline]
                fn is_empty(&self) -> bool {
                    matches!(self, #ident::#empty_ident)
                }

                #[inline]
                fn clear(&mut self) {
                    *self = Self::empty();
                }
            }
        }
    } else {
        // The oneof enum has no "empty" unit variant, so we implement the "non-empty" trait.
        let current_tag = fields.iter().map(|(variant_ident, field)| {
            let tag = field.tags()[0];
            let ignored = field.with_value(quote!(_));
            quote!(#ident::#variant_ident #ignored => #tag)
        });

        let decode = fields.iter().map(|(variant_ident, field)| {
            let tag = field.first_tag();
            let decode = field.decode_expedient(quote!(value));
            let with_new_value = field.with_value(quote!(new_value));
            let with_value = field.with_value(quote!(value));
            quote! {
                #tag => match field {
                    ::core::option::Option::None => {
                        let mut new_value =
                            ::bilrost::encoding::NewForOverwrite::new_for_overwrite();
                        let value = &mut new_value;
                        #decode?;
                        *field = Some(#ident::#variant_ident #with_new_value);
                        Ok(())
                    }
                    ::core::option::Option::Some(#ident::#variant_ident #with_value) => {
                        #decode
                    }
                    _ => Err(::bilrost::DecodeError::new(
                        ::bilrost::DecodeErrorKind::ConflictingFields
                    )),
                }
            }
        });

        quote! {
            impl #impl_generics ::bilrost::encoding::NonEmptyOneof
            for #ident #ty_generics #where_clause
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

                fn oneof_encoded_len(
                    &self,
                    tm: &mut ::bilrost::encoding::TagMeasurer,
                ) -> usize {
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
                        _ => unreachable!(
                            concat!("invalid ", stringify!(#ident), " tag: {}"), tag,
                        ),
                    }
                }
            }
        }
    };

    let aliases = encoder_alias_header();
    Ok(quote! {
        const _: () = {
            #aliases

            #expanded
        };
    })
}

#[proc_macro_derive(Oneof, attributes(bilrost))]
pub fn oneof(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_oneof(input.into()).unwrap().into()
}

fn try_distinguished_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;

    let PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        fields,
        empty_variant,
    } = preprocess_oneof(&input)?;
    let expanded = if let Some(empty_ident) = empty_variant {
        let where_clause = append_distinguished_encoder_wheres(
            where_clause,
            Some(quote!(Self: ::bilrost::encoding::Oneof)),
            &fields,
        );
        let decode = fields.iter().map(|(variant_ident, field)| {
            let tag = field.first_tag();
            let decode = field.decode_distinguished(quote!(value));
            let with_new_value = field.with_value(quote!(new_value));
            let with_value = field.with_value(quote!(value));
            quote! {
                #tag => match self {
                    #ident::#empty_ident => {
                        let mut new_value =
                            ::bilrost::encoding::NewForOverwrite::new_for_overwrite();
                        let mut value = &mut new_value;
                        let canon = #decode?;
                        *self = #ident::#variant_ident #with_new_value;
                        Ok(canon)
                    }
                    #ident::#variant_ident #with_value => {
                        #decode
                    }
                    _ => Err(::bilrost::DecodeError::new(
                        ::bilrost::DecodeErrorKind::ConflictingFields
                    )),
                }
            }
        });

        quote! {
            impl #impl_generics ::bilrost::encoding::DistinguishedOneof
            for #ident #ty_generics #where_clause
            {
                fn oneof_decode_field_distinguished<__B: ::bilrost::bytes::Buf + ?Sized>(
                    &mut self,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    duplicated: bool,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::DecodeContext,
                ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                    match tag {
                        #(#decode,)*
                        _ => unreachable!(
                            concat!("invalid ", stringify!(#ident), " tag: {}"), tag,
                        ),
                    }
                }
            }
        }
    } else {
        let where_clause = append_distinguished_encoder_wheres(where_clause, None, &fields);
        let decode = fields.iter().map(|(variant_ident, field)| {
            let tag = field.first_tag();
            let decode = field.decode_distinguished(quote!(value));
            let with_new_value = field.with_value(quote!(new_value));
            let with_value = field.with_value(quote!(value));
            quote! {
                #tag => match field {
                    ::core::option::Option::None => {
                        let mut new_value =
                            ::bilrost::encoding::NewForOverwrite::new_for_overwrite();
                        let value = &mut new_value;
                        let canon = #decode?;
                        *field = Some(#ident::#variant_ident #with_new_value);
                        Ok(canon)
                    }
                    ::core::option::Option::Some(#ident::#variant_ident #with_value) => {
                        #decode
                    }
                    _ => Err(::bilrost::DecodeError::new(
                        ::bilrost::DecodeErrorKind::ConflictingFields
                    )),
                }
            }
        });

        quote! {
            impl #impl_generics ::bilrost::encoding::NonEmptyDistinguishedOneof
            for #ident #ty_generics #where_clause
            {
                fn oneof_decode_field_distinguished<__B: ::bilrost::bytes::Buf + ?Sized>(
                    field: &mut ::core::option::Option<Self>,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    duplicated: bool,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::DecodeContext,
                ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                    match tag {
                        #(#decode,)*
                        _ => unreachable!(
                            concat!("invalid ", stringify!(#ident), " tag: {}"), tag,
                        ),
                    }
                }
            }
        }
    };

    let aliases = encoder_alias_header();
    let expanded = quote! {
        const _: () = {
            #aliases

            #expanded
        };
    };

    Ok(expanded)
}

#[proc_macro_derive(DistinguishedOneof, attributes(bilrost))]
pub fn distinguished_oneof(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_distinguished_oneof(input.into()).unwrap().into()
}

#[cfg(test)]
mod test {
    use crate::{try_enumeration, try_message, try_oneof};
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
        assert_eq!(
            output.expect_err("duplicate tags not detected").to_string(),
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
        assert_eq!(
            output
                .expect_err("conflicting variant tags not detected")
                .to_string(),
            "invalid oneof Invalid: multiple variants have tag 1"
        );
    }

    #[test]
    fn test_basic_message() {
        _ = try_message(quote! {
            pub struct Struct {
                #[bilrost(3)]
                pub fields: BTreeMap<String, i64>,
                #[bilrost(0)]
                pub foo: String,
                #[bilrost(1)]
                pub bar: i64,
                #[bilrost(2)]
                pub baz: bool,
            }
        })
        .unwrap();
    }

    #[test]
    fn test_attribute_forms_are_equivalent() {
        let one = try_message(quote! {
            struct A (
                #[bilrost(tag = "1")] bool,
                #[bilrost(oneof = "2, 3")] B,
                #[bilrost(tag = "4")] u32,
                #[bilrost(tag = "5", encoding = "::custom<Z>")] String,
                #[bilrost(tag = "1000")] i64,
                #[bilrost(tag = "1001")] bool,
            );
        })
        .unwrap()
        .to_string();
        let two = try_message(quote! {
            struct A (
                bool,
                #[bilrost(oneof = "2, 3")] B,
                #[bilrost(4)] u32,
                #[bilrost(encoding(::custom< Z >))] String,
                #[bilrost(tag = 1000)] i64,
                bool,
            );
        })
        .unwrap()
        .to_string();
        let three = try_message(quote! {
            struct A (
                #[bilrost(tag(1))] bool,
                #[bilrost(oneof(2, 3))] B,
                u32,
                #[bilrost(encoding = "::custom <Z>")] String,
                #[bilrost(tag(1000))] i64,
                bool,
            );
        })
        .unwrap()
        .to_string();
        let four = try_message(quote! {
            struct A (
                #[bilrost(1)] bool,
                #[bilrost(oneof(2, 3))] B,
                u32,
                #[bilrost(encoding(::custom<Z>))] String,
                #[bilrost(1000)] i64,
                #[bilrost()] bool,
            );
        })
        .unwrap()
        .to_string();
        let minimal = try_message(quote! {
            struct A (
                bool,
                #[bilrost(oneof(2, 3))] B,
                u32,
                #[bilrost(encoding(::custom<Z>))] String,
                #[bilrost(1000)] i64,
                bool,
            );
        })
        .unwrap()
        .to_string();
        assert_eq!(one, two);
        assert_eq!(one, three);
        assert_eq!(one, four);
        assert_eq!(one, minimal);
    }

    #[test]
    fn test_tuple_message() {
        _ = try_message(quote! {
            struct Tuple(
                #[bilrost(5)] bool,
                #[bilrost(0)] String,
                i64,
            );
        })
        .unwrap();
    }

    #[test]
    fn test_overlapping_message() {
        _ = try_message(quote! {
            struct Struct {
                #[bilrost(0)]
                zero: bool,
                #[bilrost(oneof(1, 10, 20))]
                a: Option<A>,
                #[bilrost(4)]
                four: bool,
                #[bilrost(5)]
                five: bool,
                #[bilrost(oneof(9, 11))]
                b: Option<B>,
                twelve: bool, // implicitly tagged 12
                #[bilrost(oneof(13, 16, 22))]
                c: Option<C>,
                #[bilrost(14)]
                fourteen: bool,
                fifteen: bool, // implicitly tagged 15
                #[bilrost(17)]
                seventeen: bool,
                #[bilrost(oneof(18, 19))]
                d: Option<D>,
                #[bilrost(21)]
                twentyone: bool,
                #[bilrost(50)]
                fifty: bool,
            }
        })
        .unwrap();
    }

    #[test]
    fn test_rejects_conflicting_empty_oneof_variants() {
        let output = try_oneof(quote!(
            enum AB {
                Empty,
                AlsoEmpty,
                #[bilrost(1)]
                A(bool),
                #[bilrost(2)]
                B(bool),
            }
        ));
        assert_eq!(
            output
                .expect_err("conflicting empty variants not detected")
                .to_string(),
            "Oneofs may have at most one empty enum variant"
        );
    }

    #[test]
    fn test_rejects_meaningless_empty_variant_attrs() {
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(tag = 0, encoding(usize), anything_else)]
                Empty,
                #[bilrost(1)]
                A(bool),
                #[bilrost(2)]
                B(bool),
            }
        ));
        assert_eq!(
            output
                .expect_err("unknown attrs on empty variant not detected")
                .to_string(),
            "Unknown attribute(s) on empty Oneof variant: tag = 0 , encoding (usize) , anything_else"
        );
    }

    #[test]
    fn test_rejects_unnumbered_oneof_variants() {
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(1)]
                A(u32),
                #[bilrost(encoding(packed))]
                B(Vec<String>),
            }
        ));
        assert_eq!(
            output
                .expect_err("unnumbered oneof variant not detected")
                .to_string(),
            "missing tag attribute"
        );
    }

    #[test]
    fn test_rejects_struct_and_union_enumerations() {
        let output = try_enumeration(quote!(
            struct X {
                x: String,
            }
        ));
        assert_eq!(
            output
                .expect_err("enumeration of struct not detected")
                .to_string(),
            "Enumeration can not be derived for a struct"
        );
        let output = try_enumeration(quote!(
            union XY {
                x: String,
                Y: Vec<u8>,
            }
        ));
        assert_eq!(
            output
                .expect_err("enumeration of union not detected")
                .to_string(),
            "Enumeration can not be derived for a union"
        );
    }

    #[test]
    fn test_rejects_variant_with_field_in_enumeration() {
        let output = try_enumeration(quote!(
            enum X {
                A = 1,
                B(u32) = 2,
            }
        ));
        assert_eq!(
            output
                .expect_err("variant with field not detected")
                .to_string(),
            "Enumeration variants may not have fields"
        );
    }

    #[test]
    fn test_accepts_mixed_values_in_enumeration() {
        _ = try_enumeration(quote!(
            enum X<T> {
                A = 1,
                #[bilrost = 1 + 1]
                B,
                #[bilrost(2 + 1)]
                C,
                #[bilrost(SomeType<T>::SOME_CONSTANT)]
                D,
            }
        ))
        .unwrap();
    }

    #[test]
    fn test_rejects_variant_without_value_in_enumeration() {
        let output = try_enumeration(quote!(
            enum X<T> {
                A = 1,
                #[bilrost = 1 + 1]
                B,
                #[bilrost(2 + 1)]
                C,
                #[bilrost(SomeType<T>::SOME_CONSTANT)]
                D,
                HasNoValue,
            }
        ));
        assert_eq!(
            output
                .expect_err("variant without discriminant not detected")
                .to_string(),
            "Enumeration variants must have a discriminant or a #[bilrost(..)] attribute with a \
            constant value"
        );
    }

    #[test]
    fn test_rejects_empty_enumeration() {
        let output = try_enumeration(quote!(
            enum X {}
        ));
        assert_eq!(
            output
                .expect_err("enumeration without variants not detected")
                .to_string(),
            "Enumeration must have at least one variant"
        );
    }
}
