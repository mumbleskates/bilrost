use crate::attrs::{
    bilrost_attrs, named_attr, set_bool, set_option, tag_attr, tag_list_attr, word_attr, TagList,
};
use crate::crate_name;
use crate::field::traits::{
    DecodeLifetime::{self, Borrowed, Owned},
    DecodeMode::{self, Distinguished, Relaxed},
    FieldBearer, SinglyTagged, Tagged,
    WhereFor::{self, Decode, Encode},
};
use crate::field::{parse_message_fields, Field, FieldTarget, InitMode, MessageFieldsSorted};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use eyre::{bail, eyre as err, Report as Error};
use itertools::Itertools;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{parse_str, Fields, Ident, Index, Meta, Type, Variant};

/// A field in a bilrost message or oneof
#[derive(Clone)]
pub struct MessageField {
    tag: u32,
    value: ValueField,
    /// In structs, we can create inherent implementations with helper methods for getting and
    /// setting enumeration values through translated types. This is only possible on messages, and
    /// even when "variant types" are available those types are unlikely to be allowed to have impls
    /// of their own.
    enumeration_ty: Option<Type>,
}

#[derive(Clone)]
struct ValueField {
    ty: Type,
    encoding: Type,
    /// If a field is part of a recursion of messages, currently the chain needs to be broken so
    /// that there is not a cyclic dependency of type constraints on the implementation of message
    /// traits. When a field is marked with the "recurses" attribute, it will not be checked in the
    /// `where` clause of the implementation, and the type must always be supported by its encoding.
    recurses: bool,
}

#[derive(Clone)]
pub struct OneofVariant {
    tag: u32,
    variant_ident: Ident,
    contents: VariantContents,
}

#[derive(Clone)]
enum VariantContents {
    Value(Box<FieldInVariant>),
    Message(Vec<Field>),
}

#[derive(Clone)]
struct FieldInVariant {
    ident_within_variant: TokenStream,
    value: ValueField,
}

impl MessageField {
    pub fn new(
        ty: &Type,
        attrs: Vec<Meta>,
        inferred_tag: Option<u32>,
    ) -> Result<Box<MessageField>, Error> {
        let mut tag = None;
        let mut enumeration_ty = None;
        let mut remaining_attrs = vec![];

        for attr in attrs {
            if let Some(t) = tag_attr(&attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if let Some(t) = named_attr(&attr, "enumeration")? {
                set_option(&mut enumeration_ty, t, "duplicate enumeration attributes")?;
            } else {
                remaining_attrs.push(attr);
            }
        }

        let value = ValueField::new(ty, remaining_attrs, "general")?;

        let tag = match tag.or(inferred_tag) {
            Some(tag) => tag,
            None => bail!("missing tag attribute"),
        };

        Ok(Box::new(MessageField {
            tag,
            value,
            enumeration_ty,
        }))
    }

    pub fn ty(&self) -> &Type {
        &self.value.ty
    }

    pub fn has_enumeration_type(&self) -> bool {
        self.enumeration_ty.is_some()
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`. `target`
    /// should be a reference to the field value.
    pub fn encode(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::Encoder<#encoding, #ty>>::encode(#tag, #target, buf, tw);
        }
    }

    /// Returns a statement which encodes the field using buffer `buf` and tag writer `tw`. `target`
    /// should be a reference to the field value.
    pub fn prepend(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::Encoder<#encoding, #ty>>::prepend_encode(
                #tag,
                #target,
                buf,
                tw,
            );
        }
    }

    /// Returns an expression which evaluates to the result of merging a decoded value into the
    /// field. The given ident must be an &mut that already refers to the destination. `target`
    /// should be a mutable reference to the field value.
    pub fn decode(
        &self,
        target: TokenStream,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        let (decoder_trait, call) = match (lifetime, mode) {
            (Owned, Relaxed) => (quote!(Decoder), quote!(decode)),
            (Borrowed, Relaxed) => (quote!(BorrowDecoder), quote!(borrow_decode)),
            (Owned, Distinguished) => (quote!(DistinguishedDecoder), quote!(decode_distinguished)),
            (Borrowed, Distinguished) => (
                quote!(DistinguishedBorrowDecoder),
                quote!(borrow_decode_distinguished),
            ),
        };
        // We need to check the duplicated status of the field ourselves to attach the right field
        // name to the error while decoding.
        quote! {
            if duplicated {
                ::core::result::Result::Err(#crate_::DecodeError::new(
                    #crate_::DecodeErrorKind::UnexpectedlyRepeated
                ))
            } else {
                <() as #crate_::encoding::#decoder_trait<#encoding, #ty>>::#call(
                    wire_type,
                    #target,
                    buf,
                    ctx,
                )
            }
        }
    }

    /// Returns an expression which evaluates to the encoded length of the field. The given ident
    /// must be the location name of the field value, not a reference. `target` should be a
    /// reference to the field value.
    pub fn encoded_len(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::Encoder<#encoding, #ty>>::encoded_len(#tag, #target, tm)
        }
    }

    /// Returns an expression which initializes the field's type as a guaranteed empty value with
    /// its encoding.
    pub fn empty(&self) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote!(<() as #crate_::encoding::EmptyState<#encoding, #ty>>::empty())
    }

    /// Returns an expression which returns whether the field is considered empty in the encoding.
    /// `target` should be a reference to the field value.
    pub fn is_empty(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote!(<() as #crate_::encoding::EmptyState<#encoding, #ty>>::is_empty(#target))
    }

    /// Returns an expression which resets the field's value to empty with its encoding. `target`
    /// should be a mutable reference to the field value.
    pub fn clear(&self, target: TokenStream) -> TokenStream {
        let crate_ = crate_name();
        let encoding = &self.value.encoding;
        let ty = &self.value.ty;
        quote! {
            <() as #crate_::encoding::EmptyState<#encoding, #ty>>::clear(#target);
        }
    }

    /// Returns the where clause constraint terms for the field's encoder.
    pub fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let crate_ = crate_name();
        if self.value.recurses {
            return vec![];
        }
        let ty = &self.value.ty;
        let encoding = &self.value.encoding;
        vec![
            match purpose {
                Encode => quote!((): #crate_::encoding::Encoder<#encoding, #ty>),
                Decode(Owned, Relaxed) => {
                    quote!((): #crate_::encoding::Decoder<#encoding, #ty>)
                }
                Decode(Borrowed, Relaxed) => {
                    quote!((): #crate_::encoding::BorrowDecoder<'__a, #encoding, #ty>)
                }
                Decode(Owned, Distinguished) => {
                    quote!((): #crate_::encoding::DistinguishedDecoder<#encoding, #ty>)
                }
                Decode(Borrowed, Distinguished) => {
                    quote!(
                        (): #crate_::encoding::DistinguishedBorrowDecoder<'__a, #encoding, #ty>
                    )
                }
            },
            // Message field encoding always requires EmptyState instead of just ForOverwrite
            // because we need to know whether a field is empty to know whether we should write
            // anything; and all the decoding traits imply the encoding trait.
            quote!((): #crate_::encoding::EmptyState<#encoding, #ty>),
        ]
    }

    /// Returns methods to embed in the message. `ident` must be the name of the field within the
    /// message struct.
    pub fn methods(&self, ident: &TokenStream) -> Option<TokenStream> {
        let crate_ = crate_name();
        let enumeration_ty = self.enumeration_ty.as_ref()?;

        let ident_str = ident.to_string();
        let ident_str = ident_str.as_str().strip_prefix("r#").unwrap_or(&ident_str);

        // Prepend `get_` for getter methods of tuple structs.
        let get = match parse_str::<Index>(ident_str) {
            Ok(index) => {
                let get = Ident::new(&format!("get_{}", index.index), Span::call_site());
                quote!(#get)
            }
            Err(_) => quote!(#ident),
        };

        let set = Ident::new(&format!("set_{}", ident_str), Span::call_site());

        let field_ty = &self.value.ty;

        Some(quote! {
            fn #get(
                &self
            ) -> <#enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>>::Output {
                <
                    #enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>
                >::help_get(self.#ident)
            }

            fn #set(
                &mut self,
                val: <#enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>>::Input,
            ) {
                self.#ident = <
                    #enumeration_ty as #crate_::encoding::EnumerationHelper<#field_ty>
                >::help_set(val);
            }
        })
    }
}

impl SinglyTagged for MessageField {
    fn tag(&self) -> u32 {
        self.tag
    }
}

impl ValueField {
    fn new(
        ty: &Type,
        attrs: Vec<Meta>,
        implicit_default_encoding: &str,
    ) -> Result<ValueField, Error> {
        let mut encoding = None;
        let mut recurses = false;
        let mut unknown_attrs = Vec::new();

        for attr in &attrs {
            if let Some(t) = named_attr(attr, "encoding")? {
                set_option(&mut encoding, t, "duplicate encoding attributes")?;
            } else if word_attr(attr, "recurses") {
                set_bool(&mut recurses, "duplicate recurses attributes")?;
            } else {
                unknown_attrs.push(attr);
            }
        }

        if !unknown_attrs.is_empty() {
            bail!(
                "unknown attribute(s) for field: {attrs}",
                attrs = quote!(#(#unknown_attrs),*),
            )
        }

        let encoding =
            encoding.unwrap_or_else(|| parse_str::<Type>(implicit_default_encoding).unwrap());

        Ok(ValueField {
            ty: ty.clone(),
            encoding,
            recurses,
        })
    }
}

impl OneofVariant {
    /// Parses values specifically for within a Oneof variant, which works differently than fields
    /// within a Message.
    ///
    /// Returns `Ok` for data variants, and `Err` with just the ident for an empty variant.
    pub fn new(variant: Variant) -> Result<Option<OneofVariant>, Error> {
        let mut tag = None; // tag number
        let mut message = false; // whether this variant is marked as a "message" variant
        let mut empty = false; // whether this unit is marked as an "empty" variant
        let mut other_attrs = vec![];
        let our_attrs = bilrost_attrs(&variant.attrs)?;

        for attr in our_attrs {
            if let Some(t) = tag_attr(&attr)? {
                set_option(&mut tag, t, "duplicate tag attributes")?;
            } else if word_attr(&attr, "message") {
                set_bool(&mut message, "duplicate message attributes")?;
            } else if word_attr(&attr, "empty") {
                set_bool(&mut empty, "duplicate empty attributes")?;
            } else {
                other_attrs.push(attr);
            }
        }

        match (tag, message, empty) {
            // Implicitly or explicitly empty variant
            (None, false, _) if other_attrs.is_empty() => {
                if match variant.fields {
                    Fields::Named(fields) => fields.named.is_empty(),
                    Fields::Unnamed(fields) => fields.unnamed.is_empty(),
                    Fields::Unit => true,
                } {
                    Ok(None)
                } else {
                    // Return a error message depending on whether the variant is explicitly marked
                    // empty
                    if empty {
                        bail!(
                            "Oneof variant {variant_ident} is marked 'empty' but it has fields",
                            variant_ident = variant.ident,
                        );
                    } else {
                        bail!(
                            "missing tag attribute on variant {variant_ident}",
                            variant_ident = variant.ident,
                        );
                    }
                }
            }

            // Empty attribute plus any other attribute
            (_, _, true) => {
                bail!(
                    "the 'empty' attribute is combined with other attributes on variant \
                    {variant_ident}, but it must always be alone",
                    variant_ident = variant.ident,
                );
            }

            // Value variant with missing tag
            (None, false, false) => {
                bail!(
                    "missing tag attribute on value variant {variant_ident}",
                    variant_ident = variant.ident,
                );
            }

            // Normal value variant, neither empty nor message
            (Some(tag), false, false) => {
                let fields = match &variant.fields {
                    Fields::Named(fields) => &fields.named,
                    Fields::Unnamed(fields) => &fields.unnamed,
                    Fields::Unit => bail!(
                        "Oneof value variants must have exactly one field, but variant \
                        {variant_ident} has no fields",
                        variant_ident = variant.ident
                    ),
                };
                if fields.len() != 1 {
                    bail!(
                        "Oneof value variants must have exactly one field, but variant \
                        {variant_ident} has {num_fields} fields",
                        variant_ident = variant.ident,
                        num_fields = fields.len()
                    );
                }
                let field = fields.first().unwrap();
                Ok(Some(OneofVariant {
                    tag,
                    variant_ident: variant.ident.clone(),
                    contents: VariantContents::Value(Box::new(FieldInVariant {
                        value: ValueField::new(&field.ty, other_attrs, "general_packed")?,
                        ident_within_variant: field
                            .ident
                            .as_ref()
                            .map(ToTokens::to_token_stream)
                            .unwrap_or_else(|| quote!(0)),
                    })),
                }))
            }

            // Message variant with missing tag
            (None, true, false) => {
                bail!(
                    "missing tag attribute on message variant {variant_ident}",
                    variant_ident = variant.ident,
                );
            }

            // Message variant
            (Some(tag), true, false) => {
                let mut reserved_tags: Option<TagList> = None;
                let mut unknown_attrs = vec![];
                for attr in &other_attrs {
                    if let Some(tags) = tag_list_attr(attr, "reserved_tags", None)? {
                        set_option(
                            &mut reserved_tags,
                            tags,
                            "duplicate reserved_tags attributes",
                        )?;
                    } else {
                        unknown_attrs.push(attr);
                    }
                }

                if !unknown_attrs.is_empty() {
                    bail!(
                        "unknown or unsupported attribute(s) for message variant {variant_ident}: \
                        {attrs}",
                        variant_ident = variant.ident,
                        attrs = quote!(#(#unknown_attrs),*),
                    );
                }

                let variant_fields =
                    parse_message_fields(variant.fields, InitMode::ParentDefault, reserved_tags)
                        .map_err(|e| {
                            err!(
                                "in message variant {variant_ident}: {e}",
                                variant_ident = variant.ident
                            )
                        })?;

                for field in &variant_fields {
                    if field.has_enumeration_type() {
                        bail!(
                            "in message variant {variant_ident} on field {field_ident}: \
                            enumeration helpers are not supported on messages embedded inside \
                            variants since variants can't have their own methods",
                            variant_ident = variant.ident,
                            field_ident = field.ident(),
                        );
                    }
                }

                Ok(Some(OneofVariant {
                    tag,
                    variant_ident: variant.ident,
                    contents: VariantContents::Message(variant_fields),
                }))
            }
        }
    }

    pub fn ident(&self) -> &Ident {
        &self.variant_ident
    }

    pub fn has_ignored_fields(&self) -> bool {
        matches!(
            &self.contents,
            VariantContents::Message(fields)
            if fields.iter().any(Field::is_ignored)
        )
    }

    /// Generates bindings to a message variant's non-ignored fields.
    fn binding(fields: &[Field]) -> TokenStream {
        let bindings = fields
            .iter()
            .filter(|field| !field.is_ignored())
            .map(|field| {
                let field_ident = field.ident();
                let binding_ident = FieldTarget::free_field_ident(field);
                quote!(#field_ident: #binding_ident)
            });
        quote! { #(#bindings,)* .. }
    }

    pub fn encode(&self, type_ident: impl ToTokens) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let value_ident = &field.ident_within_variant;
                quote! {
                    #type_ident::#variant_ident { #value_ident: value } => {
                        <() as #crate_::encoding::FieldEncoder<#encoding, #ty>>::encode_field(
                            #tag,
                            &value,
                            buf,
                            tw,
                        );
                    }
                }
            }
            VariantContents::Message(fields) => {
                let binding = OneofVariant::binding(fields);
                let encode = MessageFieldsSorted::new_filtering_ignored(fields)
                    .encode(&FieldTarget::BoundVariantFields);
                quote! {
                    #type_ident::#variant_ident { #binding } => {
                        #encode
                    }
                }
            }
        }
    }

    pub fn prepend(&self, type_ident: impl ToTokens) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let value_ident = &field.ident_within_variant;
                quote! {
                    #type_ident::#variant_ident { #value_ident: value } => {
                        <() as #crate_::encoding::FieldEncoder<#encoding, #ty>>::prepend_field(
                            #tag,
                            &value,
                            buf,
                            tw,
                        );
                    }
                }
            }
            VariantContents::Message(fields) => {
                let binding = OneofVariant::binding(fields);
                let prepend = MessageFieldsSorted::new_filtering_ignored(fields)
                    .prepend(&FieldTarget::BoundVariantFields);
                quote! {
                    #type_ident::#variant_ident { #binding } => {
                        #prepend
                    }
                }
            }
        }
    }

    pub fn encoded_len(&self, type_ident: impl ToTokens) -> TokenStream {
        let crate_ = crate_name();
        let tag = self.tag;
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let value_ident = &field.ident_within_variant;
                quote! {
                    #type_ident::#variant_ident { #value_ident: value } => {
                        <() as #crate_::encoding::FieldEncoder<#encoding, #ty>>::field_encoded_len(
                            #tag,
                            &value,
                            tm,
                        )
                    }
                }
            }
            VariantContents::Message(fields) => {
                let binding = OneofVariant::binding(fields);
                let encoded_len = MessageFieldsSorted::new_filtering_ignored(fields)
                    .encoded_len(&FieldTarget::BoundVariantFields);
                quote! {
                    #type_ident::#variant_ident { #binding } => {
                        #encoded_len
                    }
                }
            }
        }
    }

    /// Oneof decoders have four different cases they may be implemented in: implemented for either
    /// NonEmptyOneof or Oneof, and either relaxed or distinguished. The code for these should all
    /// be similarly deduplicated.
    pub fn decode(
        &self,
        type_ident: impl ToTokens,
        lifetime: DecodeLifetime,
        mode: DecodeMode,
    ) -> TokenStream {
        let tag = self.tag;
        let for_overwrite = self.for_overwrite();
        let decode = self.decode_fields(lifetime, mode);
        let construct = self.construct(type_ident);
        let (decode_result, output) = match mode {
            Relaxed => (quote!(()), construct),
            Distinguished => (quote!(canon), quote!((#construct, canon))),
        };
        // It's important that we spell the whole expression for the decoder matching for oneofs as
        // a single Result expression that never early-returns with `?`; that way when we add guards
        // to the Oneof trait impls (which have natural empty variants, a collision guard, and error
        // attribution) our clause that traces the error location will see every error that occurs,
        // including errors that bubble up from the inner decoders, and those error details can
        // still path down through the oneof variant.
        quote! {
            #tag => {
                #for_overwrite
                match #decode {
                    ::core::result::Result::Ok(#decode_result) => {
                        ::core::result::Result::Ok(#output)
                    },
                    ::core::result::Result::Err(error) => ::core::result::Result::Err(error),
                }
            }
        }
    }

    fn for_overwrite(&self) -> TokenStream {
        // TODO: can we clear the variant's field(s) instead if it's already known to be present?
        match &self.contents {
            VariantContents::Value(field) => {
                let crate_ = crate_name();
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                quote! {
                    let mut value =
                        <() as #crate_::encoding::ForOverwrite<#encoding, #ty>>::for_overwrite();
                }
            }
            VariantContents::Message(fields) => {
                let empties = fields.iter().map(|field| {
                    let field_ident = FieldTarget::free_field_ident(field);
                    let empty = field.empty();
                    quote! { let mut #field_ident = #empty; }
                });
                quote! { #(#empties)* }
            }
        }
    }

    fn decode_fields(&self, lifetime: DecodeLifetime, mode: DecodeMode) -> TokenStream {
        let crate_ = crate_name();
        match &self.contents {
            VariantContents::Value(field) => {
                let encoding = &field.value.encoding;
                let ty = &field.value.ty;
                let (decoder_trait, call) = match (lifetime, mode) {
                    (Owned, Relaxed) => (quote!(FieldDecoder), quote!(decode_field)),
                    (Borrowed, Relaxed) => {
                        (quote!(FieldBorrowDecoder), quote!(borrow_decode_field))
                    }
                    (Owned, Distinguished) => (
                        quote!(DistinguishedFieldDecoder),
                        quote!(decode_field_distinguished::<true>), // empty values are ok
                    ),
                    (Borrowed, Distinguished) => (
                        quote!(DistinguishedFieldBorrowDecoder),
                        quote!(borrow_decode_field_distinguished::<true>), // empty values are ok
                    ),
                };
                quote!(
                    <() as #crate_::encoding::#decoder_trait<#encoding, #ty>>::#call(
                        wire_type,
                        &mut value,
                        buf,
                        ctx,
                    )
                )
            }
            VariantContents::Message(fields) => {
                let variant_ident_str = self.variant_ident.to_string();
                let field_arms: Vec<_> = fields
                    .iter()
                    .map(|field| {
                        let tags = field.tags().into_iter().map(|tag| quote!(#tag));
                        let tags = Itertools::intersperse(tags, quote!(|));
                        let decode = field.decode(&FieldTarget::FreeVariantFields, lifetime, mode);
                        let field_ident_str = field.ident.to_string();
                        quote!(#(#tags)* => match #decode {
                            ::core::result::Result::Ok(res) => ::core::result::Result::Ok(res),
                            ::core::result::Result::Err(mut error) => {
                                error.push(#variant_ident_str, #field_ident_str);
                                ::core::result::Result::Err(error)
                            }
                        })
                    })
                    .collect();
                let (result_init, result_update, result_value) = match mode {
                    Relaxed => (None, None, quote!(())),
                    Distinguished => (
                        Some(quote!(let mut canon = #crate_::Canonicity::Canonical;)),
                        Some(quote!(canon.update(ctx.check($crate_::Canonicity::HasExtensions)?);)),
                        quote!(canon),
                    ),
                };
                quote! {
                    // The decoder is wrapped in an IIFE so that we can use the try operator inside.
                    // If we get try blocks in a later edition of rust we can use those
                    // conditionally instead.
                    (|| {
                        let mut outer_buf = buf;
                        // msg_buf is the delimited range of this sub-message
                        let mut msg_buf = outer_buf.take_length_delimited()?;
                        let tr = &mut #crate_::encoding::TagReader::new();
                        let mut last_tag = ::core::option::Option::None::<u32>;
                        #result_init
                        while msg_buf.has_remaining()? {
                            let (tag, wire_type) = tr.decode_key(msg_buf.lend())?;
                            let duplicated = last_tag == ::core::option::Option::Some(tag);
                            last_tag = ::core::option::Option::Some(tag);
                            // individual field decodes will move and consume `buf` and `ctx`,
                            // so we (cheaply) create copies of those values here
                            let buf = msg_buf.lend();
                            let ctx = ctx.clone();
                            match tag {
                                #(#field_arms)*
                                _ => {
                                    #result_update
                                    #crate_::encoding::skip_field(wire_type, buf)
                                },
                            }?;
                        }
                        ::core::result::Result::Ok(#result_value)
                    })()
                }
            }
        }
    }

    fn construct(&self, type_ident: impl ToTokens) -> TokenStream {
        let variant_ident = &self.variant_ident;
        match &self.contents {
            VariantContents::Value(field) => {
                let value_ident = &field.ident_within_variant;
                quote!( #type_ident::#variant_ident { #value_ident: value } )
            }
            VariantContents::Message(fields) => {
                let field_inits = fields
                    .iter()
                    .filter(|field| !field.is_ignored())
                    .map(|field| {
                        let free_ident = FieldTarget::free_field_ident(field);
                        let ident = field.ident();
                        quote!(#ident: #free_ident)
                    });
                // currently, oneof enums never have default-per-field
                let has_ignored_fields = fields.iter().any(Field::is_ignored);
                let maybe_fill_default =
                    has_ignored_fields.then(|| quote!(..::core::default::Default::default()));
                quote! {
                    #type_ident::#variant_ident {
                        #(#field_inits,)*
                        #maybe_fill_default
                    }
                }
            }
        }
    }
}

impl FieldBearer for OneofVariant {
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        match &self.contents {
            VariantContents::Value(field) => field.where_terms(purpose),
            VariantContents::Message(fields) => fields.where_terms(purpose),
        }
    }
}

impl SinglyTagged for OneofVariant {
    fn tag(&self) -> u32 {
        self.tag
    }
}

/// Oneof encoding & decoding bounds for value-variants, which have trivial presence
/// information and must always embed as a single real field and thus need the
/// "value encoder/decoder" traits.
impl FieldBearer for FieldInVariant {
    fn where_terms(&self, purpose: WhereFor) -> Vec<TokenStream> {
        let crate_ = crate_name();
        if self.value.recurses {
            return vec![]; // don't generate constraints for this (sub-)field
        }
        let ty = &self.value.ty;
        let encoding = &self.value.encoding;
        vec![
            match purpose {
                Encode => quote!((): #crate_::encoding::ValueEncoder<#encoding, #ty>),
                Decode(Owned, Relaxed) => {
                    quote!((): #crate_::encoding::ValueDecoder<#encoding, #ty>)
                }
                Decode(Borrowed, Relaxed) => {
                    quote!((): #crate_::encoding::ValueBorrowDecoder<'__a, #encoding, #ty>)
                }
                Decode(Owned, Distinguished) => {
                    quote!((): #crate_::encoding::DistinguishedValueDecoder<#encoding, #ty>)
                }
                Decode(Borrowed, Distinguished) => {
                    quote!(
                        (): #crate_::encoding::
                            DistinguishedValueBorrowDecoder<'__a, #encoding, #ty>
                    )
                }
            },
            // Encoding or decoding a oneof field always has trivially externally determined
            // presence, and we never need to know whether or not the value is empty; it never
            // needs to implement the empty state.
            quote!((): #crate_::encoding::ForOverwrite<#encoding, #ty>),
        ]
    }
}
