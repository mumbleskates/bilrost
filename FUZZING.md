# Fuzzing

Bilrost ships a few fuzz tests, using both libfuzzer and aflfuzz.

To run the libfuzzer tests, first install cargo-fuzz:

    cargo install cargo-fuzz

Then the fuzzers can be run:

    cargo fuzz run <fuzzer> -- <flags>

The following fuzzers are available:

* `bilrost_fuzz`: core bilrost functionality and parsing
* `bilrost_borrowed_fuzz`: some very basic tests of bilrost borrowed decoding
  functionality on `Cow<T>` types
* `bilrost_type_support_fuzz`: extended bilrost type support for third party
  types
* `parse_date_fuzz`: tests the conversions to and from strings for `Timestamp`
  and `Duration` in `bilrost-types`.

Example invocation:

    cargo fuzz run bilrost_fuzz -- -fork=12

See [the libfuzzer docs](https://llvm.org/docs/LibFuzzer.html) for options and
further info.

To run the afl fuzz tests, first install [cargo-afl][afl]:

    cargo install cargo-afl

[afl]: https://rust-fuzz.github.io/book/afl/tutorial.html

Then build a fuzz target and run afl on it:

    cd fuzz
    cargo afl build --package fuzz-afl --bin bilrost_afl
    cargo afl fuzz -i afl/in -o afl/out target/debug/bilrost_afl

To reproduce a crash, use the `reproduce` binary in the "fuzz/common" directory:

    cd fuzz
    cargo run --package common --bin reproduce -- <crashfile>
