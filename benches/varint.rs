use std::iter;

use bilrost::buf::ReverseBuffer;
use bilrost::encoding::{
    const_varint, decode_varint, encode_varint, encoded_len_varint, prepend_varint, Capped,
    TagReader,
};
use bytes::Buf;
use criterion::{Criterion, Throughput};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

fn benchmark_varint(criterion: &mut Criterion, name: &str, mut values: Vec<u64>) {
    // Shuffle the values in a stable order.
    values.shuffle(&mut StdRng::seed_from_u64(0));
    let name = format!("varint/{}", name);

    let encoded_len = values
        .iter()
        .cloned()
        .map(encoded_len_varint)
        .sum::<usize>() as u64;

    criterion
        .benchmark_group(&name)
        .throughput(Throughput::Bytes(encoded_len))
        .bench_function("encode", {
            let encode_values = values.clone();
            move |b| {
                let mut buf = Vec::<u8>::with_capacity(encode_values.len() * 10);
                b.iter(|| {
                    buf.clear();
                    for &value in &encode_values {
                        encode_varint(value, &mut buf);
                    }
                    criterion::black_box(&buf);
                })
            }
        });

    criterion
        .benchmark_group(&name)
        .throughput(Throughput::Bytes(encoded_len))
        .bench_function("prepend", {
            let encode_values = values.clone();
            move |b| {
                let mut buf = ReverseBuffer::with_capacity(encode_values.len() * 10);
                b.iter(|| {
                    buf.clear();
                    for &value in &encode_values {
                        prepend_varint(value, &mut buf);
                    }
                    criterion::black_box(&buf);
                })
            }
        });

    criterion
        .benchmark_group(&name)
        .throughput(Throughput::Bytes(encoded_len))
        .bench_function("decode", {
            let decode_values = values.clone();

            move |b| {
                let mut buf = Vec::with_capacity(decode_values.len() * 10);
                for &value in &decode_values {
                    encode_varint(value, &mut buf);
                }

                b.iter(|| {
                    let mut buf = &mut buf.as_slice();
                    while buf.has_remaining() {
                        let result = decode_varint(&mut buf);
                        debug_assert!(result.is_ok());
                        criterion::black_box(&result);
                    }
                })
            }
        });

    criterion
        .benchmark_group(&name)
        .throughput(Throughput::Elements(values.len() as u64))
        .bench_function("encoded_len", move |b| {
            b.iter(|| {
                let mut sum = 0;
                for &value in &values {
                    sum += encoded_len_varint(value);
                }
                criterion::black_box(sum);
            })
        });
}

fn benchmark_decode_key(criterion: &mut Criterion, name: &str, mut values: Vec<u64>) {
    // Shuffle the values in a stable order.
    values.shuffle(&mut StdRng::seed_from_u64(0));
    let name = format!("field/{}", name);

    let encoded_len = values
        .iter()
        .cloned()
        .map(encoded_len_varint)
        .sum::<usize>() as u64;

    criterion
        .benchmark_group(&name)
        .throughput(Throughput::Bytes(encoded_len))
        .bench_function("decode_key", {
            let decode_values = values.clone();

            move |b| {
                let mut buf = Vec::with_capacity(decode_values.len() * 9);
                for &value in &decode_values {
                    encode_varint(value, &mut buf);
                }

                b.iter(|| {
                    let mut to_decode = buf.as_slice();
                    let mut buf = Capped::new(&mut to_decode);
                    while buf.remaining() > 0 {
                        let result = TagReader::new().decode_key(buf.lend());
                        debug_assert!(result.is_ok());
                        criterion::black_box(&result);
                    }
                })
            }
        });
}

fn assert_all_sized(
    vals: impl IntoIterator<Item = u64>,
    varint_len: usize,
) -> impl Iterator<Item = u64> {
    vals.into_iter().inspect(move |&val| {
        assert_eq!(const_varint(val).len(), varint_len);
    })
}

fn main() {
    let criterion = Criterion::default();
    #[cfg(feature = "pprof")]
    let criterion = criterion.with_profiler(profiling::FlamegraphProfiler::new(1000));
    let mut criterion = criterion.configure_from_args();

    // Benchmark encoding and decoding 100 small (1 byte) varints.
    benchmark_varint(
        &mut criterion,
        "small-1",
        assert_all_sized(0..100, 1).collect(),
    );

    // Benchmark encoding and decoding 100 medium (2 byte) varints.
    benchmark_varint(
        &mut criterion,
        "medium-2",
        assert_all_sized((200..).take(100), 2).collect(),
    );

    // Benchmark encoding and decoding 100 medium (3 byte) varints.
    benchmark_varint(
        &mut criterion,
        "medium-3",
        assert_all_sized((1 << 20..).take(100), 3).collect(),
    );

    // Benchmark encoding and decoding 100 medium (4 byte) varints.
    benchmark_varint(
        &mut criterion,
        "medium-4",
        assert_all_sized((1 << 25..).take(100), 4).collect(),
    );

    // Benchmark encoding and decoding 100 medium (5 byte) varints.
    benchmark_varint(
        &mut criterion,
        "medium-5",
        assert_all_sized((1 << 30..).take(100), 5).collect(),
    );

    // Benchmark encoding and decoding 100 medium (6 byte) varints.
    benchmark_varint(
        &mut criterion,
        "medium-6",
        assert_all_sized((1 << 40..).take(100), 6).collect(),
    );

    // Benchmark encoding and decoding 100 medium (7 byte) varints.
    benchmark_varint(
        &mut criterion,
        "medium-7",
        assert_all_sized((1 << 45..).take(100), 7).collect(),
    );

    // Benchmark encoding and decoding 100 large (8 byte) varints.
    benchmark_varint(
        &mut criterion,
        "large-8",
        assert_all_sized((1 << 50..).take(100), 8).collect(),
    );

    // Benchmark encoding and decoding 100 large (9 byte) varints.
    benchmark_varint(
        &mut criterion,
        "large-9",
        assert_all_sized((1 << 63..).take(100), 9).collect(),
    );

    // Benchmark encoding and decoding 100 varints of mixed width (average 5.5 bytes).
    benchmark_varint(
        &mut criterion,
        "mixed",
        (0..9)
            .flat_map(move |width| {
                let exponent = width * 7 + 1;
                (0..11).map(move |offset| offset + (1 << exponent))
            })
            .chain(iter::once(1))
            .collect(),
    );

    // Benchmark encoding and decoding 100 small (1 byte) field keys.
    benchmark_decode_key(&mut criterion, "small", (0..100).collect());

    // Benchmark encoding and decoding 100 medium (5 byte) field keys.
    benchmark_decode_key(&mut criterion, "medium", (1 << 28..).take(100).collect());

    criterion.final_summary();
}

#[cfg(feature = "pprof")]
mod profiling {
    use criterion::profiler::Profiler;
    use pprof::ProfilerGuard;
    use std::ffi::c_int;
    use std::fs::File;
    use std::path::Path;

    pub struct FlamegraphProfiler<'a> {
        frequency: c_int,
        active_profiler: Option<ProfilerGuard<'a>>,
    }

    impl FlamegraphProfiler<'_> {
        pub fn new(frequency: c_int) -> Self {
            FlamegraphProfiler {
                frequency,
                active_profiler: None,
            }
        }
    }

    impl Profiler for FlamegraphProfiler<'_> {
        fn start_profiling(&mut self, _benchmark_id: &str, _benchmark_dir: &Path) {
            self.active_profiler = Some(ProfilerGuard::new(self.frequency).unwrap());
        }

        fn stop_profiling(&mut self, _benchmark_id: &str, benchmark_dir: &Path) {
            std::fs::create_dir_all(benchmark_dir).unwrap();
            let flamegraph_path = benchmark_dir.join("flamegraph.svg");
            let flamegraph_file = File::create(flamegraph_path)
                .expect("File system error while creating flamegraph.svg");
            if let Some(profiler) = self.active_profiler.take() {
                profiler
                    .report()
                    .build()
                    .unwrap()
                    .flamegraph(flamegraph_file)
                    .expect("Error writing flamegraph");
            }
        }
    }
}
