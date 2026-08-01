use bilrost::Message;
use hashbrown::{HashMap, HashSet};

#[derive(Debug, Default, PartialEq, Message)]
struct VeryOldHashbrown {
    map: HashMap<u64, String>,
    set: HashSet<i32>,
}

#[cfg(test)]
#[test]
fn old_hashbrown_encoding_functions() {
    use bilrost::OwnedMessage;

    let original = VeryOldHashbrown {
        map: HashMap::from_iter([(1, "hello".to_owned()), (2, "world".to_owned())]),
        set: HashSet::from_iter([3, 4]),
    };
    let encoded = original.encode_to_vec();
    let decoded = VeryOldHashbrown::decode(encoded.as_slice()).unwrap();

    assert_eq!(original, decoded);
}
