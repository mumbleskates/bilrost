use crate::attrs::{bilrost_attrs, TagList};
use crate::crate_name;
use crate::field::traits::{DecodeLifetime, DecodeMode, FieldBearer, Tagged, WhereFor};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use core::iter::repeat;
use core::mem::take;
use core::ops::Deref;
use eyre::{bail, eyre as err, Report as Error};
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{Attribute, Type};

mod ignored;
mod oneof;
pub mod traits;
mod value;

pub use value::OneofVariant;

#[derive(Clone)]
pub struct Field {
    ident: TokenStream,
    content: MessageFieldContent,
}

#[derive(Clone)]
enum MessageFieldContent {
    /// A single-value message field.
    Value(Box<value::MessageField>),
    /// A oneof field.
    Oneof(Box<oneof::OneofInclusion>),
    /// An ignored field.
    Ignored(Box<ignored::IgnoredField>),
}
use MessageFieldContent::*;

/// Processes message fields from a vec of syn::Field, validating their tags against the given
/// reserved tag list and each other.
pub fn parse_message_fields(
    fields: syn::Fields,
    reserved: Option<TagList>,
) -> Result<Vec<Field>, Error> {
    let mut next_tag = Some(match fields {
        // tuple structs begin field numbering at zero
        syn::Fields::Unnamed(..) => 0,
        // regular structs begin field numbering at one
        _ => 1,
    });

    let unsorted_fields = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let field_ident = field
                .ident
                .as_ref()
                .map(ToTokens::to_token_stream)
                .unwrap_or_else(|| {
                    let index = syn::Index::from(index);
                    quote!(#index)
                });
            let field = Field::new(&field_ident, &field.ty, &field.attrs, next_tag)
                .map_err(|e| err!("invalid field {field_ident}: {e}"))?;
            if !field.is_ignored() {
                next_tag = field.last_tag().checked_add(1);
            }
            Ok(field)
        })
        .collect::<Result<Vec<_>, Error>>()?;

    // Index all fields by their tag(s) and check them against the forbidden tag ranges
    let all_tags: BTreeMap<u32, &Field> = unsorted_fields
        .iter()
        .flat_map(|field| field.tags().into_iter().zip(repeat(field)))
        .collect();
    for reserved_range in reserved.unwrap_or_default().iter_tag_ranges() {
        if let Some((forbidden_tag, bad_field)) = all_tags.range(reserved_range).next() {
            let field_ident = bad_field.ident();
            bail!("field {field_ident} has reserved tag {forbidden_tag}");
        }
    }

    // Find any duplicates in the full list of tags
    if let Some((duplicated_tag, _)) = unsorted_fields
        .iter()
        .flat_map(|field| field.tags())
        .sorted_unstable()
        .tuple_windows()
        .find(|(a, b)| a == b)
    {
        bail!("multiple fields have tag {duplicated_tag}")
    };

    Ok(unsorted_fields)
}

/// If there can never be a tag delta larger than 31, field keys will never be more than 1 byte.
pub fn tag_measurer(for_these: &[impl Tagged]) -> TokenStream {
    let crate_ = crate_name();
    if matches!(for_these.iter().flat_map(Tagged::tags).max(), Some(max_tag) if max_tag < 32) {
        quote!(#crate_::encoding::TrivialTagMeasurer)
    } else {
        quote!(#crate_::encoding::RuntimeTagMeasurer)
    }
}

impl Field {
    /// Creates a new `Field` from field attributes
    ///
    /// If the meta items are invalid, an error will be returned.
    /// If the field should be ignored, `None` is returned.
    pub fn new(
        ident: &TokenStream,
        ty: &Type,
        attrs: &[Attribute],
        inferred_tag: Option<u32>,
    ) -> Result<Field, Error> {
        let attrs = bilrost_attrs(attrs)?;

        Ok(Field {
            content: if let Some(field) = ignored::IgnoredField::new(ty, &attrs)? {
                Ignored(field)
            } else if let Some(field) = oneof::OneofInclusion::new(ty, &attrs)? {
                Oneof(field)
            } else {
                Value(value::MessageField::new(ty, attrs, inferred_tag)?)
            },
            ident: ident.clone(),
        })
    }

    /// Returns the ident of the field within its struct or variant.
    pub fn ident(&self) -> &TokenStream {
        &self.ident
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self.content, Ignored(..))
    }

    pub fn has_enumeration_type(&self) -> bool {
        let Value(scalar) = &self.content else {
            return false;
        };
        scalar.has_enumeration_type()
    }

    /// Returns a statement that statically asserts the type of this field has the correct tags in
    /// its accepted fields, if it is a oneof inclusion.
    pub fn tag_list_guard(&self) -> Option<TokenStream> {
        let Oneof(field) = &self.content else {
            return None; // only oneof inclusions have lists of tags that need assertions
        };
        let crate_ = crate_name();
        let mut tags = self.tags();
        tags.sort();
        let oneof_ty = &field.ty;
        let oneof_ty_name = oneof_ty.to_token_stream().to_string();
        let field_name = self.ident.to_string();
        let description =
            format!("tags don't match for oneof field {field_name} with type {oneof_ty_name}");
        let description = description.as_str();
        // Static assertion pattern borrowed from static_assertions crate.
        Some(quote!(
            #crate_::assert_tags_are_equal(
                #description,
                <#oneof_ty as #crate_::encoding::Oneof>::FIELD_TAGS,
                &[#(#tags),*],
            );
        ))
    }

    /// Returns a statement which encodes the field.
    pub fn encode(&self, instance: impl ToTokens) -> TokenStream {
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        match &self.content {
            Value(scalar) => scalar.encode(target),
            Oneof(oneof) => oneof.encode(target),
            Ignored(..) => panic!("cannot encode ignored field"),
        }
    }

    /// Returns a statement which prepends the field.
    pub fn prepend(&self, instance: impl ToTokens) -> TokenStream {
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        match &self.content {
            Value(scalar) => scalar.prepend(target),
            Oneof(oneof) => oneof.prepend(target),
            Ignored(..) => panic!("cannot prepend ignored field"),
        }
    }

    /// Returns an expression which evaluates to the result of decoding a value into the field.
    pub fn decode(
        &self,
        instance: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        match &self.content {
            Value(scalar) => scalar.decode(target, lifetime, mode),
            Oneof(oneof) => oneof.decode(target, lifetime, mode),
            Ignored(..) => panic!("canot decode ignored field"),
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field.
    pub fn encoded_len(&self, instance: impl ToTokens) -> TokenStream {
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        match &self.content {
            Value(scalar) => scalar.encoded_len(target),
            Oneof(oneof) => oneof.encoded_len(target),
            Ignored(..) => panic!("cannot get encode length of ignored field"),
        }
    }

    /// Returns an expression which initializes the field's type with its encoding with a guaranteed
    /// empty value.
    pub fn empty(&self) -> TokenStream {
        let ident = &self.ident;
        let init = match &self.content {
            Value(scalar) => scalar.empty(),
            Oneof(oneof) => oneof.empty(),
            Ignored(ignored) => ignored.initialize(),
        };
        quote!(#ident: #init)
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    pub fn is_empty(&self, instance: TokenStream) -> TokenStream {
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        match &self.content {
            Value(scalar) => scalar.is_empty(target),
            Oneof(oneof) => oneof.is_empty(target),
            Ignored(..) => panic!("cannot detect empty on ignored field"),
        }
    }

    /// Returns an expression which resets the field's value to empty with its encoding.
    pub fn clear(&self, instance: TokenStream) -> TokenStream {
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        match &self.content {
            Value(scalar) => scalar.clear(target),
            Oneof(oneof) => oneof.clear(target),
            Ignored(..) => panic!("cannot clear ignored field"),
        }
    }

    /// If the field is a oneof, returns an expression which evaluates to an Option<u32> of the tag
    /// of the (maybe) present field in the oneof. Panics if the field is not a oneof.
    pub fn current_tag(&self, instance: impl ToTokens) -> TokenStream {
        let Oneof(field) = &self.content else {
            panic!("tried to use a value field as a oneof")
        };
        let ident = &self.ident;
        let target = quote!(#instance.#ident);
        field.current_tag(target)
    }

    pub fn methods(&self) -> Option<TokenStream> {
        match &self.content {
            Value(scalar) => scalar.methods(&self.ident),
            _ => None,
        }
    }
}

impl FieldBearer for Field {
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        match &self.content {
            Value(field) => field.where_terms(purpose),
            Oneof(field) => field.where_terms(purpose),
            Ignored(field) => field.where_terms(purpose),
        }
    }
}

impl Tagged for Field {
    fn tags(&self) -> Vec<u32> {
        match &self.content {
            Value(scalar) => scalar.tags(),
            Oneof(oneof) => oneof.tags(),
            Ignored(..) => vec![],
        }
    }
}

/// Helper type to ensure a value is used at runtime.
struct MustMove<T>(Option<T>);

impl<T> MustMove<T> {
    fn new(t: T) -> Self {
        Self(Some(t))
    }

    fn into_inner(mut self) -> T {
        take(&mut self.0).expect("MustMove value was moved out twice")
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
        self.0
            .as_ref()
            .expect("MustMove dereferenced after the value was moved out")
    }
}

/// Represents fields that are organized into sortable groups that can be efficiently emitted in
/// guaranteed tag order.
pub struct MessageFieldsSorted<'a> {
    chunks: Vec<FieldChunk<'a>>,
    tag_measurer_ty: TokenStream,
}

enum FieldChunk<'a> {
    // A field that does not need to be sorted
    AlwaysOrdered(&'a Field),
    // A set of fields that must be sorted before emitting
    SortGroup(Vec<SortGroupPart<'a>>),
}
use FieldChunk::*;

enum SortGroupPart<'a> {
    // A set of fields that can be sorted by any of their tags, as they are always contiguous
    Contiguous(Vec<&'a Field>),
    // A oneof field that needs to be sorted based on its current value's tag
    OneofPart(&'a Field),
}
use SortGroupPart::*;

impl<'a> MessageFieldsSorted<'a> {
    /// Sorts a vec of unsorted fields into discrete chunks that may be ordered together at runtime to
    /// ensure that all their fields are encoded in sorted order.
    pub fn new(unsorted_fields: &'a [Field]) -> Self {
        let mut chunks: Vec<FieldChunk> = vec![];
        let mut fields = unsorted_fields
            .iter()
            .sorted_unstable_by_key(|field| field.first_tag())
            .peekable();
        // Current vecs we are building for FieldChunk::SortGroup and SortGroupPart::Contiguous
        let mut current_contiguous_group: Vec<&Field> = vec![];
        let mut current_sort_group: Vec<SortGroupPart> = vec![];
        // Set of oneof tags that are interspersed with other fields, so we know when we're able to
        // put multiple fields into the same ordered group.
        let mut sort_group_oneof_tags = BTreeSet::<u32>::new();
        while let (Some(this_field), next_field) = (fields.next(), fields.peek()) {
            // The following logic is a bit involved, so ensure that we can't forget to use the values.
            let this_field = MustMove::new(this_field);
            let field = this_field.deref();
            let first_tag = field.first_tag();
            let last_tag = field.last_tag();
            // Check if this field is a oneof with tags interleaved with other fields' tags. If true,
            // this field must always be emitted into a sort group.
            let overlaps =
                matches!(next_field, Some(next_field) if last_tag > next_field.first_tag());
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
                    current_sort_group.push(OneofPart(this_field.into_inner()));
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
                    current_sort_group.push(OneofPart(this_field.into_inner()));
                } else {
                    // This field doesn't overlap with anything so we just add it to the current group
                    // of already-ordered fields.
                    if let Some(previous_field) = current_contiguous_group.last() {
                        if sort_group_oneof_tags
                            .range(previous_field.last_tag()..=first_tag)
                            .next()
                            .is_some()
                        {
                            // One of the overlapping oneofs in this sort group may emit a tag between
                            // the previous field in the ordered group and this one, so split the
                            // ordered group here.
                            current_sort_group
                                .push(Contiguous(take(&mut current_contiguous_group)));
                        }
                    }
                    current_contiguous_group.push(this_field.into_inner());
                }
            } else {
                // We are not already in a sort group.
                if overlaps {
                    // This field requires sorting with others. Begin a new sort group.
                    sort_group_oneof_tags.clear();
                    sort_group_oneof_tags.extend(field.tags());
                    current_sort_group.push(OneofPart(this_field.into_inner()));
                } else {
                    // This field doesn't need to be sorted.
                    chunks.push(AlwaysOrdered(this_field.into_inner()));
                }
            }

            // MSRV: can't use .last()
            if let Some(&sort_group_end) = sort_group_oneof_tags.iter().next_back() {
                if !matches!(
                    next_field,
                    Some(next_field) if next_field.first_tag() < sort_group_end
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

        Self {
            chunks,
            tag_measurer_ty: tag_measurer(unsorted_fields),
        }
    }

    pub fn encoded_len(&self, target: impl ToTokens) -> TokenStream {
        let tag_measurer_ty = &self.tag_measurer_ty;
        let chunks = self.chunks.iter().map(|chunk| match chunk {
            AlwaysOrdered(field) => field.encoded_len(&target),
            SortGroup(parts) => {
                // TODO: consider altering these to unconditionally populate the start of the array
                //  with guaranteed fields on initialization, then only conditionally appending the
                //  oneof fields to the end. leaning on the sort while cheapening the initialization
                //  may be faster overall when there are a lot of fields interleaved in between the
                //  gaps in oneof numbering.
                let parts: Vec<TokenStream> = parts
                    .iter()
                    .map(|part| match part {
                        Contiguous(fields) => {
                            let Some(first_field) = fields.first() else {
                                panic!("empty contiguous field group");
                            };
                            let first_tag = first_field.first_tag();
                            let each_len = fields
                                .iter()
                                .map(|field| field.encoded_len(quote!(instance)));
                            quote! {
                                parts[nparts] = (
                                    #first_tag,
                                    ::core::option::Option::Some(|instance, tm| {
                                        0 #(+ #each_len)*
                                    }),
                                );
                                nparts += 1;
                            }
                        }
                        OneofPart(field) => {
                            let current_tag = field.current_tag(&target);
                            let encoded_len = field.encoded_len(quote!(instance));
                            quote! {
                                if let ::core::option::Option::Some(tag) = #current_tag {
                                    parts[nparts] = (
                                        tag,
                                        ::core::option::Option::Some(|instance, tm| {
                                            #encoded_len
                                        }),
                                    );
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
                            total_len += ::core::option::Option::unwrap(*len_func_option)(#target, tm)
                        }
                        total_len
                    }
                }
            }
        });
        quote! {
            {
                let tm = &mut #tag_measurer_ty::new();
                0 #(+ #chunks)*
            }
        }
    }

    pub fn encode(&self, target: impl ToTokens) -> TokenStream {
        let crate_ = crate_name();
        let chunks = self.chunks.iter().map(|chunk| match chunk {
            AlwaysOrdered(field) => field.encode(&target),
            SortGroup(parts) => {
                let parts: Vec<TokenStream> = parts
                    .iter()
                    .map(|part| match part {
                        Contiguous(fields) => {
                            let Some(first_field) = fields.first() else {
                                panic!("empty contiguous field group");
                            };
                            let first_tag = first_field.first_tag();
                            let each_field =
                                fields.iter().map(|field| field.encode(quote!(instance)));
                            quote! {
                                parts[nparts] = (
                                    #first_tag,
                                    ::core::option::Option::Some(|instance, buf, tw| {
                                        #(#each_field)*
                                    }),
                                );
                                nparts += 1;
                            }
                        }
                        OneofPart(field) => {
                            let current_tag = field.current_tag(&target);
                            let encode = field.encode(quote!(instance));
                            quote! {
                                if let ::core::option::Option::Some(tag) = #current_tag {
                                    parts[nparts] = (
                                        tag,
                                        ::core::option::Option::Some(|instance, buf, tw| {
                                            #encode
                                        }),
                                    );
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
                                       fn(&Self, &mut __B, &mut #crate_::encoding::TagWriter)
                                   >);
                            #max_parts
                        ];
                        let mut nparts = 0usize;
                        #(#parts)*
                        let parts = &mut parts[..nparts];
                        parts.sort_unstable_by_key(|(tag, _)| *tag);
                        for (_, encode_func) in parts {
                            (encode_func.unwrap())(#target, buf, tw);
                        }
                    }
                }
            }
        });
        quote! {
            {
                let tw = &mut #crate_::encoding::TagWriter::new();
                #(#chunks)*
            }
        }
    }

    pub fn prepend(&self, target: impl ToTokens) -> TokenStream {
        let crate_ = crate_name();
        let chunks = self.chunks.iter().rev().map(|chunk| match chunk {
            AlwaysOrdered(field) => field.prepend(&target),
            SortGroup(parts) => {
                let parts: Vec<TokenStream> = parts
                    .iter()
                    .rev()
                    .map(|part| match part {
                        Contiguous(fields) => {
                            let Some(first_field) = fields.first() else {
                                panic!("empty contiguous field group");
                            };
                            let first_tag = first_field.first_tag();
                            let each_field = fields
                                .iter()
                                .rev()
                                .map(|field| field.prepend(quote!(instance)));
                            quote! {
                                parts[nparts] = (
                                    #first_tag,
                                    ::core::option::Option::Some(|instance, buf, tw| {
                                        #(#each_field)*
                                    }),
                                );
                                nparts += 1;
                            }
                        }
                        OneofPart(field) => {
                            let current_tag = field.current_tag(&target);
                            let prepend = field.prepend(quote!(instance));
                            quote! {
                                if let ::core::option::Option::Some(tag) = #current_tag {
                                    parts[nparts] = (
                                        tag,
                                        ::core::option::Option::Some(|instance, buf, tw| {
                                            #prepend
                                        }),
                                    );
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
                                       fn(&Self, &mut __B, &mut #crate_::encoding::TagRevWriter)
                                   >);
                            #max_parts
                        ];
                        let mut nparts = 0usize;
                        #(#parts)*
                        let parts = &mut parts[..nparts];
                        parts.sort_unstable_by_key(|(tag, _)| ::core::cmp::Reverse(*tag));
                        for (_, prepend_func) in parts {
                            (prepend_func.unwrap())(#target, buf, tw);
                        }
                    }
                }
            }
        });
        quote! {
            {
                let tw = &mut #crate_::encoding::TagRevWriter::new();
                #(#chunks)*
                tw.finalize(buf);
            }
        }
    }
}
