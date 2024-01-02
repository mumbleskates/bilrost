#[allow(unused_macros)]
macro_rules! map {
    // TODO(widders): change map configurations
    //  * two Maplike impls similar to Veclike, one for sorted and reversible and one non
    //  * map keys must not recur in any configuration
    //  * maps should be packed! keys and values should directly alternate within a length-
    //    delineated field
    //  * delegate value encoding for maps from General to Map<General, General>

    // TODO(widders): hashbrown support in a feature
    ($map_ty:ident) => {
        use crate::encoding::*;
        use core::hash::Hash;

        /// Generic protobuf map encode function.
        pub fn encode<K, V, B, KE, KL, VE, VL>(
            key_encode: KE,
            key_encoded_len: KL,
            val_encode: VE,
            val_encoded_len: VL,
            tag: u32,
            values: &$map_ty<K, V>,
            buf: &mut B,
            tw: &mut TagWriter,
        ) where
            K: Default + Eq + Hash + Ord,
            V: Default + PartialEq,
            B: BufMut,
            KE: Fn(u32, &K, &mut B, &mut TagWriter),
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VE: Fn(u32, &V, &mut B, &mut TagWriter),
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            encode_with_default(
                key_encode,
                key_encoded_len,
                val_encode,
                val_encoded_len,
                &V::default(),
                tag,
                values,
                buf,
                tw,
            )
        }

        /// Generic protobuf map merge function.
        pub fn merge<K, V, B, KM, VM>(
            key_merge: KM,
            val_merge: VM,
            values: &mut $map_ty<K, V>,
            buf: &mut Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            K: Default + Eq + Hash + Ord,
            V: Default,
            B: Buf,
            KM: Fn(WireType, &mut K, &mut Capped<B>, DecodeContext) -> Result<(), DecodeError>,
            VM: Fn(WireType, &mut V, &mut Capped<B>, DecodeContext) -> Result<(), DecodeError>,
        {
            merge_with_default(key_merge, val_merge, V::default(), values, buf, ctx)
        }

        /// Generic protobuf map encode function.
        pub fn encoded_len<K, V, KL, VL>(
            key_encoded_len: KL,
            val_encoded_len: VL,
            tag: u32,
            values: &$map_ty<K, V>,
            tm: &mut TagMeasurer,
        ) -> usize
        where
            K: Default + Eq + Hash + Ord,
            V: Default + PartialEq,
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            encoded_len_with_default(
                key_encoded_len,
                val_encoded_len,
                &V::default(),
                tag,
                values,
                tm,
            )
        }

        /// Generic protobuf map encode function with an overridden value default.
        ///
        /// This is necessary because enumeration values can have a default value other
        /// than 0 in proto2.
        // TODO(widders): this probably isn't needed actually, due to the above. should enums all
        //  be optional-only?
        pub fn encode_with_default<K, V, B, KE, KL, VE, VL>(
            key_encode: KE,
            key_encoded_len: KL,
            val_encode: VE,
            val_encoded_len: VL,
            val_default: &V,
            tag: u32,
            values: &$map_ty<K, V>,
            buf: &mut B,
            tw: &mut TagWriter,
        ) where
            K: Default + Eq + Hash + Ord,
            V: PartialEq,
            B: BufMut,
            KE: Fn(u32, &K, &mut B, &mut TagWriter),
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VE: Fn(u32, &V, &mut B, &mut TagWriter),
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            for (key, val) in values.iter() {
                let skip_key = key == &K::default();
                let skip_val = val == val_default;
                let inner_tw = &mut TagWriter::new();
                let inner_tm = &mut inner_tw.measurer();

                let len = (if skip_key {
                    0
                } else {
                    key_encoded_len(1, key, inner_tm)
                }) + (if skip_val {
                    0
                } else {
                    val_encoded_len(2, val, inner_tm)
                });

                tw.encode_key(tag, WireType::LengthDelimited, buf);
                encode_varint(len as u64, buf);
                if !skip_key {
                    key_encode(1, key, buf, inner_tw);
                }
                if !skip_val {
                    val_encode(2, val, buf, inner_tw);
                }
            }
        }

        /// Generic protobuf map merge function with an overridden value default.
        ///
        /// This is necessary because enumeration values can have a default value other
        /// than 0 in proto2.
        // TODO(widders): this probably isn't needed actually, due to the above. should enums all
        //  be optional-only?
        pub fn merge_with_default<K, V, B, KM, VM>(
            key_merge: KM,
            val_merge: VM,
            val_default: V,
            values: &mut $map_ty<K, V>,
            buf: &mut Capped<B>,
            ctx: DecodeContext,
        ) -> Result<(), DecodeError>
        where
            K: Default + Eq + Hash + Ord,
            B: Buf,
            KM: Fn(WireType, &mut K, &mut B, DecodeContext) -> Result<(), DecodeError>,
            VM: Fn(WireType, &mut V, &mut B, DecodeContext) -> Result<(), DecodeError>,
        {
            let mut key: K = Default::default();
            let mut val = val_default;
            ctx.limit_reached()?;
            let mut tr = TagReader::new();
            buf.take_length_delimited()?
                .consume(|buf| {
                    let (tag, wire_type) = tr.decode_key(buf)?;
                    // TODO(widders): does this have correct behavior if k or v are incorrectly
                    //  repeated?
                    match tag {
                        1 => key_merge(wire_type, &mut key, buf, ctx.clone()),
                        2 => val_merge(wire_type, &mut val, buf, ctx.clone()),
                        _ => skip_field(wire_type, buf),
                    }
                })
                .collect()?;
            values.insert(key, val);

            Ok(())
        }

        /// Generic protobuf map encode function with an overridden value default.
        ///
        /// This is necessary because enumeration values can have a default value other
        /// than 0 in proto2.
        pub fn encoded_len_with_default<K, V, KL, VL>(
            key_encoded_len: KL,
            val_encoded_len: VL,
            val_default: &V,
            tag: u32,
            values: &$map_ty<K, V>,
            tm: &mut TagMeasurer,
        ) -> usize
        where
            K: Default + Eq + Hash + Ord,
            V: PartialEq,
            KL: Fn(u32, &K, &mut TagMeasurer) -> usize,
            VL: Fn(u32, &V, &mut TagMeasurer) -> usize,
        {
            if values.is_empty() {
                0
            } else {
                // successive repeated keys always take up 1 byte
                tm.key_len(tag) + values.len() - 1
                    + values
                        .iter()
                        .map(|(key, val)| {
                            let inner_tm = &mut TagMeasurer::new();
                            let len = (if key == &K::default() {
                                0
                            } else {
                                key_encoded_len(1, key, inner_tm)
                            }) + (if val == val_default {
                                0
                            } else {
                                val_encoded_len(2, val, inner_tm)
                            });
                            encoded_len_varint(len as u64) + len
                        })
                        .sum::<usize>()
            }
        }
    };
}

// #[cfg(feature = "std")]
// pub mod hash_map {
//     use std::collections::HashMap;
//     map!(HashMap);
// }
//
// pub mod btree_map {
//     map!(BTreeMap);
// }

#[cfg(test)]
mod test {
    /// This big bowl o' macro soup generates an encoding property test for each combination of map
    /// type, scalar map key, and value type.
    /// TODO: these tests take a long time to compile, can this be improved?
    #[allow(unused_macros)]
    macro_rules! map_tests {
        (keys: $keys:tt,
         vals: $vals:tt) => {
            #[cfg(feature = "std")]
            mod hash_map {
                use ::std::collections::HashMap;
                map_tests!(@private HashMap, hash_map, $keys, $vals);
            }
            mod btree_map {
                use ::alloc::collections::BTreeMap;
                map_tests!(@private BTreeMap, btree_map, $keys, $vals);
            }
        };

        (@private $map_type:ident,
                  $mod_name:ident,
                  [$(($key_ty:ty, $key_proto:ident)),*],
                  $vals:tt) => {
            $(
                mod $key_proto {
                    use super::$map_type;

                    use proptest::prelude::*;

                    use crate::encoding::*;
                    use crate::encoding::test::check_collection_type;

                    map_tests!(@private $map_type, $mod_name, ($key_ty, $key_proto), $vals);
                }
            )*
        };

        (@private $map_type:ident,
                  $mod_name:ident,
                  ($key_ty:ty, $key_proto:ident),
                  [$(($val_ty:ty, $val_proto:ident)),*]) => {
            $(
                proptest! {
                    #[test]
                    fn $val_proto(values: $map_type<$key_ty, $val_ty>, tag: u32) {
                        check_collection_type(values, tag, WireType::LengthDelimited,
                                              |tag, values, buf, tw| {
                                                  $mod_name::encode($key_proto::encode,
                                                                    $key_proto::encoded_len,
                                                                    $val_proto::encode,
                                                                    $val_proto::encoded_len,
                                                                    tag,
                                                                    values,
                                                                    buf,
                                                                    tw)
                                              },
                                              |wire_type, values, buf, ctx| {
                                                  check_wire_type(WireType::LengthDelimited, wire_type)?;
                                                  $mod_name::merge($key_proto::merge,
                                                                   $val_proto::merge,
                                                                   values,
                                                                   buf,
                                                                   ctx)
                                              },
                                              |tag, values, tm| {
                                                  $mod_name::encoded_len($key_proto::encoded_len,
                                                                         $val_proto::encoded_len,
                                                                         tag,
                                                                         values,
                                                                         tm)
                                              })?;
                    }
                }
             )*
        };
    }

    // TODO(widders): tests
    // map_tests!(keys: [
    //     (u32, uint32),
    //     (u64, uint64),
    //     (i32, sint32),
    //     (i64, sint64),
    //     (u32, ufixed32),
    //     (u64, ufixed64),
    //     (i32, sfixed32),
    //     (i64, sfixed64),
    //     (bool, bool),
    //     (String, string),
    //     (Vec<u8>, bytes)
    // ],
    // vals: [
    //     (f32, float32),
    //     (f64, float64),
    //     (u32, uint32),
    //     (u64, uint64),
    //     (i32, sint32),
    //     (i64, sint64),
    //     (u32, ufixed32),
    //     (u64, ufixed64),
    //     (i32, sfixed32),
    //     (i64, sfixed64),
    //     (bool, bool),
    //     (String, string),
    //     (Vec<u8>, bytes)
    // ]);
}
