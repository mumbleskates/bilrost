#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct Container {
    #[bilrost(oneof="container::Data", tags="1, 2")]
    pub data: ::core::option::Option<container::Data>,
}
/// Nested message and enum types in `Container`.
pub mod container {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::bilrost::Oneof)]
    pub enum Data {
        #[bilrost(message, tag="1")]
        Foo(::bilrost::alloc::boxed::Box<super::Foo>),
        #[bilrost(message, tag="2")]
        Bar(super::Bar),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct Foo {
    #[bilrost(string, tag="1")]
    pub foo: ::bilrost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct Bar {
    #[bilrost(message, optional, boxed, tag="1")]
    pub qux: ::core::option::Option<::bilrost::alloc::boxed::Box<Qux>>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct Qux {
}
