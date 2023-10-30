#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::bilrost::Message)]
pub struct OutdirRequest {
    #[bilrost(string, tag = "1")]
    pub query: ::bilrost::alloc::string::String,
    #[bilrost(int32, tag = "2")]
    pub page_number: i32,
    #[bilrost(int32, tag = "3")]
    pub result_per_page: i32,
}
