#![doc(html_root_url = "https://docs.rs/bilrost-derive/0.1012.0-dev")]
// The `quote!` macro requires deep recursion.
#![recursion_limit = "4096"]
#![no_std]

//! This crate contains the derive macro implementations for the
//! [`bilrost`][bilrost] crate; see the documentation in that crate for usage and
//! details.
//!
//! [bilrost]: https://docs.rs/bilrost

extern crate alloc;

use crate::attrs::{tag_list_attr, word_attr, TagList};
use crate::field::{
    bilrost_attrs, set_bool, set_option,
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    Field,
    WhereFor::{self, Decode, Encode},
};
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::iter::repeat;
use core::mem::take;
use core::ops::{Deref, RangeInclusive};
use eyre::{bail, eyre as err, Error};
use itertools::Itertools;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{
    parse2, Attribute, Data, DataEnum, DataStruct, DeriveInput, Expr, Fields, FieldsNamed,
    FieldsUnnamed, Generics, Ident, Index, Meta, MetaList, MetaNameValue, Pat, TypeGenerics,
    Variant, WhereClause,
};

mod attrs;
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
use FieldChunk::*;

struct PreprocessedMessage<'a> {
    ident: Ident,
    impl_generics: &'a Generics,
    ty_generics: TypeGenerics<'a>,
    where_clause: Option<&'a WhereClause>,
    unsorted_fields: Vec<(TokenStream, Field)>,
    distinguished: bool,
    borrow_only: bool,
    has_ignored_fields: bool,
    tag_range: Option<RangeInclusive<u32>>,
}

fn preprocess_message(input: &DeriveInput) -> Result<PreprocessedMessage, Error> {
    let ident = input.ident.clone();

    let variant_data = match &input.data {
        Data::Struct(variant_data) => variant_data,
        Data::Enum(..) => panic!("should be unreachable, Message for enums depends on oneof"),
        Data::Union(..) => bail!("Message can not be derived for a union"),
    };

    let mut reserved_tags: Option<TagList> = None;
    let mut unknown_attrs = Vec::new();
    let mut distinguished = false;
    let mut borrow_only = false;
    for attr in bilrost_attrs(input.attrs.clone())? {
        if let Some(tags) = tag_list_attr(&attr, "reserved_tags", None)? {
            set_option(
                &mut reserved_tags,
                tags,
                "duplicate reserved_tags attributes",
            )?;
        } else if word_attr(&attr, "distinguished") {
            set_bool(&mut distinguished, "duplicate distinguished attributes")?;
        } else if word_attr(&attr, "borrowed") {
            set_bool(&mut borrow_only, "duplicate borrowed attributes")?;
        } else {
            unknown_attrs.push(attr);
        }
    }

    if !unknown_attrs.is_empty() {
        bail!(
            "unknown attribute(s) for message: {}",
            quote!(#(#unknown_attrs),*)
        )
    }
    let reserved_tags = reserved_tags.unwrap_or_default();

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

    // Tuple structs with anonymous fields have their field numbering start at zero, and structs
    // with named fields start at 1.
    let mut next_tag = Some(match variant_data.fields {
        Fields::Unnamed(..) => 0,
        _ => 1,
    });
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
                    err.wrap_err(format!("invalid message field {}.{}", ident, field_ident))
                )),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Index all fields by their tag(s) and check them against the forbidden tag ranges
    let all_tags: BTreeMap<u32, &TokenStream> = unsorted_fields
        .iter()
        .flat_map(|(ident, field)| field.tags().into_iter().zip(repeat(ident)))
        .collect();
    for reserved_range in reserved_tags.iter_tag_ranges() {
        if let Some((forbidden_tag, field_ident)) = all_tags.range(reserved_range).next() {
            bail!("message {ident} field {field_ident} has reserved tag {forbidden_tag}");
        }
    }
    let tag_range = all_tags
        .iter()
        .next()
        .map(|(first_tag, _)| *first_tag..=*all_tags.iter().next_back().unwrap().0);

    if let Some((duplicate_tag, _)) = unsorted_fields
        .iter()
        .flat_map(|(_, field)| field.tags())
        .sorted_unstable()
        .tuple_windows()
        .find(|(a, b)| a == b)
    {
        bail!("message {ident} has duplicate tag {duplicate_tag}")
    };

    let (_, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(PreprocessedMessage {
        ident,
        impl_generics: &input.generics,
        ty_generics,
        where_clause,
        unsorted_fields,
        distinguished,
        borrow_only,
        has_ignored_fields,
        tag_range,
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

/// Appends a WhereClause and a provided optional where term(s) into a phrase that will always be a
/// valid where clause if present.
fn append_self_where(
    where_clause: Option<&WhereClause>,
    self_where: Option<TokenStream>,
) -> Option<TokenStream> {
    match (where_clause, self_where) {
        (Some(a), Some(b)) => Some(quote!(#a, #b)),
        (Some(a), None) => Some(quote!(#a)),
        (None, Some(b)) => Some(quote!(where #b)),
        _ => None,
    }
}

/// Combines an optional already-existing where clause with additional terms for each field's
/// encoder to assert that it supports the field's type.
fn append_wheres<T>(
    where_clause: Option<&WhereClause>,
    self_where: Option<TokenStream>,
    fields: &[(T, Field)],
    field_purpose: WhereFor,
) -> Option<TokenStream> {
    // dedup the where clauses by their String values
    let encoder_wheres: BTreeMap<_, _> = fields
        .iter()
        .flat_map(|(_, field)| field.where_terms(field_purpose))
        .map(|where_| (where_.to_string(), where_))
        .collect();
    let mut appended_wheres = encoder_wheres.values().peekable();
    // append our encoder where terms to the existing where clause if there is one
    if let Some(header) = append_self_where(where_clause, self_where) {
        Some(quote! { #header #(, #appended_wheres)* })
    } else if appended_wheres.peek().is_none() {
        None
    } else {
        Some(quote! { where #(#appended_wheres),*})
    }
}

/// Adds the given identifier to the generics list
fn append_generic(generics: &Generics, ident: TokenStream) -> TokenStream {
    let params = &generics.params;
    quote!(<#ident, #params>)
}

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;

    if let Data::Enum(..) = input.data {
        return try_message_via_oneof(input);
    }

    let PreprocessedMessage {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        unsorted_fields,
        distinguished,
        borrow_only: _, // TODO(widders): impl borrow only
        has_ignored_fields,
        tag_range,
    } = preprocess_message(&input)?;

    if distinguished && has_ignored_fields {
        bail!("messages with ignored fields cannot be distinguished");
    }

    let fields = sort_fields(unsorted_fields.clone());
    let self_where = if has_ignored_fields {
        // When there are ignored fields, the whole message impl should be bounded by
        // Self: Default
        Some(quote!(Self: core::default::Default))
    } else {
        None
    };

    let borrow_generics = append_generic(impl_generics, quote!('__a));

    let encoder_where_clause =
        append_wheres(where_clause, self_where.clone(), &unsorted_fields, Encode);
    let [owned_decoder_where_clause, borrowed_decoder_where_clause] =
        [Owned, Borrowed].map(|lifetime| {
            append_wheres(
                where_clause,
                self_where.clone(),
                &unsorted_fields,
                Decode(lifetime, Relaxed),
            )
        });

    // If there can never be a tag delta larger than 31, field keys will never be more than 1 byte.
    let can_use_trivial_tag_measurer = matches!(tag_range, Some(range) if *range.end() < 32);

    let tag_measurer_ty = if can_use_trivial_tag_measurer {
        quote!(::bilrost::encoding::TrivialTagMeasurer)
    } else {
        quote!(::bilrost::encoding::RuntimeTagMeasurer)
    };

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
            quote! {
                {
                    let mut parts = [
                        (0u32, ::core::option::Option::None::<
                                   fn(&Self, &mut #tag_measurer_ty) -> usize
                               >);
                        #max_parts
                    ];
                    let mut nparts = 0usize;
                    #(#parts)*
                    let parts = &mut parts[..nparts];
                    <[_]>::sort_unstable_by_key(parts, |(tag, _)| *tag);
                    let mut total_len = 0usize;
                    for (_, len_func_option) in parts {
                        total_len += ::core::option::Option::unwrap(*len_func_option)(self, tm)
                    }
                    total_len
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

    let prepend = fields.iter().rev().map(|chunk| match chunk {
        AlwaysOrdered((field_ident, field)) => field.prepend(quote!(self.#field_ident)),
        SortGroup(parts) => {
            let parts: Vec<TokenStream> = parts
                .iter()
                .rev()
                .map(|part| match part {
                    Contiguous(fields) => {
                        let Some((_, first_field)) = fields.first() else {
                            panic!("empty contiguous field group");
                        };
                        let first_tag = first_field.first_tag();
                        let each_field =
                            fields.iter().rev().cloned().map(|(field_ident, field)| {
                                field.prepend(quote!(instance.#field_ident))
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
                        let prepend = field.prepend(quote!(instance.#field_ident));
                        quote! {
                            if let Some(tag) = #current_tag {
                                parts[nparts] = (tag, Some(|instance, buf, tw| {
                                    #prepend
                                }));
                                nparts += 1;
                            }
                        }
                    }
                })
                .collect();
            let max_parts = parts.len();
            quote! {
                {
                    let mut parts = [
                        (0u32, ::core::option::Option::None::<
                                   fn(&Self, &mut __B, &mut ::bilrost::encoding::TagRevWriter)
                               >);
                        #max_parts
                    ];
                    let mut nparts = 0usize;
                    #(#parts)*
                    let parts = &mut parts[..nparts];
                    parts.sort_unstable_by_key(|(tag, _)| ::core::cmp::Reverse(*tag));
                    parts.iter()
                        .for_each(|(_, prepend_func)| (prepend_func.unwrap())(self, buf, tw));
                }
            }
        }
    });

    let [decode_owned, decode_borrowed] = [Owned, Borrowed].map(|lifetime| {
        let ident = ident.clone();
        unsorted_fields.iter().map(move |(field_ident, field)| {
            let decode = field.decode(quote!(&mut self.#field_ident), lifetime, Relaxed);
            let tags = field.tags().into_iter().map(|tag| quote!(#tag));
            let tags = Itertools::intersperse(tags, quote!(|));

            quote! {
                #(#tags)* => {
                    if let ::core::result::Result::Err(mut error) = #decode {
                        error.push(stringify!(#ident), stringify!(#field_ident));
                        return ::core::result::Result::Err(error);
                    }
                }
            }
        })
    });

    let methods = unsorted_fields
        .iter()
        .flat_map(|(field_ident, field)| field.methods(field_ident))
        .collect::<Vec<_>>();
    let methods = if methods.is_empty() {
        quote!()
    } else {
        quote! {
            #[allow(dead_code)]
            impl #impl_generics #ident #ty_generics #encoder_where_clause {
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

    // The static guards should be instantiated within each of the methods of the trait; in newer
    // versions of rust simply instantiating a variable in any method with `let` is enough to cause
    // the assertions to be evaluated, but in older versions the evaluation might not happen unless
    // there is an actual code path that invokes the function.
    //
    // Even in rust 1.79 nightly, if the constant is never named anywhere the assertions won't
    // actually run.
    let impls = quote! {
        impl #impl_generics ::bilrost::encoding::RawMessage
        for #ident #ty_generics #encoder_where_clause {
            const __ASSERTIONS: () = { #(#static_guards)* };

            #[allow(unused_variables)]
            fn raw_encode<__B>(&self, buf: &mut __B)
            where
                __B: ::bilrost::bytes::BufMut + ?Sized,
            {
                let _ = <Self as ::bilrost::encoding::RawMessage>::__ASSERTIONS;
                let tw = &mut ::bilrost::encoding::TagWriter::new();
                #(#encode)*
            }

            #[allow(unused_variables)]
            fn raw_prepend<__B>(&self, buf: &mut __B)
            where
                __B: ::bilrost::buf::ReverseBuf + ?Sized,
            {
                let _ = <Self as ::bilrost::encoding::RawMessage>::__ASSERTIONS;
                let tw = &mut ::bilrost::encoding::TagRevWriter::new();
                #(#prepend)*
                tw.finalize(buf);
            }

            #[inline]
            fn raw_encoded_len(&self) -> usize {
                let _ = <Self as ::bilrost::encoding::RawMessage>::__ASSERTIONS;
                let tm = &mut #tag_measurer_ty::new();
                0 #(+ #encoded_len)*
            }
        }

        impl #impl_generics ::bilrost::encoding::RawMessageDecoder
        for #ident #ty_generics #owned_decoder_where_clause {
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
                let _ = <Self as ::bilrost::encoding::RawMessage>::__ASSERTIONS;
                match tag {
                    #(#decode_owned)*
                    _ => ::bilrost::encoding::skip_field(wire_type, buf)?,
                }
                ::core::result::Result::Ok(())
            }
        }

        impl #borrow_generics ::bilrost::encoding::RawMessageBorrowDecoder<'__a>
        for #ident #ty_generics #borrowed_decoder_where_clause {
            #[allow(unused_variables)]
            #[inline]
            fn raw_borrow_decode_field(
                &mut self,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                duplicated: bool,
                buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
                let _ = <Self as ::bilrost::encoding::RawMessage>::__ASSERTIONS;
                match tag {
                    #(#decode_borrowed)*
                    _ => ::bilrost::encoding::skip_field(wire_type, buf)?,
                }
                ::core::result::Result::Ok(())
            }
        }

        impl #impl_generics ::bilrost::encoding::ForOverwrite
        for #ident #ty_generics #encoder_where_clause {
            fn for_overwrite() -> Self {
                Self {
                    #(#field_idents: ::bilrost::encoding::ForOverwrite::for_overwrite(),)*
                    #initialize_ignored
                }
            }
        }

        impl #impl_generics ::bilrost::encoding::EmptyState
        for #ident #ty_generics #encoder_where_clause {
            fn is_empty(&self) -> bool {
                true #(&& ::bilrost::encoding::EmptyState::is_empty(&self.#field_idents))*
            }

            fn clear(&mut self) {
                #(::bilrost::encoding::EmptyState::clear(&mut self.#field_idents);)*
            }
        }
    };

    let distinguished_impls = distinguished.then(|| {
        let [owned_decoder_where_clause, borrowed_decoder_where_clause] =
            [Owned, Borrowed].map(|lifetime| {
                append_wheres(
                    where_clause,
                    Some(quote!(Self: ::core::cmp::Eq)),
                    &unsorted_fields,
                    Decode(lifetime, Distinguished),
                )
            });

        let [decode_owned, decode_borrowed] = [Owned, Borrowed].map(|lifetime| {
            let ident = ident.clone();
            unsorted_fields.iter().map(move |(field_ident, field)| {
                let decode = field.decode(quote!(&mut self.#field_ident), lifetime, Distinguished);
                let tags = field.tags().into_iter().map(|tag| quote!(#tag));
                let tags = Itertools::intersperse(tags, quote!(|));

                quote! {
                    #(#tags)* => {
                        match #decode {
                            ::core::result::Result::Ok(new_canon) => {
                                canon.update(new_canon);
                            }
                            ::core::result::Result::Err(mut error) => {
                                error.push(stringify!(#ident), stringify!(#field_ident));
                                return ::core::result::Result::Err(error);
                            }
                        }
                    }
                }
            })
        });

        quote! {
            impl #impl_generics ::bilrost::encoding::RawDistinguishedMessageDecoder
            for #ident #ty_generics #owned_decoder_where_clause {
                #[allow(unused_variables)]
                #[inline]
                fn raw_decode_field_distinguished<__B>(
                    &mut self,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    duplicated: bool,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError>
                where
                    __B: ::bilrost::bytes::Buf + ?Sized,
                {
                    let canon = &mut ::bilrost::Canonicity::Canonical;
                    match tag {
                        #(#decode_owned)*
                        _ => {
                            ctx.update(canon, ::bilrost::Canonicity::HasExtensions)?;
                            ::bilrost::encoding::skip_field(wire_type, buf)?;
                        }
                    }
                    ::core::result::Result::Ok(*canon)
                }
            }

            impl #borrow_generics ::bilrost::encoding::RawDistinguishedMessageBorrowDecoder<'__a>
            for #ident #ty_generics #borrowed_decoder_where_clause {
                #[allow(unused_variables)]
                #[inline]
                fn raw_borrow_decode_field_distinguished(
                    &mut self,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    duplicated: bool,
                    buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                    ctx: ::bilrost::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                    let canon = &mut ::bilrost::Canonicity::Canonical;
                    match tag {
                        #(#decode_borrowed)*
                        _ => {
                            ctx.update(canon, ::bilrost::Canonicity::HasExtensions)?;
                            ::bilrost::encoding::skip_field(wire_type, buf)?;
                        }
                    }
                    ::core::result::Result::Ok(*canon)
                }
            }
        }
    });

    let aliases = encoder_alias_header();
    let expanded = quote! {
        const _: () = {
            #aliases

            #impls

            #distinguished_impls

            #methods
        };
    };

    Ok(expanded)
}

fn try_message_via_oneof(input: DeriveInput) -> Result<TokenStream, Error> {
    let PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        fields,
        distinguished,
        borrow_only: _, // TODO(widders): impl borrow only
        empty_variant,
    } = preprocess_oneof(&input)?;

    let tag_measurer = if matches!(
        fields.iter().map(|(_, field)| field.last_tag()).max(),
        Some(last_tag) if last_tag >= 32
    ) {
        quote!(::bilrost::encoding::RuntimeTagMeasurer)
    } else {
        quote!(::bilrost::encoding::TrivialTagMeasurer)
    };

    if empty_variant.is_none() {
        bail!("Message can only be derived for Oneof enums that have an empty variant.")
    }

    let borrow_generics = append_generic(impl_generics, quote!('__a));

    let encoder_where_clause =
        append_self_where(where_clause, Some(quote!(Self: ::bilrost::encoding::Oneof)));
    let owned_decoder_where_clause = append_self_where(
        where_clause,
        Some(quote!(Self: ::bilrost::encoding::OneofDecoder)),
    );
    let borrowed_decoder_where_clause = append_self_where(
        where_clause,
        Some(quote!(Self: ::bilrost::encoding::OneofBorrowDecoder<'__a>)),
    );

    let impls = quote! {
        impl #impl_generics ::bilrost::encoding::RawMessage
        for #ident #ty_generics #encoder_where_clause {
            const __ASSERTIONS: () = ();

            #[inline(always)]
            fn raw_encode<__B>(&self, buf: &mut __B)
            where
                __B: ::bilrost::bytes::BufMut + ?Sized,
            {
                <Self as ::bilrost::encoding::Oneof>::oneof_encode(
                    self,
                    buf,
                    &mut ::bilrost::encoding::TagWriter::new(),
                );
            }

            #[inline(always)]
            fn raw_prepend<__B>(&self, buf: &mut __B)
            where
                __B: ::bilrost::buf::ReverseBuf + ?Sized,
            {
                let tw = &mut ::bilrost::encoding::TagRevWriter::new();
                <Self as ::bilrost::encoding::Oneof>::oneof_prepend(self, buf, tw);
                tw.finalize(buf);
            }

            #[inline(always)]
            fn raw_encoded_len(&self) -> usize {
                <Self as ::bilrost::encoding::Oneof>::oneof_encoded_len(
                    self,
                    &mut #tag_measurer::new(),
                )
            }
        }

        impl #impl_generics ::bilrost::encoding::RawMessageDecoder
        for #ident #ty_generics #owned_decoder_where_clause {
            #[inline(always)]
            fn raw_decode_field<__B>(
                &mut self,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                _duplicated: bool,
                buf: ::bilrost::encoding::Capped<__B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::bilrost::DecodeError>
            where
                __B: ::bilrost::bytes::Buf + ?Sized,
            {
                if <Self as ::bilrost::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                    <Self as ::bilrost::encoding::OneofDecoder>::oneof_decode_field(
                        self,
                        tag,
                        wire_type,
                        buf,
                        ctx,
                    )
                } else {
                    ::core::result::Result::Ok(())
                }
            }
        }

        impl #borrow_generics ::bilrost::encoding::RawMessageBorrowDecoder<'__a>
        for #ident #ty_generics #borrowed_decoder_where_clause {
            #[inline(always)]
            fn raw_borrow_decode_field(
                &mut self,
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                _duplicated: bool,
                buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
                if <Self as ::bilrost::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                    <Self as ::bilrost::encoding::OneofBorrowDecoder>::oneof_borrow_decode_field(
                        self,
                        tag,
                        wire_type,
                        buf,
                        ctx,
                    )
                } else {
                    ::core::result::Result::Ok(())
                }
            }
        }
    };

    let distinguished_impls = distinguished.then(|| {
        let owned_decoder_where_clause = append_self_where(
            where_clause,
            Some(quote!(
                Self: ::bilrost::encoding::DistinguishedOneofDecoder + ::core::cmp::Eq
            )),
        );
        let borrowed_decoder_where_clause = append_self_where(
            where_clause,
            Some(quote!(
                Self: ::bilrost::encoding::DistinguishedOneofBorrowDecoder<'__a> + ::core::cmp::Eq
            )),
        );

        quote! {
            impl #impl_generics ::bilrost::encoding::RawDistinguishedMessageDecoder
            for #ident #ty_generics #owned_decoder_where_clause {
                #[inline(always)]
                fn raw_decode_field_distinguished<__B>(
                    &mut self,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    _duplicated: bool,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError>
                where
                    __B: ::bilrost::bytes::Buf + ?Sized,
                {
                    if <Self as ::bilrost::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                        <Self as ::bilrost::encoding::DistinguishedOneofDecoder>::
                            oneof_decode_field_distinguished
                        (
                            self,
                            tag,
                            wire_type,
                            buf,
                            ctx,
                        )
                    } else {
                        ctx.check(::bilrost::Canonicity::HasExtensions)
                    }
                }
            }

            impl #borrow_generics ::bilrost::encoding::RawDistinguishedMessageBorrowDecoder<'__a>
            for #ident #ty_generics #borrowed_decoder_where_clause {
                #[inline(always)]
                fn raw_borrow_decode_field_distinguished(
                    &mut self,
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    _duplicated: bool,
                    buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                    ctx: ::bilrost::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                    if <Self as ::bilrost::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                        <Self as ::bilrost::encoding::DistinguishedOneofBorrowDecoder>::
                            oneof_borrow_decode_field_distinguished
                        (
                            self,
                            tag,
                            wire_type,
                            buf,
                            ctx,
                        )
                    } else {
                        ctx.check(::bilrost::Canonicity::HasExtensions)
                    }
                }
            }
        }
    });

    Ok(quote!(
        #impls

        #distinguished_impls
    ))
}

#[proc_macro_derive(Message, attributes(bilrost))]
pub fn message(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_message(input.into()).unwrap().into()
}

fn try_enumeration(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;
    let ident = input.ident;

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let borrow_generics = append_generic(generics, quote!('__a));

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
                err!(
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
    // need at least some way to create a value to be overwritten, so we impl `ForOverwrite`
    // directly with an arbitrary variant.
    let creation_impl = if let Some(zero) = &zero_variant_ident {
        quote! {
            impl #impl_generics ::bilrost::encoding::ForOverwrite
            for #ident #ty_generics #where_clause {
                #[inline]
                fn for_overwrite() -> Self {
                    Self::#zero
                }
            }

            impl #impl_generics ::bilrost::encoding::EmptyState
            for #ident #ty_generics #where_clause {
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
            impl #impl_generics ::bilrost::encoding::ForOverwrite
            for #ident #ty_generics #where_clause {
                fn for_overwrite() -> Self {
                    Self::#first_variant
                }
            }
        }
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
            fn prepend_value<__B: ::bilrost::buf::ReverseBuf + ?Sized>(
                value: &Self,
                buf: &mut __B,
            ) {
                ::bilrost::encoding::prepend_varint(
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
        }

        impl #impl_generics ::bilrost::encoding::ValueDecoder<::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            #[inline]
            fn decode_value<__B: ::bilrost::bytes::Buf + ?Sized>(
                value: &mut Self,
                mut buf: ::bilrost::encoding::Capped<__B>,
                _ctx: ::bilrost::encoding::DecodeContext,
            ) -> Result<(), ::bilrost::DecodeError> {
                let decoded = buf.decode_varint()?;
                let ::core::result::Result::Ok(in_range) = u32::try_from(decoded) else {
                    return ::core::result::Result::Err(
                        ::bilrost::DecodeErrorKind::OutOfDomainValue.into()
                    );
                };
                let ::core::result::Result::Ok(typed) = <Self as ::bilrost::Enumeration>::
                    try_from_number
                (in_range) else {
                    return ::core::result::Result::Err(
                        ::bilrost::DecodeErrorKind::OutOfDomainValue.into()
                    );
                };
                *value = typed;
                ::core::result::Result::Ok(())
            }
        }

        impl #impl_generics
        ::bilrost::encoding::DistinguishedValueDecoder<::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut Self,
                buf: ::bilrost::encoding::Capped<impl ::bilrost::bytes::Buf + ?Sized>,
                ctx: ::bilrost::encoding::RestrictedDecodeContext,
            ) -> Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                ::bilrost::encoding::ValueDecoder::<::bilrost::encoding::General>::decode_value(
                    value,
                    buf,
                    ctx.into_inner(),
                )?;
                ::core::result::Result::Ok(::bilrost::Canonicity::Canonical)
            }
        }

        impl #borrow_generics
        ::bilrost::encoding::ValueBorrowDecoder<'__a, ::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            #[inline(always)]
            fn borrow_decode_value(
                value: &mut Self,
                mut buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> Result<(), ::bilrost::DecodeError> {
                ::bilrost::encoding::ValueDecoder::<::bilrost::encoding::General>::decode_value(
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl #borrow_generics
        ::bilrost::encoding::DistinguishedValueBorrowDecoder<'__a, ::bilrost::encoding::General>
        for #ident #ty_generics #where_clause {
            const CHECKS_EMPTY: bool = false;

            #[inline(always)]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut Self,
                buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                ctx: ::bilrost::encoding::RestrictedDecodeContext,
            ) -> Result<::bilrost::Canonicity, ::bilrost::DecodeError> {
                ::bilrost::encoding::ValueDecoder::<::bilrost::encoding::General>::decode_value(
                    value,
                    buf,
                    ctx.into_inner(),
                )?;
                ::core::result::Result::Ok(::bilrost::Canonicity::Canonical)
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
            // attribute values for enumerations don't have to be exactly numeric literals, but they
            // will need to be used both as a literal-equivalent u32 value and as the match pattern
            // for the variant's corresponding value.
            let Some(expr) = match &attr.meta {
                Meta::List(MetaList { tokens, .. }) => parse2::<Expr>(tokens.clone()).ok(),
                Meta::NameValue(MetaNameValue { value, .. }) => Some(value.clone()),
                _ => None,
            }
            .filter(|expr| {
                // it's a valid expression; also make sure that it parses successfully as a
                // single-variant pattern
                syn::parse::Parser::parse2(Pat::parse_single, expr.to_token_stream()).is_ok()
            }) else {
                bail!(
                    "attribute on enumeration variant must be valid as both an expression and a \
                    match pattern for u32"
                );
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
    impl_generics: &'a Generics,
    ty_generics: TypeGenerics<'a>,
    where_clause: Option<&'a WhereClause>,
    fields: Vec<(Ident, Field)>,
    distinguished: bool,
    borrow_only: bool,
    empty_variant: Option<Ident>,
}

fn preprocess_oneof(input: &DeriveInput) -> Result<PreprocessedOneof, Error> {
    let ident = input.ident.clone();

    let variants = match &input.data {
        Data::Enum(DataEnum { variants, .. }) => variants.clone(),
        Data::Struct(..) => bail!("Oneof can not be derived for a struct"),
        Data::Union(..) => bail!("Oneof can not be derived for a union"),
    };

    let mut unknown_attrs = Vec::new();
    let mut distinguished = false;
    let mut borrow_only = false;
    for attr in bilrost_attrs(input.attrs.clone())? {
        if word_attr(&attr, "distinguished") {
            set_bool(&mut distinguished, "duplicate distinguished attributes")?;
        } else if word_attr(&attr, "borrowed") {
            set_bool(&mut borrow_only, "duplicate borrowed attributes")?;
        } else {
            unknown_attrs.push(attr);
        }
    }

    if !unknown_attrs.is_empty() {
        bail!(
            "unknown attribute(s) for oneof-message: {}",
            quote!(#(#unknown_attrs),*)
        )
    }

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
    let (_, ty_generics, where_clause) = generics.split_for_impl();

    Ok(PreprocessedOneof {
        ident,
        impl_generics: &generics,
        ty_generics,
        where_clause,
        fields,
        distinguished,
        borrow_only,
        empty_variant,
    })
}

fn try_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let input: DeriveInput = parse2(input)?;

    // TODO(widders): support a "message" word attr that converts an enum variant, with possibly
    //  more than one field, to be interpreted as a non-nested message value with its own set of
    //  fields. this still shouldn't support zero fields: the blessed way to encode a zero-field
    //  message enum variant would be to use a variant with a single field with the type ().
    let PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        fields,
        distinguished,
        borrow_only: _, // TODO(widders): impl borrow only
        empty_variant,
    } = preprocess_oneof(&input)?;

    let borrow_generics = append_generic(impl_generics, quote!('__a));

    let encoder_where_clause = append_wheres(where_clause, None, &fields, Encode);
    let owned_decoder_where_clause =
        append_wheres(where_clause, None, &fields, Decode(Owned, Relaxed));
    let borrowed_decoder_where_clause =
        append_wheres(where_clause, None, &fields, Decode(Borrowed, Relaxed));

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

    let mut encode: Vec<TokenStream> = fields
        .iter()
        .map(|(variant_ident, field)| {
            let encode = field.encode(quote!(*value));
            let with_value = field.with_value(quote!(value));
            quote!(#ident::#variant_ident #with_value => { #encode })
        })
        .collect();

    let mut prepend: Vec<TokenStream> = fields
        .iter()
        .map(|(variant_ident, field)| {
            let prepend = field.prepend(quote!(*value));
            let with_value = field.with_value(quote!(value));
            quote!(#ident::#variant_ident #with_value => { #prepend })
        })
        .collect();

    let mut encoded_len: Vec<TokenStream> = fields
        .iter()
        .map(|(variant_ident, field)| {
            let encoded_len = field.encoded_len(quote!(*value));
            let with_value = field.with_value(quote!(value));
            quote!(#ident::#variant_ident #with_value => #encoded_len)
        })
        .collect();

    let encoder_trait;
    let owned_decoder_trait;
    let borrowed_decoder_trait;
    let decode_field_self_arg;
    let decode_field_return_ty;
    let current_tag_ty;
    let current_tag: Vec<TokenStream>;
    let empty_state_impl;
    let some;

    if let Some(empty_ident) = &empty_variant {
        encoder_trait = quote!(Oneof);
        owned_decoder_trait = quote!(OneofDecoder);
        borrowed_decoder_trait = quote!(OneofBorrowDecoder<'__a>);
        decode_field_self_arg = Some(quote!(value: &mut Self,));
        decode_field_return_ty = quote!(());
        some = Some(quote!(::core::option::Option::Some));

        current_tag_ty = quote!(::core::option::Option<u32>);
        current_tag = fields
            .iter()
            .map(|(variant_ident, field)| {
                let tag = field.tags()[0];
                let ignored = field.with_value(quote!(_));
                quote!(#ident::#variant_ident #ignored => ::core::option::Option::Some(#tag))
            })
            .chain([quote!(#ident::#empty_ident => ::core::option::Option::None)])
            .collect();
        encode.push(quote!(#ident::#empty_ident => {}));
        prepend.push(quote!(#ident::#empty_ident => {}));
        encoded_len.push(quote!(#ident::#empty_ident => 0));

        empty_state_impl = Some(quote! {
            impl #impl_generics ::bilrost::encoding::ForOverwrite
            for #ident #ty_generics #encoder_where_clause {
                #[inline]
                fn for_overwrite() -> Self {
                    #ident::#empty_ident
                }
            }

            impl #impl_generics ::bilrost::encoding::EmptyState
            for #ident #ty_generics #encoder_where_clause {
                #[inline]
                fn is_empty(&self) -> bool {
                    matches!(self, #ident::#empty_ident)
                }

                #[inline]
                fn clear(&mut self) {
                    *self = Self::empty();
                }
            }
        });
    } else {
        encoder_trait = quote!(NonEmptyOneof);
        owned_decoder_trait = quote!(NonEmptyOneofDecoder);
        borrowed_decoder_trait = quote!(NonEmptyOneofBorrowDecoder<'__a>);
        decode_field_self_arg = None;
        decode_field_return_ty = quote!(Self);
        some = None;

        // The oneof enum has no "empty" unit variant, so we implement the "non-empty" trait.
        current_tag_ty = quote!(u32);
        current_tag = fields
            .iter()
            .map(|(variant_ident, field)| {
                let tag = field.tags()[0];
                let ignored = field.with_value(quote!(_));
                quote!(#ident::#variant_ident #ignored => #tag)
            })
            .collect();

        empty_state_impl = None;
    };

    let variant_name_arms = fields.iter().map(|(variant_ident, field)| {
        let tag = field.first_tag();
        quote! {
            #tag => (stringify!(#ident), stringify!(#variant_ident)),
        }
    });

    let decode_arms = |lifetime, mode| {
        let arms = fields.iter().map(|(variant_ident, field)| DecoderForOneof {
            ident: &ident,
            variant_ident,
            field,
            lifetime,
            mode,
        });

        quote! {
            match tag {
                #(#arms,)*
                _ => unreachable!(
                    concat!("invalid ", stringify!(#ident), " tag: {}"), tag,
                ),
            }
        }
    };

    let [decode_owned, decode_borrowed] = match empty_variant {
        None => [decode_arms(Owned, Relaxed), decode_arms(Borrowed, Relaxed)],
        Some(ref empty_ident) => [
            decode_arms(Owned, Relaxed),
            decode_arms(Borrowed, Relaxed),
        ]
        .map(|decode| quote! {
            // Guards against colliding oneof field decoding are only evaluated by the Oneof trait,
            // when `oneof_decode_field` is called and the oneof value is already populated.
            // Whichever implementer is responsible for the oneof having an empty state is also
            // responsible for checking for this conflict and returning an error with the
            // appropriate decode error kind and augmenting it with the correct variant name, which
            // can be gotten from `oneof_variant_name` via the trait.
            //
            // In this case we are implementing a Oneof that has an intrinsic empty state via a unit
            // variant, so we insert this guard right into our trait impl. The signature for this
            // method in the `NonEmptyOneof` trait is slightly different to allow for easier nested
            // value implementations, and returns the `Self` type directly in the result instead
            // with no guard.
            //
            // The other purpose this guard serves is to attach an error path detail for the field
            // in the oneof when it bubbles back up through this call. For that reason, also
            // mentioned elsewhere, we structure most of this code to be pretty much one big
            // Result-valued expression to serve this match on the very outside.
            match if let #ident::#empty_ident = value {
                match #decode {
                    ::core::result::Result::Ok(decoded) => {
                        *value = decoded;
                        ::core::result::Result::Ok(())
                    }
                    ::core::result::Result::Err(error) => ::core::result::Result::Err(error),
                }
            } else {
                ::core::result::Result::Err(::bilrost::DecodeError::new(
                    if ::bilrost::encoding::#encoder_trait::oneof_current_tag(value) == #some(tag) {
                        ::bilrost::DecodeErrorKind::UnexpectedlyRepeated
                    } else {
                        ::bilrost::DecodeErrorKind::ConflictingFields
                    }
                ))
            } {
                ::core::result::Result::Err(mut error) => {
                    let (msg, field) =
                        <Self as ::bilrost::encoding::#encoder_trait>::oneof_variant_name(tag);
                    error.push(msg, field);
                    ::core::result::Result::Err(error)
                }
                ok => ok,
            }
        })
    };

    let impls = quote! {
        impl #impl_generics ::bilrost::encoding::#encoder_trait
        for #ident #ty_generics #encoder_where_clause
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

            fn oneof_prepend<__B: ::bilrost::buf::ReverseBuf + ?Sized>(
                &self,
                buf: &mut __B,
                tw: &mut ::bilrost::encoding::TagRevWriter,
            ) {
                match self {
                    #(#prepend,)*
                }
            }

            fn oneof_encoded_len(
                &self,
                tm: &mut impl ::bilrost::encoding::TagMeasurer,
            ) -> usize {
                match self {
                    #(#encoded_len,)*
                }
            }

            fn oneof_current_tag(&self) -> #current_tag_ty {
                match self {
                    #(#current_tag,)*
                }
            }

            fn oneof_variant_name(tag: u32) -> (&'static str, &'static str) {
                match tag {
                    #(#variant_name_arms)*
                    _ => ("", ""),
                }
            }
        }

        impl #impl_generics ::bilrost::encoding::#owned_decoder_trait
        for #ident #ty_generics #owned_decoder_where_clause
        {
            fn oneof_decode_field<__B: ::bilrost::bytes::Buf + ?Sized>(
                #decode_field_self_arg
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                buf: ::bilrost::encoding::Capped<__B>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<#decode_field_return_ty, ::bilrost::DecodeError> {
                #decode_owned
            }
        }

        impl #borrow_generics ::bilrost::encoding::#borrowed_decoder_trait
        for #ident #ty_generics #borrowed_decoder_where_clause
        {
            fn oneof_borrow_decode_field(
                #decode_field_self_arg
                tag: u32,
                wire_type: ::bilrost::encoding::WireType,
                buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                ctx: ::bilrost::encoding::DecodeContext,
            ) -> ::core::result::Result<#decode_field_return_ty, ::bilrost::DecodeError> {
                #decode_borrowed
            }
        }

        #empty_state_impl
    };

    let distinguished_impls = distinguished.then(|| {
        let owned_decoder_trait;
        let borrowed_decoder_trait;
        let relaxed_oneof_trait; // we must reference the parent trait for `oneof_current_tag`
        let decode_field_self_arg;
        let decode_field_return_ty;
        let some; // oneofs that have empty states return Option<u32> from `oneof_current_tag`
        let owned_decoder_where_clause;
        let borrowed_decoder_where_clause;
        if empty_variant.is_some() {
            owned_decoder_trait = quote!(DistinguishedOneofDecoder);
            borrowed_decoder_trait = quote!(DistinguishedOneofBorrowDecoder<'__a>);
            relaxed_oneof_trait = quote!(Oneof);
            decode_field_self_arg = Some(quote!(value: &mut Self,));
            decode_field_return_ty = quote!(::bilrost::Canonicity);
            some = Some(quote!(::core::option::Option::Some));
            [owned_decoder_where_clause, borrowed_decoder_where_clause] =
                [Owned, Borrowed].map(|lifetime| {
                    append_wheres(
                        where_clause,
                        Some(quote!(Self: ::bilrost::encoding::Oneof)),
                        &fields,
                        Decode(lifetime, Distinguished),
                    )
                });
        } else {
            owned_decoder_trait = quote!(NonEmptyDistinguishedOneofDecoder);
            borrowed_decoder_trait = quote!(NonEmptyDistinguishedOneofBorrowDecoder<'__a>);
            relaxed_oneof_trait = quote!(NonEmptyOneof);
            decode_field_self_arg = None;
            decode_field_return_ty = quote!((Self, ::bilrost::Canonicity));
            some = None;
            [owned_decoder_where_clause, borrowed_decoder_where_clause] =
                [Owned, Borrowed].map(|lifetime| {
                    append_wheres(where_clause, None, &fields, Decode(lifetime, Distinguished))
                });
        };

        let [decode_owned, decode_borrowed] = match empty_variant {
            None => [
                decode_arms(Owned, Distinguished),
                decode_arms(Borrowed, Distinguished),
            ],
            Some(empty_ident) => [
                decode_arms(Owned, Distinguished),
                decode_arms(Borrowed, Distinguished),
            ]
            .map(|decode| {
                quote! {
                    // See the note above for details about the colliding field guard.
                    match if let #ident::#empty_ident = value {
                        match #decode {
                            ::core::result::Result::Ok((decoded, canon)) => {
                                *value = decoded;
                                ::core::result::Result::Ok(canon)
                            }
                            ::core::result::Result::Err(error) => {
                                ::core::result::Result::Err(error)
                            },
                        }
                    } else {
                        ::core::result::Result::Err(::bilrost::DecodeError::new(
                            if ::bilrost::encoding::#relaxed_oneof_trait::oneof_current_tag(value)
                                == #some(tag)
                            {
                                ::bilrost::DecodeErrorKind::UnexpectedlyRepeated
                            } else {
                                ::bilrost::DecodeErrorKind::ConflictingFields
                            }
                        ))
                    } {
                        ::core::result::Result::Err(mut error) => {
                            let (msg, field) =
                                <Self as ::bilrost::encoding::#relaxed_oneof_trait>::
                                    oneof_variant_name(tag);
                            error.push(msg, field);
                            ::core::result::Result::Err(error)
                        }
                        ok => ok,
                    }
                }
            }),
        };

        quote! {
            impl #impl_generics ::bilrost::encoding::#owned_decoder_trait
            for #ident #ty_generics #owned_decoder_where_clause
            {
                fn oneof_decode_field_distinguished<__B: ::bilrost::bytes::Buf + ?Sized>(
                    #decode_field_self_arg
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    buf: ::bilrost::encoding::Capped<__B>,
                    ctx: ::bilrost::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<#decode_field_return_ty, ::bilrost::DecodeError> {
                    #decode_owned
                }
            }

            impl #borrow_generics ::bilrost::encoding::#borrowed_decoder_trait
            for #ident #ty_generics #borrowed_decoder_where_clause
            {
                fn oneof_borrow_decode_field_distinguished(
                    #decode_field_self_arg
                    tag: u32,
                    wire_type: ::bilrost::encoding::WireType,
                    buf: ::bilrost::encoding::Capped<&'__a [u8]>,
                    ctx: ::bilrost::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<#decode_field_return_ty, ::bilrost::DecodeError> {
                    #decode_borrowed
                }
            }
        }
    });

    let aliases = encoder_alias_header();
    Ok(quote! {
        const _: () = {
            #aliases

            #impls

            #distinguished_impls
        };
    })
}

/// Oneof decoders have four different cases they may be implemented in: implemented for either
/// NonEmptyOneof or Oneof, and either relaxed or distinguished. The code for these should all be
/// similarly deduplicated.
struct DecoderForOneof<'a> {
    /// The ident of the oneof enum itself
    ident: &'a Ident,
    /// The ident of this variant
    variant_ident: &'a Ident,
    /// The Field struct for this variant
    field: &'a Field,
    /// Decoded ownership lifetime
    lifetime: DecodeLifetime,
    /// Decoding mode
    mode: DecodeMode,
}

impl ToTokens for DecoderForOneof<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident = self.ident;
        let variant_ident = self.variant_ident;
        let field = self.field;
        let tag = field.first_tag();
        let with_new_value = field.with_value(quote!(new_value));
        let decode = field.decode(quote!(&mut new_value), self.lifetime, self.mode);

        // It's important that we spell the whole expression for the decoder matching for oneofs as
        // a single Result expression that never early-returns with `?`; that way when we add guards
        // to the Oneof trait impls (which have natural empty variants, a collision guard, and error
        // attribution) our clause that traces the error location will see every error that occurs,
        // including errors that bubble up from the inner decoders, and those error details can
        // still path down through the oneof variant.
        tokens.append_all(match self.mode {
            Relaxed => quote! {
                #tag => {
                    let mut new_value = ::bilrost::encoding::ForOverwrite::for_overwrite();
                    match #decode {
                        ::core::result::Result::Ok(()) => {
                            ::core::result::Result::Ok(#ident::#variant_ident #with_new_value)
                        },
                        ::core::result::Result::Err(error) => ::core::result::Result::Err(error),
                    }
                }
            },
            Distinguished => quote! {
                #tag => {
                    let mut new_value = ::bilrost::encoding::ForOverwrite::for_overwrite();
                    match #decode {
                        ::core::result::Result::Ok(canon) => ::core::result::Result::Ok((
                            #ident::#variant_ident #with_new_value,
                            canon
                        )),
                        ::core::result::Result::Err(error) => ::core::result::Result::Err(error),
                    }
                }
            },
        })
    }
}

#[proc_macro_derive(Oneof, attributes(bilrost))]
pub fn oneof(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_oneof(input.into()).unwrap().into()
}

#[cfg(test)]
mod test {
    use alloc::format;
    use alloc::string::ToString;

    use quote::quote;

    use crate::{try_enumeration, try_message, try_oneof};

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

        let output = try_message(quote! {
            struct Invalid {
                #[bilrost(tag = "2")]
                a: bool,
                #[bilrost(oneof(1-3))]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("duplicate tags not detected").to_string(),
            "message Invalid has duplicate tag 2"
        );

        let output = try_message(quote! {
            struct Invalid {
                #[bilrost(tag = "10")]
                a: bool,
                #[bilrost(oneof = "5-10")]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("duplicate tags not detected").to_string(),
            "message Invalid has duplicate tag 10"
        );

        // Tags that don't collide with ranges are fine
        _ = try_message(quote! {
            struct Valid {
                #[bilrost(tag = "4")]
                a: bool,
                #[bilrost(oneof(5-10, 1-3))]
                b: Option<super::Whatever>,
            }
        })
        .unwrap();
    }

    #[test]
    fn test_rejects_reserved_message_fields() {
        let output = try_message(quote! {
            #[bilrost(reserved_tags(1, 100))]
            struct Invalid {
                #[bilrost(tag = "1")]
                a: bool,
                #[bilrost(oneof(3-5))]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "message Invalid field a has reserved tag 1"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(4, 55))]
            struct Invalid {
                #[bilrost(tag = "1")]
                a: bool,
                #[bilrost(oneof(3-5))]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "message Invalid field b has reserved tag 4"
        );
    }

    #[test]
    fn test_rejects_oversize_oneof_tag_ranges() {
        let output = try_message(quote! {
            struct Invalid {
                #[bilrost(oneof(1-100))]
                a: SomeOneof,
            }
        });
        assert_eq!(
            format!(
                "{:#}",
                output.expect_err("oversized tag range not detected")
            ),
            "invalid message field Invalid.a: too-large tag range 1-100; use smaller ranges"
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
                #[bilrost(tag = "0")] bool,
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
                #[bilrost(tag(0))] bool,
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
                #[bilrost(0)] bool,
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
                #[bilrost = 2]
                B,
                #[bilrost(3)]
                C,
                #[bilrost(SomeType::<T>::SOME_CONSTANT)]
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
                #[bilrost = 2]
                B,
                #[bilrost(3)]
                C,
                #[bilrost(SomeType::<T>::SOME_CONSTANT)]
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

    #[test]
    fn test_accepts_distinguished_and_borrowed_messages() {
        _ = try_message(quote!(
            #[bilrost(distinguished, borrowed)]
            struct DistinguishedBorrowedMessage {
                name: &str,
            }
        ))
        .unwrap();
        _ = try_message(quote!(
            #[bilrost(distinguished, borrowed)]
            enum DistinguishedBorrowedOneof {
                Empty,
                #[bilrost(1)]
                Name(&str),
            }
        ))
        .unwrap();
    }

    #[test]
    fn test_accepts_distinguished_and_borrowed_oneofs() {
        _ = try_oneof(quote!(
            #[bilrost(distinguished, borrowed)]
            enum DistinguishedBorrowedOneof {
                #[bilrost(1)]
                Name(&str),
            }
        ))
        .unwrap();
    }

    #[test]
    fn test_rejects_duplicated_message_attrs() {
        let output = try_message(quote!(
            #[bilrost(distinguished, distinguished)]
            struct DistinguishedBorrowedMessage {
                name: &str,
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated distinguished attrs not detected")
                .to_string(),
            "duplicate distinguished attributes"
        );
        let output = try_message(quote!(
            #[bilrost(borrowed, distinguished, borrowed)]
            struct DistinguishedBorrowedMessage {
                name: &str,
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated borrowed attrs not detected")
                .to_string(),
            "duplicate borrowed attributes"
        );

        let output = try_message(quote!(
            #[bilrost(distinguished, distinguished)]
            enum DistinguishedBorrowedOneof {
                Empty,
                #[bilrost(1)]
                Name(&str),
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated distinguished attrs not detected")
                .to_string(),
            "duplicate distinguished attributes"
        );
        let output = try_message(quote!(
            #[bilrost(borrowed, distinguished, borrowed)]
            enum DistinguishedBorrowedOneof {
                Empty,
                #[bilrost(1)]
                Name(&str),
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated borrowed attrs not detected")
                .to_string(),
            "duplicate borrowed attributes"
        );
    }

    #[test]
    fn test_rejects_duplicated_oneof_attrs() {
        let output = try_message(quote!(
            #[bilrost(distinguished, distinguished)]
            enum DistinguishedBorrowedOneof {
                #[bilrost(1)]
                Name(&str),
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated distinguished attrs not detected")
                .to_string(),
            "duplicate distinguished attributes"
        );
        let output = try_message(quote!(
            #[bilrost(borrowed, distinguished, borrowed)]
            enum DistinguishedBorrowedOneof {
                #[bilrost(1)]
                Name(&str),
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated borrowed attrs not detected")
                .to_string(),
            "duplicate borrowed attributes"
        );
    }
}
