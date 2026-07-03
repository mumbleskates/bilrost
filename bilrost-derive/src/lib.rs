#![doc(html_root_url = "https://docs.rs/bilrost-derive/0.1016.0-dev")]
// The `quote!` macro requires deep recursion.
#![recursion_limit = "4096"]
#![no_std]
#![forbid(unsafe_code)]

//! This crate contains the derive macro implementations for the
//! [`bilrost`][bilrost] crate; see the documentation in that crate for usage and
//! details.
//!
//! [bilrost]: https://docs.rs/bilrost

extern crate alloc;

use crate::attrs::{
    bilrost_attrs, named_attr, set_bool, set_option_with_display, tag_list_attr, word_attr, TagList,
};
use crate::field::traits::{
    DecodeLifetime::{Borrowed, Owned},
    DecodeMode::{Distinguished, Relaxed},
    FieldBearer, SinglyTagged, Tagged,
    WhereFor::{self, Decode, Encode},
};
use crate::field::{
    initializer_class_definition, parse_message_fields, tag_measurer, Field, FieldTarget, InitMode,
    MessageFieldsSorted, OneofVariant,
};
use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, eyre as err, Report as Error};
use itertools::Itertools;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{
    parse2, Attribute, Data, DeriveInput, Expr, Fields, Generics, Ident, Meta, Pat, TypeGenerics,
    Variant, WhereClause,
};

mod attrs;
mod field;

fn crate_name() -> TokenStream {
    quote!(::bilrost)
}

/// Defines the common aliases for encoder types available to every bilrost derive.
///
/// The standard encoders are all made available in scope with lower-cased names, making them
/// simultaneously easier to spell when writing the field attributes and making them less likely to
/// shadow custom encoder types.
fn encoder_alias_header() -> TokenStream {
    let crate_ = crate_name();
    quote! {
        use #crate_::encoding::{
            Fixed as fixed,
            General as general,
            GeneralPacked as general_packed,
            Map as map,
            Packed as packed,
            PlainBytes as plainbytes,
            Unpacked as unpacked,
            Varint as varint,
        };
    }
}

/// Combines an optional WhereClause and any number of additional provided where term(s) into a
/// phrase that will always be a valid where clause if present.
fn append_wheres(
    where_clause: Option<&WhereClause>,
    wheres: impl IntoIterator<Item = TokenStream>,
) -> Option<TokenStream> {
    // dedup the where clauses by their String values
    let where_terms: BTreeMap<_, _> = wheres
        .into_iter()
        .chain(
            where_clause
                .into_iter()
                .flat_map(|w| w.predicates.iter().map(|term| quote!(#term))),
        )
        .map(|where_| (where_.to_string(), where_))
        .collect();
    // append our encoder where terms to the existing where clause if there is one
    if where_terms.is_empty() {
        None
    } else {
        let each_where_term = where_terms.values();
        Some(quote! { where #(#each_where_term,)* })
    }
}

/// Combines an optional where clause with additional terms for each field's encoder to assert that
/// it supports the field's type.
fn append_wheres_with_fields(
    where_clause: Option<&WhereClause>,
    wheres: impl IntoIterator<Item = TokenStream>,
    fields: impl FieldBearer,
    field_purpose: WhereFor,
) -> Option<TokenStream> {
    append_wheres(
        where_clause,
        wheres.into_iter().chain(fields.where_terms(field_purpose)),
    )
}

/// Adds the given identifier to the generics list
fn prepend_to_generics(generics: &Generics, ident: TokenStream) -> TokenStream {
    let params = &generics.params;
    quote!(<#ident, #params>)
}

fn try_message(input: TokenStream) -> Result<TokenStream, Error> {
    let crate_ = crate_name();
    let input: DeriveInput = parse2(input)?;

    let DeriveInput {
        ident,
        attrs: input_attrs,
        generics: impl_generics,
        data: Data::Struct(data_struct),
        ..
    } = input
    else {
        // `enum` types are only derived as `Message` in terms of their `Oneof` implementation
        if matches!(input.data, Data::Enum(..)) {
            return try_message_via_oneof(input);
        } else {
            bail!("Message can only be derived for a struct or an enum");
        }
    };

    // Process attributes
    let mut reserved_tags: Option<TagList> = None;
    let mut distinguished = false;
    let mut borrow_only = false;
    let mut default_per_field = false;
    let mut default_expr: Option<Expr> = None;
    let mut unknown_attrs = Vec::new();
    for attr in bilrost_attrs(&input_attrs)? {
        if let Some(tags) = tag_list_attr(&attr, "reserved_tags", None)? {
            set_option_with_display(
                &mut reserved_tags,
                tags,
                "duplicate reserved_tags attributes",
                TagList::display,
            )?;
        } else if word_attr(&attr, "distinguished") {
            set_bool(&mut distinguished, "duplicated distinguished attributes")?;
        } else if word_attr(&attr, "borrowed_only") {
            set_bool(&mut borrow_only, "duplicated borrowed_only attributes")?;
        } else if word_attr(&attr, "default_per_field") {
            set_bool(
                &mut default_per_field,
                "duplicated default_per_field attributes",
            )?;
        } else if let Some(expr) = named_attr(&attr, "default")? {
            set_option_with_display(
                &mut default_expr,
                expr,
                "duplicated default (expression) attributes",
                |t| quote!((#t)).to_string(),
            )?;
        } else {
            unknown_attrs.push(attr);
        }
    }
    if default_per_field && default_expr.is_some() {
        bail!("default_per_field and default (expression) attributes are mutually exclusive");
    }

    if !unknown_attrs.is_empty() {
        bail!(
            "unknown attribute(s) for message: {attrs}",
            attrs = quote!(#(#unknown_attrs),*),
        )
    }

    let init_mode = match default_per_field {
        true => InitMode::DefaultPerField,
        false => InitMode::FromStructUpdate,
    };

    // Parse field data
    let (ignored_fields, unsorted_fields): (Vec<_>, Vec<_>) =
        parse_message_fields(data_struct.fields, init_mode, reserved_tags)?
            .into_iter()
            .partition(Field::is_ignored);

    if distinguished && !ignored_fields.is_empty() {
        bail!("messages with ignored fields cannot be distinguished");
    }

    let (_, ty_generics, where_clause) = impl_generics.split_for_impl();

    let self_where = if ignored_fields
        .iter()
        .any(Field::ignored_and_uses_struct_update_syntax)
        && default_expr.is_none()
    {
        // When there are ignored fields that we are taking from ..<Self as Default>, the whole
        // message impl should be bounded by Self: Default
        Some(quote!(Self: ::core::default::Default))
    } else {
        None
    };

    let borrow_generics = prepend_to_generics(&impl_generics, quote!('__a));

    let where_fields = vec![unsorted_fields.as_slice(), ignored_fields.as_slice()];
    let encoder_where_clause =
        append_wheres_with_fields(where_clause, self_where.clone(), &where_fields, Encode);
    let [owned_decoder_where_clause, borrowed_decoder_where_clause] =
        [Owned, Borrowed].map(|lifetime| {
            append_wheres_with_fields(
                where_clause,
                self_where.clone(),
                &where_fields,
                Decode(lifetime, Relaxed),
            )
        });

    let self_instance = FieldTarget::MessageInstance(quote!(self));
    let fields = MessageFieldsSorted::new(&unsorted_fields);
    let encoded_len = fields.encoded_len(&self_instance);
    let encode = fields.encode(&self_instance);
    let prepend = fields.prepend(&self_instance);

    let [decode_owned, decode_borrowed] = [Owned, Borrowed].map(|lifetime| {
        let ident_str = ident.to_string();
        let self_instance = self_instance.clone();
        unsorted_fields.iter().map(move |field| {
            let decode = field.decode(&self_instance, lifetime, Relaxed);
            let tags = field.tags().into_iter().map(|tag| quote!(#tag));
            let tags = Itertools::intersperse(tags, quote!(|));
            let field_ident_str = field.ident().to_string();

            quote! {
                #(#tags)* => {
                    if let ::core::result::Result::Err(mut error) = #decode {
                        error.push(#ident_str, #field_ident_str);
                        return ::core::result::Result::Err(error);
                    }
                }
            }
        })
    });

    let methods = unsorted_fields
        .iter()
        .flat_map(|field| field.methods())
        .collect::<Vec<_>>();
    let methods = if methods.is_empty() {
        None
    } else {
        Some(quote! {
            #[allow(dead_code)]
            impl #impl_generics __Self #ty_generics #encoder_where_clause {
                #(#methods)*
            }
        })
    };

    let static_guards = unsorted_fields
        .iter()
        .filter_map(|field| field.tag_list_guard());

    let empties: Vec<_> = unsorted_fields
        .iter()
        .chain(ignored_fields.iter())
        .flat_map(|field| {
            let empty = field.empty(None)?;
            let ident = field.ident();
            Some(quote!(#ident: #empty))
        })
        .collect();
    let is_empties: Vec<_> = unsorted_fields
        .iter()
        .map(|field| field.is_empty(&self_instance))
        .collect();
    let clears: Vec<_> = unsorted_fields
        .iter()
        .map(|field| field.clear(&self_instance))
        .collect();

    let maybe_struct_update = if ignored_fields
        .iter()
        .any(Field::ignored_and_uses_struct_update_syntax)
    {
        // initialize ignored fields from our struct-update expression:
        let default_expr = default_expr.map_or(
            quote!(::core::default::Default::default()),
            |expr| quote!(#expr),
        );
        Some(quote!(..#default_expr))
    } else {
        None
    };

    let impl_owned_decoder = (!borrow_only).then(|| {
        quote! {
            impl #impl_generics #crate_::encoding::RawMessageDecoder
            for __Self #ty_generics #owned_decoder_where_clause {
                #[allow(unused_variables)]
                #[inline]
                fn raw_decode_field<__B>(
                    &mut self,
                    tag: u32,
                    wire_type: #crate_::encoding::WireType,
                    duplicated: bool,
                    buf: #crate_::encoding::Capped<__B>,
                    ctx: #crate_::encoding::DecodeContext,
                ) -> ::core::result::Result<(), #crate_::DecodeError>
                where
                    __B: #crate_::bytes::Buf + ?Sized,
                {
                    let _ = <Self as #crate_::encoding::RawMessage>::__ASSERTIONS;
                    match tag {
                        #(#decode_owned)*
                        _ => #crate_::encoding::skip_field(wire_type, buf)?,
                    }
                    ::core::result::Result::Ok(())
                }
            }
        }
    });

    // The static guards should be instantiated within each of the methods of the trait; in newer
    // versions of rust simply instantiating a variable in any method with `let` is enough to cause
    // the assertions to be evaluated, but in older versions the evaluation might not happen unless
    // there is an actual code path that invokes the function.
    //
    // Even in rust 1.79 nightly, if the constant is never named anywhere the assertions won't
    // actually run.
    let impls = quote! {
        impl #impl_generics #crate_::encoding::RawMessage
        for __Self #ty_generics #encoder_where_clause {
            const __ASSERTIONS: () = { #(#static_guards)* };

            fn empty() -> Self {
                Self {
                    #(#empties,)*
                    #maybe_struct_update
                }
            }

            fn is_empty(&self) -> bool {
                true #(&& #is_empties)*
            }

            fn clear(&mut self) {
                #(#clears)*
            }

            #[allow(unused_variables)]
            fn raw_encode<__B>(&self, buf: &mut __B)
            where
                __B: #crate_::bytes::BufMut + ?Sized,
            {
                let _ = <Self as #crate_::encoding::RawMessage>::__ASSERTIONS;
                #encode
            }

            #[allow(unused_variables)]
            fn raw_prepend<__B>(&self, buf: &mut __B)
            where
                __B: #crate_::buf::ReverseBuf + ?Sized,
            {
                let _ = <Self as #crate_::encoding::RawMessage>::__ASSERTIONS;
                #prepend
            }

            #[inline]
            fn raw_encoded_len(&self) -> usize {
                let _ = <Self as #crate_::encoding::RawMessage>::__ASSERTIONS;
                #encoded_len
            }
        }

        #impl_owned_decoder

        impl #borrow_generics #crate_::encoding::RawMessageBorrowDecoder<'__a>
        for __Self #ty_generics #borrowed_decoder_where_clause {
            #[allow(unused_variables)]
            #[inline]
            fn raw_borrow_decode_field(
                &mut self,
                tag: u32,
                wire_type: #crate_::encoding::WireType,
                duplicated: bool,
                buf: #crate_::encoding::Capped<&'__a [u8]>,
                ctx: #crate_::encoding::DecodeContext,
            ) -> ::core::result::Result<(), #crate_::DecodeError> {
                let _ = <Self as #crate_::encoding::RawMessage>::__ASSERTIONS;
                match tag {
                    #(#decode_borrowed)*
                    _ => #crate_::encoding::skip_field(wire_type, buf)?,
                }
                ::core::result::Result::Ok(())
            }
        }

        impl #impl_generics #crate_::encoding::ForOverwrite<(), __Self #ty_generics> for ()
        #encoder_where_clause {
            fn for_overwrite() -> __Self #ty_generics {
                <__Self #ty_generics as #crate_::encoding::RawMessage>::empty()
            }
        }

        impl #impl_generics #crate_::encoding::EmptyState<(), __Self #ty_generics> for ()
        #encoder_where_clause {
            fn is_empty(val: &__Self #ty_generics) -> bool {
                <__Self #ty_generics as #crate_::encoding::RawMessage>::is_empty(val)
            }

            fn clear(val: &mut __Self #ty_generics) {
                <__Self #ty_generics as #crate_::encoding::RawMessage>::clear(val);
            }
        }
    };

    let distinguished_impls = distinguished.then(|| {
        // At time of commenting distinguished mode precludes any additional self-bounds, as there
        // cannot be ignored fields. If we add any in the future, we want to catch that
        // automatically.
        let distinguished_self_where = [quote!(Self: ::core::cmp::Eq)]
            .into_iter()
            .chain(self_where);
        let [owned_decoder_where_clause, borrowed_decoder_where_clause] =
            [Owned, Borrowed].map(|lifetime| {
                append_wheres_with_fields(
                    where_clause,
                    distinguished_self_where.clone(),
                    &where_fields,
                    Decode(lifetime, Distinguished),
                )
            });

        let [decode_owned, decode_borrowed] = [Owned, Borrowed].map(|lifetime| {
            let ident_str = ident.to_string();
            let self_instance = self_instance.clone();
            unsorted_fields.iter().map(move |field| {
                let decode = field.decode(&self_instance, lifetime, Distinguished);
                let tags = field.tags().into_iter().map(|tag| quote!(#tag));
                let tags = Itertools::intersperse(tags, quote!(|));
                let field_ident_str = field.ident().to_string();

                quote! {
                    #(#tags)* => {
                        match #decode {
                            ::core::result::Result::Ok(new_canon) => {
                                canon.update(new_canon);
                            }
                            ::core::result::Result::Err(mut error) => {
                                error.push(#ident_str, #field_ident_str);
                                return ::core::result::Result::Err(error);
                            }
                        }
                    }
                }
            })
        });

        let impl_owned_decoder = (!borrow_only).then(|| {
            quote! {
                impl #impl_generics #crate_::encoding::RawDistinguishedMessageDecoder
                for __Self #ty_generics #owned_decoder_where_clause {
                    #[allow(unused_variables)]
                    #[inline]
                    fn raw_decode_field_distinguished<__B>(
                        &mut self,
                        tag: u32,
                        wire_type: #crate_::encoding::WireType,
                        duplicated: bool,
                        buf: #crate_::encoding::Capped<__B>,
                        ctx: #crate_::encoding::RestrictedDecodeContext,
                    ) -> ::core::result::Result<#crate_::Canonicity, #crate_::DecodeError>
                    where
                        __B: #crate_::bytes::Buf + ?Sized,
                    {
                        let mut canon = #crate_::Canonicity::Canonical;
                        match tag {
                            #(#decode_owned)*
                            _ => {
                                canon.update(ctx.check(#crate_::Canonicity::HasExtensions)?);
                                #crate_::encoding::skip_field(wire_type, buf)?;
                            }
                        }
                        ::core::result::Result::Ok(canon)
                    }
                }
            }
        });

        quote! {
            #impl_owned_decoder

            impl #borrow_generics #crate_::encoding::RawDistinguishedMessageBorrowDecoder<'__a>
            for __Self #ty_generics #borrowed_decoder_where_clause {
                #[allow(unused_variables)]
                #[inline]
                fn raw_borrow_decode_field_distinguished(
                    &mut self,
                    tag: u32,
                    wire_type: #crate_::encoding::WireType,
                    duplicated: bool,
                    buf: #crate_::encoding::Capped<&'__a [u8]>,
                    ctx: #crate_::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<#crate_::Canonicity, #crate_::DecodeError> {
                    let canon = &mut #crate_::Canonicity::Canonical;
                    match tag {
                        #(#decode_borrowed)*
                        _ => {
                            canon.update(ctx.check(#crate_::Canonicity::HasExtensions)?);
                            #crate_::encoding::skip_field(wire_type, buf)?;
                        }
                    }
                    ::core::result::Result::Ok(*canon)
                }
            }
        }
    });

    let aliases = encoder_alias_header();
    let initializer_class = initializer_class_definition(
        ignored_fields
            .iter()
            .flat_map(|field| field.initializer_method(None)),
        &impl_generics,
    );
    let expanded = quote! {
        const _: () = {
            use #ident as __Self;

            #initializer_class

            const _: () = {
                #aliases

                #impls

                #distinguished_impls

                #methods
            };
        };
    };

    Ok(expanded)
}

fn try_message_via_oneof(input: DeriveInput) -> Result<TokenStream, Error> {
    let crate_ = crate_name();
    let PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        variants,
        distinguished,
        borrow_only,
        empty_variant,
    } = preprocess_oneof(&input)?;

    let tag_measurer_ty = tag_measurer(&variants);

    if empty_variant.is_none() {
        bail!("Message can only be derived for Oneof enums that have an empty variant.")
    }

    let borrow_generics = prepend_to_generics(impl_generics, quote!('__a));

    let encoder_where_clause = append_wheres(
        where_clause,
        [quote!(#ident #ty_generics: #crate_::encoding::Oneof)],
    );
    let owned_decoder_where_clause = append_wheres(
        where_clause,
        [quote!(#ident #ty_generics: #crate_::encoding::OneofDecoder)],
    );
    let borrowed_decoder_where_clause = append_wheres(
        where_clause,
        [quote!(#ident #ty_generics: #crate_::encoding::OneofBorrowDecoder<'__a>)],
    );

    let impl_owned_decoder = (!borrow_only).then(|| {
        quote! {
            impl #impl_generics #crate_::encoding::RawMessageDecoder
            for #ident #ty_generics #owned_decoder_where_clause {
                #[inline(always)]
                fn raw_decode_field<__B>(
                    &mut self,
                    tag: u32,
                    wire_type: #crate_::encoding::WireType,
                    _duplicated: bool,
                    buf: #crate_::encoding::Capped<__B>,
                    ctx: #crate_::encoding::DecodeContext,
                ) -> ::core::result::Result<(), #crate_::DecodeError>
                where
                    __B: #crate_::bytes::Buf + ?Sized,
                {
                    if <Self as #crate_::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                        <Self as #crate_::encoding::OneofDecoder>::oneof_decode_field(
                            self,
                            tag,
                            wire_type,
                            buf,
                            ctx,
                        )
                    } else {
                        #crate_::encoding::skip_field(wire_type, buf)
                    }
                }
            }
        }
    });

    let impls = quote! {
        impl #impl_generics #crate_::encoding::RawMessage
        for #ident #ty_generics #encoder_where_clause {
            const __ASSERTIONS: () = ();

            #[inline(always)]
            fn empty() -> Self {
                <Self as #crate_::encoding::Oneof>::empty()
            }

            #[inline(always)]
            fn is_empty(&self) -> bool {
                <Self as #crate_::encoding::Oneof>::is_empty(self)
            }

            #[inline(always)]
            fn clear(&mut self) {
                <Self as #crate_::encoding::Oneof>::clear(self)
            }

            #[inline(always)]
            fn raw_encode<__B>(&self, buf: &mut __B)
            where
                __B: #crate_::bytes::BufMut + ?Sized,
            {
                <Self as #crate_::encoding::Oneof>::oneof_encode(
                    self,
                    buf,
                    &mut #crate_::encoding::TagWriter::new(),
                );
            }

            #[inline(always)]
            fn raw_prepend<__B>(&self, buf: &mut __B)
            where
                __B: #crate_::buf::ReverseBuf + ?Sized,
            {
                let tw = &mut #crate_::encoding::TagRevWriter::new();
                <Self as #crate_::encoding::Oneof>::oneof_prepend(self, buf, tw);
                tw.finalize(buf);
            }

            #[inline(always)]
            fn raw_encoded_len(&self) -> usize {
                <Self as #crate_::encoding::Oneof>::oneof_encoded_len(
                    self,
                    &mut #tag_measurer_ty::new(),
                )
            }
        }

        impl #impl_generics #crate_::encoding::ForOverwrite<(), #ident #ty_generics> for ()
        #encoder_where_clause {
            #[inline(always)]
            fn for_overwrite() -> #ident #ty_generics {
                <#ident #ty_generics as #crate_::encoding::Oneof>::empty()
            }
        }

        impl #impl_generics #crate_::encoding::EmptyState<(), #ident #ty_generics> for ()
        #encoder_where_clause {
            #[inline(always)]
            fn is_empty(val: &#ident #ty_generics) -> bool {
                <#ident #ty_generics as #crate_::encoding::Oneof>::is_empty(val)
            }

            #[inline(always)]
            fn clear(val: &mut #ident #ty_generics) {
                <#ident #ty_generics as #crate_::encoding::Oneof>::clear(val);
            }
        }

        #impl_owned_decoder

        impl #borrow_generics #crate_::encoding::RawMessageBorrowDecoder<'__a>
        for #ident #ty_generics #borrowed_decoder_where_clause {
            #[inline(always)]
            fn raw_borrow_decode_field(
                &mut self,
                tag: u32,
                wire_type: #crate_::encoding::WireType,
                _duplicated: bool,
                buf: #crate_::encoding::Capped<&'__a [u8]>,
                ctx: #crate_::encoding::DecodeContext,
            ) -> ::core::result::Result<(), #crate_::DecodeError> {
                if <Self as #crate_::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                    <Self as #crate_::encoding::OneofBorrowDecoder>::oneof_borrow_decode_field(
                        self,
                        tag,
                        wire_type,
                        buf,
                        ctx,
                    )
                } else {
                    #crate_::encoding::skip_field(wire_type, buf)
                }
            }
        }
    };

    let distinguished_impls = distinguished.then(|| {
        let owned_decoder_where_clause = append_wheres(
            where_clause,
            [quote!(
                Self: #crate_::encoding::DistinguishedOneofDecoder + ::core::cmp::Eq
            )],
        );
        let borrowed_decoder_where_clause = append_wheres(
            where_clause,
            [quote!(
                Self: #crate_::encoding::DistinguishedOneofBorrowDecoder<'__a> + ::core::cmp::Eq
            )],
        );

        let impl_owned_decoder = (!borrow_only).then(|| {
            quote! {
                impl #impl_generics #crate_::encoding::RawDistinguishedMessageDecoder
                for #ident #ty_generics #owned_decoder_where_clause {
                    #[inline(always)]
                    fn raw_decode_field_distinguished<__B>(
                        &mut self,
                        tag: u32,
                        wire_type: #crate_::encoding::WireType,
                        _duplicated: bool,
                        buf: #crate_::encoding::Capped<__B>,
                        ctx: #crate_::encoding::RestrictedDecodeContext,
                    ) -> ::core::result::Result<#crate_::Canonicity, #crate_::DecodeError>
                    where
                        __B: #crate_::bytes::Buf + ?Sized,
                    {
                        if <Self as #crate_::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                            <Self as #crate_::encoding::DistinguishedOneofDecoder>::
                                oneof_decode_field_distinguished
                            (
                                self,
                                tag,
                                wire_type,
                                buf,
                                ctx,
                            )
                        } else {
                            _ = ctx.check(#crate_::Canonicity::HasExtensions)?;
                            #crate_::encoding::skip_field(wire_type, buf)?;
                            ::core::result::Result::Ok(#crate_::Canonicity::HasExtensions)
                        }
                    }
                }
            }
        });

        quote! {
            #impl_owned_decoder

            impl #borrow_generics #crate_::encoding::RawDistinguishedMessageBorrowDecoder<'__a>
            for #ident #ty_generics #borrowed_decoder_where_clause {
                #[inline(always)]
                fn raw_borrow_decode_field_distinguished(
                    &mut self,
                    tag: u32,
                    wire_type: #crate_::encoding::WireType,
                    _duplicated: bool,
                    buf: #crate_::encoding::Capped<&'__a [u8]>,
                    ctx: #crate_::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<#crate_::Canonicity, #crate_::DecodeError> {
                    if <Self as #crate_::encoding::Oneof>::FIELD_TAGS.contains(&tag) {
                        <Self as #crate_::encoding::DistinguishedOneofBorrowDecoder>::
                            oneof_borrow_decode_field_distinguished
                        (
                            self,
                            tag,
                            wire_type,
                            buf,
                            ctx,
                        )
                    } else {
                        _ = ctx.check(#crate_::Canonicity::HasExtensions)?;
                        #crate_::encoding::skip_field(wire_type, buf)?;
                        ::core::result::Result::Ok(#crate_::Canonicity::HasExtensions)
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
    let crate_ = crate_name();
    let input: DeriveInput = parse2(input)?;
    let ident = input.ident;

    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let unborrowed_generics = prepend_to_generics(generics, quote!(const __G: u8));
    let borrow_generics = prepend_to_generics(generics, quote!('__a, const __G: u8));

    let punctuated_variants = match input.data {
        Data::Enum(enum_) => enum_.variants,
        Data::Struct(_) => bail!("Enumeration can not be derived for a struct"),
        Data::Union(..) => bail!("Enumeration can not be derived for a union"),
    };

    struct EnumVariant {
        variant_ident: Ident,
        discriminant_expr: Expr,
    }

    // Parse each variant
    let mut variants = vec![];
    for Variant {
        attrs,
        ident: variant_ident,
        fields,
        discriminant,
        ..
    } in punctuated_variants
    {
        if !match fields {
            Fields::Unit => true,
            Fields::Named(named) => named.named.is_empty(),
            Fields::Unnamed(unnamed) => unnamed.unnamed.is_empty(),
        } {
            bail!("Enumeration variants may not have fields");
        }

        let discriminant_expr = variant_attr(&attrs)?
            .or(discriminant.map(|(_, expr)| expr))
            .ok_or_else(|| {
                err!(
                    "Enumeration variants must have a discriminant or a #[bilrost(..)] attribute \
                    with a constant value"
                )
            })?;
        variants.push(EnumVariant {
            variant_ident,
            discriminant_expr,
        });
    }
    let zero_variant_ident = variants
        .iter()
        .find(|variant| is_zero_discriminant(&variant.discriminant_expr))
        .map(|variant| &variant.variant_ident);

    let Some(EnumVariant {
        variant_ident: first_variant,
        ..
    }) = variants.first()
    else {
        bail!("Enumerations must have at least one variant");
    };

    let variant_idents: Vec<_> = variants
        .iter()
        .map(|variant| &variant.variant_ident)
        .collect();
    let discriminant_exprs: Vec<_> = variants
        .iter()
        .map(|variant| &variant.discriminant_expr)
        .collect();

    // When the type has a zero-valued variant, we implement `EmptyState`. When it doesn't, we
    // need at least some way to create a value to be overwritten, so we impl `ForOverwrite`
    // directly with an arbitrary variant.
    let creation_impl = if let Some(zero) = &zero_variant_ident {
        quote! {
            impl #impl_generics #crate_::encoding::ForOverwrite<(), #ident #ty_generics> for ()
            #where_clause {
                #[inline]
                fn for_overwrite() -> #ident #ty_generics {
                    #ident::#zero { }
                }
            }

            impl #impl_generics #crate_::encoding::EmptyState<(), #ident #ty_generics> for ()
            #where_clause {
                #[inline]
                fn is_empty(val: &#ident #ty_generics) -> bool {
                    matches!(val, #ident::#zero { })
                }

                #[inline]
                fn clear(val: &mut #ident #ty_generics) {
                    *val = #ident::#zero { };
                }
            }
        }
    } else {
        quote! {
            impl #impl_generics #crate_::encoding::ForOverwrite<(), #ident #ty_generics> for ()
            #where_clause {
                fn for_overwrite() -> #ident #ty_generics {
                    #ident::#first_variant { }
                }
            }
        }
    };

    let expanded = quote! {
        impl #impl_generics #crate_::Enumeration for #ident #ty_generics #where_clause {
            #[inline]
            fn to_number(&self) -> u32 {
                match self {
                    #(#ident::#variant_idents { } => #discriminant_exprs,)*
                }
            }

            #[inline]
            fn try_from_number(value: u32) -> ::core::result::Result<#ident, u32> {
                #[forbid(unreachable_patterns)]
                ::core::result::Result::Ok(match value {
                    #(#discriminant_exprs => #ident::#variant_idents { },)*
                    _ => ::core::result::Result::Err(value)?,
                })
            }

            #[inline]
            fn is_valid(__n: u32) -> bool {
                #[forbid(unreachable_patterns)]
                match __n {
                    #(#discriminant_exprs => true,)*
                    _ => false,
                }
            }
        }

        #creation_impl

        impl #unborrowed_generics
        #crate_::encoding::Wiretyped<
            #crate_::encoding::GeneralGeneric<__G>,
            #ident #ty_generics
        > for () #where_clause {
            const WIRE_TYPE: #crate_::encoding::WireType = #crate_::encoding::WireType::Varint;
        }

        impl #unborrowed_generics
        #crate_::encoding::ValueEncoder<
            #crate_::encoding::GeneralGeneric<__G>,
            #ident #ty_generics
        > for () #where_clause {
            #[inline]
            fn encode_value<__B: #crate_::bytes::BufMut + ?Sized>(
                value: &#ident #ty_generics,
                buf: &mut __B,
            ) {
                #crate_::encoding::encode_varint(
                    #crate_::Enumeration::to_number(value) as u64,
                    buf,
                );
            }

            #[inline]
            fn prepend_value<__B: #crate_::buf::ReverseBuf + ?Sized>(
                value: &#ident #ty_generics,
                buf: &mut __B,
            ) {
                #crate_::encoding::prepend_varint(
                    #crate_::Enumeration::to_number(value) as u64,
                    buf,
                );
            }

            #[inline]
            fn value_encoded_len(value: &#ident #ty_generics) -> usize {
                #crate_::encoding::encoded_len_varint(
                    #crate_::encoding::Enumeration::to_number(value) as u64
                )
            }
        }

        impl #unborrowed_generics
        #crate_::encoding::ValueDecoder<
            #crate_::encoding::GeneralGeneric<__G>,
            #ident #ty_generics
        > for () #where_clause {
            #[inline]
            fn decode_value<__B: #crate_::bytes::Buf + ?Sized>(
                value: &mut #ident #ty_generics,
                mut buf: #crate_::encoding::Capped<__B>,
                _ctx: #crate_::encoding::DecodeContext,
            ) -> Result<(), #crate_::DecodeError> {
                let decoded = buf.decode_varint()?;
                let ::core::result::Result::Ok(in_range) = u32::try_from(decoded) else {
                    return ::core::result::Result::Err(
                        #crate_::DecodeErrorKind::OutOfDomainValue.into()
                    );
                };
                let ::core::result::Result::Ok(typed) =
                    <#ident #ty_generics as #crate_::Enumeration>::try_from_number(in_range) else {
                    return ::core::result::Result::Err(
                        #crate_::DecodeErrorKind::OutOfDomainValue.into()
                    );
                };
                *value = typed;
                ::core::result::Result::Ok(())
            }
        }

        impl #unborrowed_generics
        #crate_::encoding::DistinguishedValueDecoder<
            #crate_::encoding::GeneralGeneric<__G>,
            #ident #ty_generics
        > for () #where_clause {
            const CHECKS_EMPTY: bool = false;

            #[inline]
            fn decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut #ident #ty_generics,
                buf: #crate_::encoding::Capped<impl #crate_::bytes::Buf + ?Sized>,
                ctx: #crate_::encoding::RestrictedDecodeContext,
            ) -> Result<#crate_::Canonicity, #crate_::DecodeError> {
                <() as #crate_::encoding::ValueDecoder<
                    #crate_::encoding::GeneralGeneric<__G>, #ident #ty_generics
                >>::decode_value(
                    value,
                    buf,
                    ctx.into_inner(),
                )?;
                ::core::result::Result::Ok(#crate_::Canonicity::Canonical)
            }
        }

        impl #borrow_generics
        #crate_::encoding::ValueBorrowDecoder<
            '__a,
            #crate_::encoding::GeneralGeneric<__G>,
            #ident #ty_generics
        > for () #where_clause {
            #[inline(always)]
            fn borrow_decode_value(
                value: &mut #ident #ty_generics,
                mut buf: #crate_::encoding::Capped<&'__a [u8]>,
                ctx: #crate_::encoding::DecodeContext,
            ) -> Result<(), #crate_::DecodeError> {
                <() as #crate_::encoding::ValueDecoder<
                    #crate_::encoding::GeneralGeneric<__G>, #ident #ty_generics
                >>::decode_value(
                    value,
                    buf,
                    ctx,
                )
            }
        }

        impl #borrow_generics
        #crate_::encoding::DistinguishedValueBorrowDecoder<
            '__a,
            #crate_::encoding::GeneralGeneric<__G>,
            #ident #ty_generics
        > for () #where_clause {
            const CHECKS_EMPTY: bool = false;

            #[inline(always)]
            fn borrow_decode_value_distinguished<const ALLOW_EMPTY: bool>(
                value: &mut #ident #ty_generics,
                buf: #crate_::encoding::Capped<&'__a [u8]>,
                ctx: #crate_::encoding::RestrictedDecodeContext,
            ) -> Result<#crate_::Canonicity, #crate_::DecodeError> {
                <() as #crate_::encoding::ValueDecoder<
                    #crate_::encoding::GeneralGeneric<__G>, #ident #ty_generics
                >>::decode_value(
                    value,
                    buf,
                    ctx.into_inner(),
                )?;
                ::core::result::Result::Ok(#crate_::Canonicity::Canonical)
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
                Meta::List(list) => parse2::<Expr>(list.tokens.clone()).ok(),
                Meta::NameValue(name_value) => Some(name_value.value.clone()),
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

            set_option_with_display(
                &mut result,
                expr,
                "duplicate value attributes on enumeration variant",
                |t| quote!((#t)).to_string(),
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
    variants: Vec<OneofVariant>,
    distinguished: bool,
    borrow_only: bool,
    empty_variant: Option<Ident>,
}

fn preprocess_oneof(input: &DeriveInput) -> Result<PreprocessedOneof<'_>, Error> {
    let ident = input.ident.clone();

    let input_variants = match &input.data {
        Data::Enum(enum_) => enum_.variants.clone(),
        Data::Struct(..) => bail!("Oneof can not be derived for a struct"),
        Data::Union(..) => bail!("Oneof can not be derived for a union"),
    };

    let mut reserved_tags = None;
    let mut unknown_attrs = Vec::new();
    let mut distinguished = false;
    let mut borrow_only = false;
    for attr in bilrost_attrs(&input.attrs)? {
        if let Some(tags) = tag_list_attr(&attr, "reserved_tags", None)? {
            set_option_with_display(
                &mut reserved_tags,
                tags,
                "duplicate reserved_tags attributes",
                TagList::display,
            )?;
        } else if word_attr(&attr, "distinguished") {
            set_bool(&mut distinguished, "duplicated distinguished attributes")?;
        } else if word_attr(&attr, "borrowed_only") {
            set_bool(&mut borrow_only, "duplicated borrowed_only attributes")?;
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
    let mut variants = vec![];
    // Map the variants into 'fields'.
    for variant in input_variants {
        let variant_ident = variant.ident.clone();
        match OneofVariant::new(variant)? {
            Some(variant) => {
                variants.push(variant);
            }
            None => {
                set_option_with_display(
                    &mut empty_variant,
                    variant_ident,
                    "Oneofs may have at most one empty enum variant. To use multiple \
                    variants without fields, the non-empty variants can be marked as values with \
                    the 'message' attribute and the empty variant can be either left un-marked or \
                    explicitly marked with the 'empty' attribute.\n\nThe conflicting variants were",
                    |t| t.to_string(),
                )?;
            }
        }
    }

    if distinguished && variants.iter().any(OneofVariant::has_ignored_fields) {
        bail!("Oneofs with ignored fields cannot be distinguished");
    }

    // Index all fields by their tag(s) and check them against the forbidden tag ranges
    let all_tags: BTreeMap<u32, &Ident> = variants
        .iter()
        .map(|variant| (variant.tag(), variant.ident()))
        .collect();
    for reserved_range in reserved_tags.unwrap_or_default().iter_tag_ranges() {
        if let Some((forbidden_tag, variant_ident)) = all_tags.range(reserved_range).next() {
            bail!("oneof {ident} variant {variant_ident} has reserved tag {forbidden_tag}");
        }
    }

    let generics = &input.generics;
    let (_, ty_generics, where_clause) = generics.split_for_impl();

    Ok(PreprocessedOneof {
        ident,
        impl_generics: generics,
        ty_generics,
        where_clause,
        variants,
        distinguished,
        borrow_only,
        empty_variant,
    })
}

fn try_oneof(input: TokenStream) -> Result<TokenStream, Error> {
    let crate_ = crate_name();
    let input: DeriveInput = parse2(input)?;

    let PreprocessedOneof {
        ident,
        impl_generics,
        ty_generics,
        where_clause,
        variants,
        distinguished,
        borrow_only,
        empty_variant,
    } = preprocess_oneof(&input)?;

    let borrow_generics = prepend_to_generics(impl_generics, quote!('__a));

    let encoder_where_clause = append_wheres_with_fields(where_clause, None, &variants, Encode);
    let owned_decoder_where_clause =
        append_wheres_with_fields(where_clause, None, &variants, Decode(Owned, Relaxed));
    let borrowed_decoder_where_clause =
        append_wheres_with_fields(where_clause, None, &variants, Decode(Borrowed, Relaxed));

    let sorted_tags: Vec<u32> = variants
        .iter()
        .map(|variant| variant.tag())
        .sorted_unstable()
        .collect();
    if let Some((duplicate_tag, _)) = sorted_tags.iter().tuple_windows().find(|(a, b)| a == b) {
        bail!(
            "invalid oneof {}: multiple variants have tag {}",
            ident,
            duplicate_tag
        );
    }

    let self_alias = quote!(Self);

    let mut encode: Vec<TokenStream> = variants
        .iter()
        .map(|variant| variant.encode(&self_alias))
        .collect();

    let mut prepend: Vec<TokenStream> = variants
        .iter()
        .map(|variant| variant.prepend(&self_alias))
        .collect();

    let mut encoded_len: Vec<TokenStream> = variants
        .iter()
        .map(|variant| variant.encoded_len(&self_alias))
        .collect();

    let encoder_trait;
    let owned_decoder_trait;
    let borrowed_decoder_trait;
    let decode_field_self_arg;
    let decode_field_return_ty;
    let current_tag_ty;
    let current_tag: Vec<TokenStream>;
    let empty_methods_impl;
    let some;

    if let Some(empty_ident) = &empty_variant {
        encoder_trait = quote!(Oneof);
        owned_decoder_trait = quote!(OneofDecoder);
        borrowed_decoder_trait = quote!(OneofBorrowDecoder<'__a>);
        decode_field_self_arg = Some(quote!(value: &mut Self,));
        decode_field_return_ty = quote!(());
        some = Some(quote!(::core::option::Option::Some));

        current_tag_ty = quote!(::core::option::Option<u32>);
        current_tag = variants
            .iter()
            .map(|variant| {
                let tag = variant.tag();
                let variant_ident = variant.ident();
                quote!(Self::#variant_ident { .. } => ::core::option::Option::Some(#tag))
            })
            .chain([quote!(Self::#empty_ident => ::core::option::Option::None)])
            .collect();
        encode.push(quote!(Self::#empty_ident => {}));
        prepend.push(quote!(Self::#empty_ident => {}));
        encoded_len.push(quote!(Self::#empty_ident => 0));

        empty_methods_impl = Some(quote! {
            fn empty() -> Self {
                Self::#empty_ident
            }

            fn is_empty(&self) -> bool {
                matches!(self, Self::#empty_ident)
            }

            fn clear(&mut self) {
                *self = Self::#empty_ident;
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
        current_tag = variants
            .iter()
            .map(|variant| {
                let tag = variant.tag();
                let variant_ident = variant.ident();
                quote!(Self::#variant_ident { .. } => #tag)
            })
            .collect();

        empty_methods_impl = None;
    };

    let variant_name_arms = variants.iter().map(|variant| {
        let tag = variant.tag();
        let ident_str = ident.to_string();
        let variant_ident_str = variant.ident().to_string();
        quote! {
            #tag => (#ident_str, #variant_ident_str),
        }
    });

    let decode_arms = |lifetime, mode| {
        let ident_str = ident.to_string();
        let arms = variants
            .iter()
            .map(|variant| variant.decode(&self_alias, lifetime, mode));
        quote! {
            match tag {
                #(#arms,)*
                _ => unreachable!(
                    concat!("invalid ", #ident_str, " tag: {}"), tag,
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
            match if let Self::#empty_ident = value {
                match #decode {
                    ::core::result::Result::Ok(decoded) => {
                        *value = decoded;
                        ::core::result::Result::Ok(())
                    }
                    ::core::result::Result::Err(error) => ::core::result::Result::Err(error),
                }
            } else {
                ::core::result::Result::Err(#crate_::DecodeError::new(
                    if #crate_::encoding::#encoder_trait::oneof_current_tag(value) == #some(tag) {
                        #crate_::DecodeErrorKind::UnexpectedlyRepeated
                    } else {
                        #crate_::DecodeErrorKind::ConflictingFields
                    }
                ))
            } {
                ::core::result::Result::Err(mut error) => {
                    let (msg, field) =
                        <Self as #crate_::encoding::#encoder_trait>::oneof_variant_name(tag);
                    error.push(msg, field);
                    ::core::result::Result::Err(error)
                }
                ok => ok,
            }
        })
    };

    let impl_owned_decoder = (!borrow_only).then(|| {
        quote! {
            impl #impl_generics #crate_::encoding::#owned_decoder_trait
            for __Self #ty_generics #owned_decoder_where_clause
            {
                fn oneof_decode_field<__B: #crate_::bytes::Buf + ?Sized>(
                    #decode_field_self_arg
                    tag: u32,
                    wire_type: #crate_::encoding::WireType,
                    buf: #crate_::encoding::Capped<__B>,
                    ctx: #crate_::encoding::DecodeContext,
                ) -> ::core::result::Result<#decode_field_return_ty, #crate_::DecodeError> {
                    #decode_owned
                }
            }
        }
    });

    let impls = quote! {
        impl #impl_generics #crate_::encoding::#encoder_trait
        for __Self #ty_generics #encoder_where_clause
        {
            const FIELD_TAGS: &'static [u32] = &[#(#sorted_tags),*];

            #empty_methods_impl

            fn oneof_encode<__B: #crate_::bytes::BufMut + ?Sized>(
                &self,
                buf: &mut __B,
                tw: &mut #crate_::encoding::TagWriter,
            ) {
                match self {
                    #(#encode,)*
                }
            }

            fn oneof_prepend<__B: #crate_::buf::ReverseBuf + ?Sized>(
                &self,
                buf: &mut __B,
                tw: &mut #crate_::encoding::TagRevWriter,
            ) {
                match self {
                    #(#prepend,)*
                }
            }

            fn oneof_encoded_len(
                &self,
                tm: &mut impl #crate_::encoding::TagMeasurer,
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

        #impl_owned_decoder

        impl #borrow_generics #crate_::encoding::#borrowed_decoder_trait
        for __Self #ty_generics #borrowed_decoder_where_clause
        {
            fn oneof_borrow_decode_field(
                #decode_field_self_arg
                tag: u32,
                wire_type: #crate_::encoding::WireType,
                buf: #crate_::encoding::Capped<&'__a [u8]>,
                ctx: #crate_::encoding::DecodeContext,
            ) -> ::core::result::Result<#decode_field_return_ty, #crate_::DecodeError> {
                #decode_borrowed
            }
        }
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
            decode_field_return_ty = quote!(#crate_::Canonicity);
            some = Some(quote!(::core::option::Option::Some));
            [owned_decoder_where_clause, borrowed_decoder_where_clause] =
                [Owned, Borrowed].map(|lifetime| {
                    append_wheres_with_fields(
                        where_clause,
                        [quote!(Self: #crate_::encoding::Oneof)],
                        &variants,
                        Decode(lifetime, Distinguished),
                    )
                });
        } else {
            owned_decoder_trait = quote!(NonEmptyDistinguishedOneofDecoder);
            borrowed_decoder_trait = quote!(NonEmptyDistinguishedOneofBorrowDecoder<'__a>);
            relaxed_oneof_trait = quote!(NonEmptyOneof);
            decode_field_self_arg = None;
            decode_field_return_ty = quote!((Self, #crate_::Canonicity));
            some = None;
            [owned_decoder_where_clause, borrowed_decoder_where_clause] =
                [Owned, Borrowed].map(|lifetime| {
                    append_wheres_with_fields(
                        where_clause,
                        None,
                        &variants,
                        Decode(lifetime, Distinguished),
                    )
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
                    match if let Self::#empty_ident = value {
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
                        ::core::result::Result::Err(#crate_::DecodeError::new(
                            if #crate_::encoding::#relaxed_oneof_trait::oneof_current_tag(value)
                                == #some(tag)
                            {
                                #crate_::DecodeErrorKind::UnexpectedlyRepeated
                            } else {
                                #crate_::DecodeErrorKind::ConflictingFields
                            }
                        ))
                    } {
                        ::core::result::Result::Err(mut error) => {
                            let (msg, field) =
                                <Self as #crate_::encoding::#relaxed_oneof_trait>::
                                    oneof_variant_name(tag);
                            error.push(msg, field);
                            ::core::result::Result::Err(error)
                        }
                        ok => ok,
                    }
                }
            }),
        };

        let impl_owned_decoder = (!borrow_only).then(|| {
            quote! {
                impl #impl_generics #crate_::encoding::#owned_decoder_trait
                for __Self #ty_generics #owned_decoder_where_clause
                {
                    fn oneof_decode_field_distinguished<__B: #crate_::bytes::Buf + ?Sized>(
                        #decode_field_self_arg
                        tag: u32,
                        wire_type: #crate_::encoding::WireType,
                        buf: #crate_::encoding::Capped<__B>,
                        ctx: #crate_::encoding::RestrictedDecodeContext,
                    ) -> ::core::result::Result<#decode_field_return_ty, #crate_::DecodeError> {
                        #decode_owned
                    }
                }
            }
        });

        quote! {
            #impl_owned_decoder

            impl #borrow_generics #crate_::encoding::#borrowed_decoder_trait
            for __Self #ty_generics #borrowed_decoder_where_clause
            {
                fn oneof_borrow_decode_field_distinguished(
                    #decode_field_self_arg
                    tag: u32,
                    wire_type: #crate_::encoding::WireType,
                    buf: #crate_::encoding::Capped<&'__a [u8]>,
                    ctx: #crate_::encoding::RestrictedDecodeContext,
                ) -> ::core::result::Result<#decode_field_return_ty, #crate_::DecodeError> {
                    #decode_borrowed
                }
            }
        }
    });

    let aliases = encoder_alias_header();
    let initializer_class = initializer_class_definition(
        variants.iter().flat_map(OneofVariant::initializer_methods),
        impl_generics,
    );
    Ok(quote! {
        const _: () = {
            use #ident as __Self;

            #initializer_class

            const _: () = {
                #aliases

                #impls

                #distinguished_impls
            };
        };
    })
}

#[proc_macro_derive(Oneof, attributes(bilrost))]
pub fn oneof(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    try_oneof(input.into()).unwrap().into()
}

#[cfg(test)]
mod test {
    use crate::{try_enumeration, try_message, try_oneof};
    use alloc::format;
    use alloc::string::ToString;
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
            "multiple fields have tag 1"
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
            "multiple fields have tag 2"
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
            "multiple fields have tag 10"
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
            "field a has reserved tag 1"
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
            "field b has reserved tag 4"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(5-10, 55))]
            struct Invalid {
                #[bilrost(tag = "1")]
                a: bool,
                #[bilrost(oneof(3-5))]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "field b has reserved tag 5"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(..=3, 55))]
            struct Invalid {
                #[bilrost(tag = "999")]
                a: bool,
                #[bilrost(oneof(3-5))]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "field b has reserved tag 3"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(0, 5..))]
            struct Invalid {
                #[bilrost(tag = "1")]
                a: bool,
                #[bilrost(oneof(3-5))]
                b: Option<super::Whatever>,
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "field b has reserved tag 5"
        );
    }

    #[test]
    fn test_rejects_reserved_oneof_fields() {
        let output = try_message(quote! {
            #[bilrost(reserved_tags(1, 100))]
            enum Invalid {
                #[bilrost(tag = "1")]
                A(bool),
                #[bilrost(5)]
                B(super::Whatever),
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "oneof Invalid variant A has reserved tag 1"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(5, 55))]
            enum Invalid {
                #[bilrost(tag = "1")]
                A(bool),
                #[bilrost(5)]
                B(super::Whatever),
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "oneof Invalid variant B has reserved tag 5"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(5-10, 55))]
            enum Invalid {
                #[bilrost(tag = "1")]
                A(bool),
                #[bilrost(5)]
                B(super::Whatever),
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "oneof Invalid variant B has reserved tag 5"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(..=3, 55))]
            enum Invalid {
                #[bilrost(tag = "1")]
                A(bool),
                #[bilrost(5)]
                B(super::Whatever),
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "oneof Invalid variant A has reserved tag 1"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(0, 5..))]
            enum Invalid {
                #[bilrost(tag = "1")]
                A(bool),
                #[bilrost(5)]
                B(super::Whatever),
            }
        });
        assert_eq!(
            output.expect_err("reserved tags not detected").to_string(),
            "oneof Invalid variant B has reserved tag 5"
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
            "invalid field a: too-large tag range 1-100; use smaller ranges"
        );
    }

    #[test]
    fn test_accepts_tag_ranges() {
        try_message(quote! {
            #[bilrost(reserved_tags(1, 2, 3))]
            struct Valid {
                #[bilrost(4)]
                x: String,
            }
        })
        .unwrap();

        try_message(quote! {
            #[bilrost(reserved_tags(1-3, 8-100))]
            struct Valid {
                #[bilrost(4)]
                x: String,
            }
        })
        .unwrap();

        try_message(quote! {
            #[bilrost(reserved_tags(..=3, 8..))]
            struct Valid {
                #[bilrost(4)]
                x: String,
            }
        })
        .unwrap();
    }

    #[test]
    fn test_rejects_colliding_tag_ranges() {
        let output = try_message(quote! {
            #[bilrost(reserved_tags(10, 15, 10))]
            struct Invalid;
        });
        assert_eq!(
            format!(
                "{:#}",
                output.expect_err("colliding reserved tag ranges not detected")
            ),
            "tag 10 is duplicated in tag list"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(1-100, 55))]
            struct Invalid;
        });
        assert_eq!(
            format!(
                "{:#}",
                output.expect_err("colliding reserved tag ranges not detected")
            ),
            "tag 55 is duplicated in tag list"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(1-100, 50-200))]
            struct Invalid;
        });
        assert_eq!(
            format!(
                "{:#}",
                output.expect_err("colliding reserved tag ranges not detected")
            ),
            "tag 50 is duplicated in tag list"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(..=100, 6-10, 2))]
            struct Invalid;
        });
        assert_eq!(
            format!(
                "{:#}",
                output.expect_err("colliding reserved tag ranges not detected")
            ),
            "tag 2 is duplicated in tag list"
        );

        let output = try_message(quote! {
            #[bilrost(reserved_tags(100.., 60, 9999))]
            struct Invalid;
        });
        assert_eq!(
            format!(
                "{:#}",
                output.expect_err("colliding reserved tag ranges not detected")
            ),
            "tag 9999 is duplicated in tag list"
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
            "Oneofs may have at most one empty enum variant. To use multiple variants without \
            fields, the non-empty variants can be marked as values with the 'message' attribute \
            and the empty variant can be either left un-marked or explicitly marked with the \
            'empty' attribute.\n\nThe conflicting variants were: Empty and AlsoEmpty"
        );
    }

    #[test]
    fn test_rejects_meaningless_empty_variant_attrs() {
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(empty, anything_else)]
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
            "the 'empty' attribute is combined with other attributes on variant Empty, but it must \
            always be alone"
        );
    }

    #[test]
    fn test_rejects_meaningless_empty_value_variants() {
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(encoding(X))]
                Empty,
                #[bilrost(1)]
                A(bool),
                #[bilrost(2)]
                B(bool),
            }
        ));
        assert_eq!(
            output
                .expect_err("tagless unit variant not detected")
                .to_string(),
            "missing tag attribute on value variant Empty"
        );
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(tag(0))]
                Empty,
                #[bilrost(1)]
                A(bool),
                #[bilrost(2)]
                B(bool),
            }
        ));
        assert_eq!(
            output.expect_err("unit variant not detected").to_string(),
            "Oneof value variants must have exactly one field, but variant Empty has no fields"
        );
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(tag(0))]
                Empty {},
                #[bilrost(1)]
                A(bool),
                #[bilrost(2)]
                B(bool),
            }
        ));
        assert_eq!(
            output
                .expect_err("brace unit variant not detected")
                .to_string(),
            "Oneof value variants must have exactly one field, but variant Empty has 0 fields"
        );
        let output = try_oneof(quote!(
            enum AB {
                #[bilrost(tag(0))]
                Empty(),
                #[bilrost(1)]
                A(bool),
                #[bilrost(2)]
                B(bool),
            }
        ));
        assert_eq!(
            output
                .expect_err("tuple unit variant not detected")
                .to_string(),
            "Oneof value variants must have exactly one field, but variant Empty has 0 fields"
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
            "missing tag attribute on value variant B"
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
            "Enumerations must have at least one variant"
        );
    }

    #[test]
    fn test_accepts_distinguished_and_borrowed_messages() {
        _ = try_message(quote!(
            #[bilrost(distinguished, borrowed_only)]
            struct DistinguishedBorrowedMessage {
                name: &str,
            }
        ))
        .unwrap();
        _ = try_message(quote!(
            #[bilrost(distinguished, borrowed_only)]
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
            #[bilrost(distinguished, borrowed_only)]
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
                .expect_err("message with duplicated distinguished attributes not detected")
                .to_string(),
            "duplicated distinguished attributes"
        );
        let output = try_message(quote!(
            #[bilrost(borrowed_only, distinguished, borrowed_only)]
            struct DistinguishedBorrowedMessage {
                name: &str,
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated borrowed_only attributes not detected")
                .to_string(),
            "duplicated borrowed_only attributes"
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
                .expect_err("message with duplicated distinguished attributes not detected")
                .to_string(),
            "duplicated distinguished attributes"
        );
        let output = try_message(quote!(
            #[bilrost(borrowed_only, distinguished, borrowed_only)]
            enum DistinguishedBorrowedOneof {
                Empty,
                #[bilrost(1)]
                Name(&str),
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated borrowed_only attributes not detected")
                .to_string(),
            "duplicated borrowed_only attributes"
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
                .expect_err("message with duplicated distinguished attributes not detected")
                .to_string(),
            "duplicated distinguished attributes"
        );
        let output = try_message(quote!(
            #[bilrost(borrowed_only, distinguished, borrowed_only)]
            enum DistinguishedBorrowedOneof {
                #[bilrost(1)]
                Name(&str),
            }
        ));
        assert_eq!(
            output
                .expect_err("message with duplicated borrowed_only attributes not detected")
                .to_string(),
            "duplicated borrowed_only attributes"
        );
    }

    #[test]
    fn test_rejects_duplicated_ignore_attrs() {
        let output = try_message(quote!(
            struct VeryIgnored {
                #[bilrost(ignore, ignore)]
                what: u32,
            }
        ));
        assert_eq!(
            output
                .expect_err("field with duplicated ignore attributes not detected")
                .root_cause()
                .to_string(),
            "invalid field what: duplicated ignore attributes for field: ignore , ignore"
        );
        let output = try_message(quote!(
            enum Thing {
                #[bilrost(tag(1), message)]
                VeryIgnored {
                    #[bilrost(ignore, ignore)]
                    what: u32,
                },
            }
        ));
        assert_eq!(
            output
                .expect_err("field with duplicated ignore attributes not detected")
                .root_cause()
                .to_string(),
            "in message variant VeryIgnored: invalid field what: duplicated ignore attributes \
            for field: ignore , ignore"
        );
        let output = try_message(quote!(
            enum Thing {
                #[bilrost(tag(1), message)]
                VeryIgnored {
                    #[bilrost(ignore, ignore = "1")]
                    what: u32,
                },
            }
        ));
        assert_eq!(
            output
                .expect_err("field with duplicated ignore attributes not detected")
                .root_cause()
                .to_string(),
            "in message variant VeryIgnored: invalid field what: duplicated ignore attributes \
                    for field: ignore , ignore = \"1\""
        );
        let output = try_message(quote!(
            struct MixedIgnores {
                #[bilrost(tag(123), ignore)]
                what: u32,
            }
        ));
        assert_eq!(
            output
                .expect_err("field with duplicated ignore attributes not detected")
                .root_cause()
                .to_string(),
            "invalid field what: ignore attribute mixed with other attributes on the same field: \
            tag (123) , ignore"
        );
        let output = try_message(quote!(
            enum Foo {
                #[bilrost(tag(1), message)]
                MixedIgnores {
                    #[bilrost(tag(123), ignore)]
                    what: u32,
                },
            }
        ));
        assert_eq!(
            output
                .expect_err("field with duplicated ignore attributes not detected")
                .root_cause()
                .to_string(),
            "in message variant MixedIgnores: invalid field what: ignore attribute mixed with \
            other attributes on the same field: tag (123) , ignore"
        );
        let output = try_message(quote!(
            #[bilrost(default_per_field, default_per_field)]
            struct DuplicatedOnStruct {}
        ));
        assert_eq!(
            output
                .expect_err("field with duplicated ignore attributes not detected")
                .to_string(),
            "duplicated default_per_field attributes"
        );
    }

    #[test]
    fn test_field_specific_ignore() {
        let _ = try_message(quote!(
            struct Foo {
                bar: usize,
                #[bilrost(ignore = "5")]
                baz: u32,
                #[bilrost(ignore(wabl()))]
                bear: String,
            }
        ))
        .unwrap();
        let _ = try_message(quote!(
            #[bilrost(default_per_field)]
            struct Foo {
                bar: usize,
                #[bilrost(ignore = "5")]
                baz: u32,
                #[bilrost(ignore(wabl()))]
                bear: String,
            }
        ))
        .unwrap();
        let _ = try_message(quote!(
            #[bilrost(default(Trait::new()))]
            struct Foo {
                bar: usize,
                #[bilrost(ignore = "5")]
                baz: u32,
                #[bilrost(ignore(wabl()))]
                bear: String,
            }
        ))
        .unwrap();
        let _ = try_oneof(quote!(
            enum Foo {
                #[bilrost(tag(1), message)]
                Thing {
                    #[bilrost(ignore = "5")]
                    baz: u32,
                    #[bilrost(ignore(wabl()))]
                    bear: String,
                },
            }
        ))
        .unwrap();
    }

    #[test]
    fn test_conflicting_struct_default_attributes() {
        let output = try_message(quote!(
            #[bilrost(default = "Trait::new()", default_per_field)]
            struct Struct {
                x: usize,
            }
        ));
        assert_eq!(
            output
                .expect_err("conflicting default attributes not detected")
                .to_string(),
            "default_per_field and default (expression) attributes are mutually exclusive"
        );
        let output = try_message(quote!(
            #[bilrost(default = "Trait::new()", default(Trait::even_newer()))]
            struct Struct {
                x: usize,
            }
        ));
        assert_eq!(
            output
                .expect_err("duplicated default attributes not detected")
                .to_string(),
            "duplicated default (expression) attributes: (Trait :: new ()) and \
            (Trait :: even_newer ())"
        );
    }
}
