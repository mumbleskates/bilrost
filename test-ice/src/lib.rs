use bilrost::{Message, Oneof};
use std::borrow::Cow;

#[derive(Oneof, Message)]
#[bilrost(borrowed_lifetime('a))]
enum Value<'a> {
    Empty,
    #[bilrost(1)]
    String(Cow<'a, str>),
    #[bilrost(2)]
    List(ListValue<'a>),
}

#[derive(Message)]
#[bilrost(borrowed_lifetime('a))]
struct ListValue<'a> {
    values: Vec<Value<'a>>,
}
