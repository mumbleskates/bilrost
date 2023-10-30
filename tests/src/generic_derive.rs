pub trait CustomType: bilrost::Message + Default {}

impl CustomType for u64 {}

#[derive(Clone, bilrost::Oneof)]
enum GenericEnum<A: CustomType> {
    #[bilrost(message, tag = "1")]
    Data(GenericMessage<A>),
    #[bilrost(uint64, tag = "2")]
    #[allow(dead_code)]
    Number(u64),
}

#[derive(Clone, bilrost::Message)]
struct GenericMessage<A: CustomType> {
    #[bilrost(message, tag = "1")]
    data: Option<A>,
}

#[test]
fn generic_enum() {
    let msg = GenericMessage { data: Some(100u64) };
    let enumeration = GenericEnum::Data(msg);
    match enumeration {
        GenericEnum::Data(d) => assert_eq!(100, d.data.unwrap()),
        GenericEnum::Number(_) => panic!("Not supposed to reach"),
    }
}
