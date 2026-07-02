use crate::attrs::{bilrost_attrs, TagList};
use crate::crate_name;
use crate::field::traits::{DecodeLifetime, DecodeMode, FieldBearer, Tagged, WhereFor};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::iter::repeat;
use core::mem::take;
use core::ops::Deref;
use core::{iter, slice};
use eyre::{bail, eyre as err, Report as Error};
use itertools::{repeat_n, Either, Itertools};
use proc_macro2::{Ident, TokenStream};
use quote::{quote, ToTokens};
use syn::{parse_str, Attribute, Type};

mod ignored;
mod oneof;
pub mod traits;
mod value;

pub use ignored::{initializer_class_definition, InitMode};
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

/// Returns a string of the given ident with any leading `r#` removed.
fn ident_string(ident: &impl ToString) -> String {
    let s = ident.to_string();
    if let Some(after_hash) = s.strip_prefix("r#") {
        after_hash.to_string()
    } else {
        s
    }
}

/// Processes message fields from a vec of syn::Field, validating their tags against the given
/// reserved tag list and each other.
pub fn parse_message_fields(
    fields: syn::Fields,
    init_mode: InitMode,
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
            let field = Field::new(
                &field_ident,
                &field.ty,
                &field.attrs,
                next_tag,
                init_mode.clone(),
            )
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
pub fn tag_measurer<T: Tagged>(for_these: impl IntoIterator<Item = T>) -> TokenStream {
    let crate_ = crate_name();
    if matches!(for_these.into_iter().flat_map(|t| t.tags()).max(), Some(max_tag) if max_tag < 32) {
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
        init_mode: InitMode,
    ) -> Result<Field, Error> {
        let attrs = bilrost_attrs(attrs)?;

        Ok(Field {
            content: if let Some(field) = ignored::IgnoredField::new(ty, &attrs, init_mode)? {
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

    pub fn ty(&self) -> &Type {
        match &self.content {
            Value(value) => value.ty(),
            Oneof(oneof) => oneof.ty(),
            Ignored(_) => panic!("ignored fields have no type"),
        }
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self.content, Ignored(..))
    }

    pub fn ignored_and_uses_struct_update_syntax(&self) -> bool {
        matches!(&self.content, Ignored(inner) if inner.uses_struct_update_syntax())
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
        let description = format!(
            "tags don't match for oneof field {field_name} with type {oneof_ty_name}",
            field_name = self.ident,
            oneof_ty_name = oneof_ty.to_token_stream(),
        );
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
    pub fn encode(&self, instance: &FieldTarget) -> TokenStream {
        let target = instance.const_field_ref(self);
        match &self.content {
            Value(scalar) => scalar.encode(target),
            Oneof(oneof) => oneof.encode(target),
            Ignored(..) => panic!("cannot encode ignored field"),
        }
    }

    /// Returns a statement which prepends the field.
    pub fn prepend(&self, instance: &FieldTarget) -> TokenStream {
        let target = instance.const_field_ref(self);
        match &self.content {
            Value(scalar) => scalar.prepend(target),
            Oneof(oneof) => oneof.prepend(target),
            Ignored(..) => panic!("cannot prepend ignored field"),
        }
    }

    /// Returns an expression which evaluates to the result of decoding a value into the field.
    pub fn decode(
        &self,
        instance: &FieldTarget,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let target = instance.mut_field_ref(self);
        match &self.content {
            Value(scalar) => scalar.decode(target, lifetime, mode),
            Oneof(oneof) => oneof.decode(target, lifetime, mode),
            Ignored(..) => panic!("canot decode ignored field"),
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field.
    pub fn encoded_len(&self, instance: &FieldTarget) -> TokenStream {
        let target = instance.const_field_ref(self);
        match &self.content {
            Value(scalar) => scalar.encoded_len(target),
            Oneof(oneof) => oneof.encoded_len(target),
            Ignored(..) => panic!("cannot get encode length of ignored field"),
        }
    }

    /// Returns an expression which initializes the field's type with its encoding with a guaranteed
    /// empty value.
    pub fn empty(&self, variant_tag: Option<u32>) -> Option<TokenStream> {
        match &self.content {
            Value(scalar) => Some(scalar.empty()),
            Oneof(oneof) => Some(oneof.empty()),
            Ignored(ignored) => ignored.initialize(variant_tag, &self.ident),
        }
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    pub fn is_empty(&self, instance: &FieldTarget) -> TokenStream {
        let target = instance.const_field_ref(self);
        match &self.content {
            Value(scalar) => scalar.is_empty(target),
            Oneof(oneof) => oneof.is_empty(target),
            Ignored(..) => panic!("cannot detect empty on ignored field"),
        }
    }

    /// Returns an expression which resets the field's value to empty with its encoding.
    pub fn clear(&self, instance: &FieldTarget) -> TokenStream {
        let target = instance.mut_field_ref(self);
        match &self.content {
            Value(scalar) => scalar.clear(target),
            Oneof(oneof) => oneof.clear(target),
            Ignored(..) => panic!("cannot clear ignored field"),
        }
    }

    /// If the field is a oneof, returns an expression which evaluates to an Option<u32> of the tag
    /// of the (maybe) present field in the oneof. Panics if the field is not a oneof.
    pub fn current_tag(&self, instance: &FieldTarget) -> TokenStream {
        let Oneof(field) = &self.content else {
            panic!("tried to use a value field as a oneof")
        };
        let target = instance.const_field_ref(self);
        field.current_tag(target)
    }

    pub fn methods(&self) -> Option<TokenStream> {
        match &self.content {
            Value(scalar) => scalar.methods(&self.ident),
            _ => None,
        }
    }

    /// Defines the initializer method for an ignored field in a message or message variant
    pub fn initializer_method(&self, variant_tag: Option<u32>) -> Option<TokenStream> {
        if let Ignored(ignored) = &self.content {
            ignored.initializer_method(variant_tag, &self.ident)
        } else {
            None
        }
    }

    pub fn schema(&self) -> Option<TokenStream> {
        match &self.content {
            Value(scalar) => Some(scalar.schema(&self.ident.to_string())),
            Oneof(oneof) => Some(oneof.schema(&self.ident.to_string())),
            Ignored(..) => None,
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

impl Tagged for &Field {
    fn tags(&self) -> Vec<u32> {
        (**self).tags()
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

struct SortGroupConfig<FC, FO> {
    /// Sort direction: ascending or descending tag numbers
    direction: Direction,
    /// Accepts an iterator of fields and outputs the generated code for a contiguous group of
    /// guaranteed fields
    contiguous_part_fn: FC,
    /// Accepts one field and outputs the generated code for a oneof field
    oneof_part_fn: FO,
    /// The type name of the extra data attached to the tags
    part_fn_ty: TokenStream,
    /// Pasted at the end of the block, after everything. `parts` will be a sorted slice of
    /// `(u32, #part_fn_ty)` containing all the fields that were initialized via the contiguous
    /// and oneof code generation
    invoke_parts: TokenStream,
}

type ReversibleFields<'a> =
    Either<slice::Iter<'a, &'a Field>, iter::Rev<slice::Iter<'a, &'a Field>>>;

/// Helper that can conditionally reverse iterators.
#[derive(Copy, Clone)]
enum Direction {
    Forward,
    Reverse,
}

impl Direction {
    fn align<T, I>(self, iterable: T) -> Either<I, iter::Rev<I>>
    where
        T: IntoIterator<IntoIter = I>,
        I: DoubleEndedIterator,
    {
        match self {
            Direction::Forward => Either::Left(iterable.into_iter()),
            Direction::Reverse => Either::Right(iterable.into_iter().rev()),
        }
    }
}

/// Implements guaranteed field ordering
fn process_sort_groups<FC, FO>(
    parts: &[SortGroupPart],
    instance: &FieldTarget,
    config: SortGroupConfig<FC, FO>,
) -> TokenStream
where
    FC: Fn(ReversibleFields) -> TokenStream,
    FO: Fn(&Field) -> TokenStream,
{
    let SortGroupConfig {
        direction,
        contiguous_part_fn,
        oneof_part_fn,
        part_fn_ty,
        invoke_parts,
    } = config;
    // `guaranteed_parts` will initialize the array slots for all the fields that we will definitely
    // try to encode
    let guaranteed_parts: Vec<_> = direction
        .align(parts)
        .flat_map(|part| match part {
            Contiguous(fields) => {
                let Some(first_field) = fields.first() else {
                    panic!("empty contiguous field group");
                };
                let first_tag = first_field.first_tag();
                let closure = contiguous_part_fn(direction.align(fields));
                Some(quote! { (#first_tag, ::core::option::Option::Some(#closure)) })
            }
            _ => None,
        })
        .collect();
    // `populate_oneof_parts` conditionally populates the trailing end of the array with oneof
    // fields and their actual tag so they can be sorted into the correct order
    let populate_oneof_parts: Vec<_> = direction
        .align(parts)
        .flat_map(|part| match part {
            OneofPart(field) => {
                let current_tag = field.current_tag(instance);
                let closure = oneof_part_fn(field);
                Some(quote! {
                    if let ::core::option::Option::Some(tag) = #current_tag {
                        parts[nparts] = (tag, ::core::option::Option::Some(#closure));
                        nparts += 1;
                    }
                })
            }
            _ => None,
        })
        .collect();
    // we need to fill in the empty slots at the end of the array for each oneof item that may or
    // may not be included
    let filler_none = quote!((0u32, ::core::option::Option::None));
    let non_guaranteed_filler = repeat_n(&filler_none, populate_oneof_parts.len());
    let num_guaranteed_parts = guaranteed_parts.len();
    let max_parts = parts.len();
    let sort_tag_expr = match direction {
        Direction::Forward => quote!(*tag),
        Direction::Reverse => quote!(::core::cmp::Reverse(*tag)),
    };
    // give the field target an iterator of all fields in this sort group in case it needs to
    // assemble a temporary struct with references to them for the function type
    let prelude = instance.prelude_for(
        parts
            .iter()
            .flat_map(|part| match part {
                Contiguous(fields) => fields.as_slice(),
                OneofPart(field) => slice::from_ref(field),
            })
            .cloned(),
    );
    quote! {
        {
            #prelude
            let mut parts: [(u32, ::core::option::Option<#part_fn_ty>); #max_parts] = [
                #(#guaranteed_parts,)*
                #(#non_guaranteed_filler,)*
            ];
            let mut nparts = #num_guaranteed_parts;
            #(#populate_oneof_parts)*
            let parts = &mut parts[..nparts];
            <[_]>::sort_unstable_by_key(parts, |(tag, _)| #sort_tag_expr);
            #invoke_parts
        }
    }
}

impl<'a> MessageFieldsSorted<'a> {
    /// Sorts a vec of unsorted fields into discrete chunks that may be ordered together at runtime
    /// to ensure that all their fields are encoded in sorted order.
    pub fn new(unsorted_fields: impl IntoIterator<Item = &'a Field>) -> Self {
        let mut chunks: Vec<FieldChunk> = vec![];
        let mut fields = unsorted_fields
            .into_iter()
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
            tag_measurer_ty: tag_measurer(fields),
        }
    }

    pub fn new_filtering_ignored(unsorted_fields: impl IntoIterator<Item = &'a Field>) -> Self {
        Self::new(
            unsorted_fields
                .into_iter()
                .filter(|field| !field.is_ignored()),
        )
    }

    pub fn encoded_len(&self, instance: &FieldTarget) -> TokenStream {
        let tag_measurer_ty = &self.tag_measurer_ty;
        let sort_group_instance;
        let part_fn_instance_ty;
        if instance.has_instance() {
            sort_group_instance = instance.clone();
            part_fn_instance_ty = quote!(Self);
        } else {
            sort_group_instance = FieldTarget::RefsInstance(quote!(refs));
            part_fn_instance_ty = quote!(__BilrostRefs);
        }
        let sort_group_self = sort_group_instance.self_expr();
        let renamed = sort_group_instance.rename(quote!(instance));
        let chunks = self.chunks.iter().map(|chunk| match chunk {
            AlwaysOrdered(field) => field.encoded_len(instance),
            SortGroup(parts) => process_sort_groups(
                parts,
                &sort_group_instance,
                SortGroupConfig {
                    direction: Direction::Forward,
                    contiguous_part_fn: |fields: ReversibleFields| {
                        let each_len = fields.map(|field| field.encoded_len(&renamed));
                        quote! {
                            |instance, tm| { 0 #(+ #each_len)* }
                        }
                    },
                    oneof_part_fn: |field: &Field| {
                        let encoded_len = field.encoded_len(&renamed);
                        quote! {
                            |instance, tm| { #encoded_len }
                        }
                    },
                    part_fn_ty: quote!(fn(&#part_fn_instance_ty, &mut #tag_measurer_ty) -> usize),
                    invoke_parts: quote! {
                        let mut total_len = 0usize;
                        for (_, len_func) in parts {
                            total_len += (len_func.unwrap())(#sort_group_self, tm)
                        }
                        total_len
                    },
                },
            ),
        });
        quote! {
            {
                let tm = &mut #tag_measurer_ty::new();
                0 #(+ #chunks)*
            }
        }
    }

    pub fn encode(&self, instance: &FieldTarget) -> TokenStream {
        let crate_ = crate_name();
        let sort_group_instance;
        let part_fn_instance_ty;
        if instance.has_instance() {
            sort_group_instance = instance.clone();
            part_fn_instance_ty = quote!(Self);
        } else {
            sort_group_instance = FieldTarget::RefsInstance(quote!(refs));
            part_fn_instance_ty = quote!(__BilrostRefs);
        }
        let sort_group_self = sort_group_instance.self_expr();
        let renamed = sort_group_instance.rename(quote!(instance));
        let chunks = self.chunks.iter().map(|chunk| match chunk {
            AlwaysOrdered(field) => field.encode(instance),
            SortGroup(parts) => process_sort_groups(
                parts,
                &sort_group_instance,
                SortGroupConfig {
                    direction: Direction::Forward,
                    contiguous_part_fn: |fields: ReversibleFields| {
                        let each_field = fields.map(|field| field.encode(&renamed));
                        quote! {
                            |instance, buf, tw| { #(#each_field)* }
                        }
                    },
                    oneof_part_fn: |field: &Field| {
                        let encode = field.encode(&renamed);
                        quote! {
                            |instance, buf, tw| { #encode }
                        }
                    },
                    part_fn_ty: quote!(
                        fn(&#part_fn_instance_ty, &mut __B, &mut #crate_::encoding::TagWriter)
                    ),
                    invoke_parts: quote! {
                        for (_, encode_func) in parts {
                            (encode_func.unwrap())(#sort_group_self, buf, tw);
                        }
                    },
                },
            ),
        });
        quote! {
            {
                let tw = &mut #crate_::encoding::TagWriter::new();
                #(#chunks)*
            }
        }
    }

    pub fn prepend(&self, instance: &FieldTarget) -> TokenStream {
        let crate_ = crate_name();
        let sort_group_instance;
        let part_fn_instance_ty;
        if instance.has_instance() {
            sort_group_instance = instance.clone();
            part_fn_instance_ty = quote!(Self);
        } else {
            sort_group_instance = FieldTarget::RefsInstance(quote!(refs));
            part_fn_instance_ty = quote!(__BilrostRefs);
        }
        let sort_group_self = sort_group_instance.self_expr();
        let renamed = sort_group_instance.rename(quote!(instance));
        let chunks = self.chunks.iter().rev().map(|chunk| match chunk {
            AlwaysOrdered(field) => field.prepend(instance),
            SortGroup(parts) => process_sort_groups(
                parts,
                &sort_group_instance,
                SortGroupConfig {
                    direction: Direction::Reverse,
                    contiguous_part_fn: |fields: ReversibleFields| {
                        let each_field = fields.map(|field| field.prepend(&renamed));
                        quote! {
                            |instance, buf, tw| { #(#each_field)* }
                        }
                    },
                    oneof_part_fn: |field: &Field| {
                        let prepend = field.prepend(&renamed);
                        quote! {
                            |instance, buf, tw| { #prepend }
                        }
                    },
                    part_fn_ty: quote!(fn(&#part_fn_instance_ty, &mut __B, &mut #crate_::encoding::TagRevWriter)),
                    invoke_parts: quote! {
                        for (_, prepend_func) in parts {
                            (prepend_func.unwrap())(#sort_group_self, buf, tw);
                        }
                    },
                },
            ),
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

#[derive(Clone)]
pub enum FieldTarget {
    /// Represents a plain addressable instance of a message struct.
    MessageInstance(TokenStream),
    /// Represents free-floating variables for the fields of a message, named after their tags.
    FreeVariantFields,
    /// Represents free-floating reference bindings to the fields of a message, named after their
    /// tags.
    BoundVariantFields,
    /// Represents an addressable instance constructed out of refs to bound message fields.
    RefsInstance(TokenStream),
}

impl FieldTarget {
    pub fn free_field_ident(field: &Field) -> Ident {
        parse_str::<Ident>(&format!("field_{tag}", tag = field.first_tag()))
            .expect("bound field name didn't parse as an ident")
    }

    /// Returns true if this target has a single name/reference that fields are found through,
    /// rather than dispatched to loose names.
    pub fn has_instance(&self) -> bool {
        match self {
            FieldTarget::MessageInstance(..) | FieldTarget::RefsInstance(..) => true,
            FieldTarget::FreeVariantFields | FieldTarget::BoundVariantFields => false,
        }
    }

    /// Returns an expression naming this item's instance, if it is nameable.
    pub fn self_expr(&self) -> Option<TokenStream> {
        match self {
            FieldTarget::MessageInstance(instance) => Some(instance.clone()),
            FieldTarget::RefsInstance(instance) => Some(quote!(&#instance)),
            FieldTarget::FreeVariantFields | FieldTarget::BoundVariantFields => None,
        }
    }

    /// Returns an expression for a constant reference to the given field in the target.
    pub fn const_field_ref(&self, field: &Field) -> TokenStream {
        match self {
            FieldTarget::MessageInstance(instance) => {
                let field_ident = field.ident();
                quote!(&#instance.#field_ident)
            }
            FieldTarget::FreeVariantFields => {
                let field_ident = Self::free_field_ident(field);
                quote!(&#field_ident)
            }
            FieldTarget::BoundVariantFields => Self::free_field_ident(field).to_token_stream(),
            FieldTarget::RefsInstance(instance) => {
                let field_ident = Self::free_field_ident(field);
                // Reference-bearing instances are assumed to already have the correct type of
                // reference inside them.
                quote!(#instance.#field_ident)
            }
        }
    }

    pub fn mut_field_ref(&self, field: &Field) -> TokenStream {
        match self {
            FieldTarget::MessageInstance(instance) => {
                let field_ident = field.ident();
                quote!(&mut #instance.#field_ident)
            }
            FieldTarget::FreeVariantFields => {
                let field_ident = Self::free_field_ident(field);
                quote!(&mut #field_ident)
            }
            FieldTarget::BoundVariantFields => Self::free_field_ident(field).to_token_stream(),
            FieldTarget::RefsInstance(instance) => {
                let field_ident = Self::free_field_ident(field);
                quote!(#instance.#field_ident)
            }
        }
    }

    pub fn rename(&self, new_instance_ident: TokenStream) -> Self {
        match self {
            FieldTarget::MessageInstance(_) => FieldTarget::MessageInstance(new_instance_ident),
            FieldTarget::FreeVariantFields | FieldTarget::BoundVariantFields => {
                panic!("free variant fields have no instance to rename")
            }
            FieldTarget::RefsInstance(_) => FieldTarget::RefsInstance(new_instance_ident),
        }
    }

    /// Generates the prelude for a target, if necessary. For a reference-bearing-instance target,
    /// this defines a new type with references to the given fields and then instantiates it; this
    /// gives us a single reference we can pass to the `fn` values that we'll be sorting, rather
    /// than being at risk of needing to have function signatures that take a ref to each field
    /// because enum variants cannot be named.
    pub fn prelude_for<'a>(
        &self,
        fields: impl IntoIterator<Item = &'a Field>,
    ) -> Option<TokenStream> {
        let FieldTarget::RefsInstance(instance_ident) = self else {
            return None;
        };
        let fields: Vec<_> = fields.into_iter().collect();
        let field_idents: Vec<_> = fields
            .iter()
            .map(|field| Self::free_field_ident(field))
            .collect();
        let field_types: Vec<_> = fields.iter().map(|field| field.ty()).collect();
        Some(quote! {
            struct __BilrostRefs<'__r> {
                #(#field_idents: &'__r #field_types,)*
            }
            let #instance_ident = &mut __BilrostRefs { #(#field_idents),* };
        })
    }
}
