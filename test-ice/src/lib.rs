struct Value<'a>(&'a ());
impl<'a> ::bilrost::encoding::Oneof for Value<'a>
where
    (): ::bilrost::encoding::ForOverwrite<::bilrost::encoding::General, ListValue<'a>>,
    (): ::bilrost::encoding::ValueEncoder<::bilrost::encoding::General, ListValue<'a>>,
{
    const FIELD_TAGS: &'static [u32] = &[1u32, 2u32];
    fn empty() -> Self {
        panic!()
    }
    fn is_empty(&self) -> bool {
        panic!()
    }
    fn clear(&mut self) {
        panic!()
    }
    fn oneof_encode<__B: ::bilrost::bytes::BufMut + ?Sized>(
        &self,
        buf: &mut __B,
        tw: &mut ::bilrost::encoding::TagWriter,
    ) {
        panic!()
    }
    fn oneof_prepend<__B: ::bilrost::buf::ReverseBuf + ?Sized>(
        &self,
        buf: &mut __B,
        tw: &mut ::bilrost::encoding::TagRevWriter,
    ) {
        panic!()
    }
    fn oneof_encoded_len(&self, tm: &mut impl ::bilrost::encoding::TagMeasurer) -> usize {
        panic!()
    }
    fn oneof_current_tag(&self) -> ::core::option::Option<u32> {
        panic!()
    }
    fn oneof_variant_name(tag: u32) -> (&'static str, &'static str) {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::OneofDecoder for Value<'a>
where
    (): ::bilrost::encoding::ForOverwrite<::bilrost::encoding::General, ListValue<'a>>,
    (): ::bilrost::encoding::ValueDecoder<::bilrost::encoding::General, ListValue<'a>>,
{
    fn oneof_decode_field<__B: ::bilrost::bytes::Buf + ?Sized>(
        value: &mut Self,
        tag: u32,
        wire_type: ::bilrost::encoding::WireType,
        buf: ::bilrost::encoding::Capped<__B>,
        ctx: impl ::bilrost::encoding::DecodeContext,
    ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::OneofBorrowDecoder<'a> for Value<'a>
where
    (): ::bilrost::encoding::ForOverwrite<::bilrost::encoding::General, ListValue<'a>>,
    (): ::bilrost::encoding::ValueBorrowDecoder<'a, ::bilrost::encoding::General, ListValue<'a>>,
{
    fn oneof_borrow_decode_field(
        value: &mut Self,
        tag: u32,
        wire_type: ::bilrost::encoding::WireType,
        buf: ::bilrost::encoding::Capped<&'a [u8]>,
        ctx: impl ::bilrost::encoding::DecodeContext,
    ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::RawMessage for Value<'a>
where
    Value<'a>: ::bilrost::encoding::Oneof,
{
    const __ASSERTIONS: () = ();

    fn empty() -> Self {
        panic!()
    }

    fn is_empty(&self) -> bool {
        panic!()
    }

    fn clear(&mut self) {
        panic!()
    }

    fn raw_encode<__B>(&self, buf: &mut __B)
    where
        __B: ::bilrost::bytes::BufMut + ?Sized,
    {
        panic!()
    }

    fn raw_prepend<__B>(&self, buf: &mut __B)
    where
        __B: ::bilrost::buf::ReverseBuf + ?Sized,
    {
        panic!()
    }

    fn raw_encoded_len(&self) -> usize {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::ForOverwrite<(), Value<'a>> for ()
where
    Value<'a>: ::bilrost::encoding::Oneof,
{
    fn for_overwrite() -> Value<'a> {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::EmptyState<(), Value<'a>> for ()
where
    Value<'a>: ::bilrost::encoding::Oneof,
{
    fn is_empty(val: &Value<'a>) -> bool {
        panic!()
    }

    fn clear(val: &mut Value<'a>) {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::RawMessageDecoder for Value<'a>
where
    Value<'a>: ::bilrost::encoding::OneofDecoder,
{
    fn raw_decode_field<__B>(
        &mut self,
        tag: u32,
        wire_type: ::bilrost::encoding::WireType,
        _duplicated: bool,
        buf: ::bilrost::encoding::Capped<__B>,
        ctx: impl ::bilrost::encoding::DecodeContext,
    ) -> ::core::result::Result<(), ::bilrost::DecodeError>
    where
        __B: ::bilrost::bytes::Buf + ?Sized,
    {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::RawMessageBorrowDecoder<'a> for Value<'a>
where
    Value<'a>: ::bilrost::encoding::OneofBorrowDecoder<'a>,
{
    fn raw_borrow_decode_field(
        &mut self,
        tag: u32,
        wire_type: ::bilrost::encoding::WireType,
        _duplicated: bool,
        buf: ::bilrost::encoding::Capped<&'a [u8]>,
        ctx: impl ::bilrost::encoding::DecodeContext,
    ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
        panic!()
    }
}

struct ListValue<'a>(&'a ());
impl<'a> ::bilrost::encoding::RawMessage for ListValue<'a>
where
    (): ::bilrost::encoding::EmptyState<::bilrost::encoding::General, Vec<Value<'a>>>,
    (): ::bilrost::encoding::Encoder<::bilrost::encoding::General, Vec<Value<'a>>>,
{
    const __ASSERTIONS: () = ();
    fn empty() -> Self {
        panic!()
    }
    fn is_empty(&self) -> bool {
        panic!()
    }
    fn clear(&mut self) {
        panic!()
    }

    fn raw_encode<__B>(&self, buf: &mut __B)
    where
        __B: ::bilrost::bytes::BufMut + ?Sized,
    {
        panic!()
    }

    fn raw_prepend<__B>(&self, buf: &mut __B)
    where
        __B: ::bilrost::buf::ReverseBuf + ?Sized,
    {
        panic!()
    }

    fn raw_encoded_len(&self) -> usize {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::RawMessageDecoder for ListValue<'a>
where
    (): ::bilrost::encoding::Decoder<::bilrost::encoding::General, Vec<Value<'a>>>,
    (): ::bilrost::encoding::EmptyState<::bilrost::encoding::General, Vec<Value<'a>>>,
{
    fn raw_decode_field<__B>(
        &mut self,
        tag: u32,
        wire_type: ::bilrost::encoding::WireType,
        duplicated: bool,
        buf: ::bilrost::encoding::Capped<__B>,
        ctx: impl ::bilrost::encoding::DecodeContext,
    ) -> ::core::result::Result<(), ::bilrost::DecodeError>
    where
        __B: ::bilrost::bytes::Buf + ?Sized,
    {
        panic!()
    }
}
impl<'a> ::bilrost::encoding::RawMessageBorrowDecoder<'a> for ListValue<'a>
where
    (): ::bilrost::encoding::BorrowDecoder<'a, ::bilrost::encoding::General, Vec<Value<'a>>>,
    (): ::bilrost::encoding::EmptyState<::bilrost::encoding::General, Vec<Value<'a>>>,
{
    fn raw_borrow_decode_field(
        &mut self,
        tag: u32,
        wire_type: ::bilrost::encoding::WireType,
        duplicated: bool,
        buf: ::bilrost::encoding::Capped<&'a [u8]>,
        ctx: impl ::bilrost::encoding::DecodeContext,
    ) -> ::core::result::Result<(), ::bilrost::DecodeError> {
        panic!()
    }
}

impl<'a> ::bilrost::encoding::ForOverwrite<(), ListValue<'a>> for ()
where
    (): ::bilrost::encoding::EmptyState<::bilrost::encoding::General, Vec<Value<'a>>>,
    (): ::bilrost::encoding::Encoder<::bilrost::encoding::General, Vec<Value<'a>>>,
{
    fn for_overwrite() -> ListValue<'a> {
        <ListValue<'a> as ::bilrost::encoding::RawMessage>::empty()
    }
}
impl<'a> ::bilrost::encoding::EmptyState<(), ListValue<'a>> for ()
where
    (): ::bilrost::encoding::EmptyState<::bilrost::encoding::General, Vec<Value<'a>>>,
    (): ::bilrost::encoding::Encoder<::bilrost::encoding::General, Vec<Value<'a>>>,
{
    fn is_empty(val: &ListValue<'a>) -> bool {
        <ListValue<'a> as ::bilrost::encoding::RawMessage>::is_empty(val)
    }
    fn clear(val: &mut ListValue<'a>) {
        <ListValue<'a> as ::bilrost::encoding::RawMessage>::clear(val);
    }
}
