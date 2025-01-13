#!/usr/bin/env bash

set -euxo pipefail

cd $(dirname $0)
cargo install rustfilt

cargo clean
rm *.profdata *.profraw || echo ok

RUSTFLAGS="-C instrument-coverage" cargo test --features full-test-suite
RUSTFLAGS="-C instrument-coverage" cargo test --features test-suite-extra

llvm-profdata merge -sparse *.profraw -o bilrost-tests.profdata

llvm-cov show --format=html -Xdemangler=rustfilt \
 --instr-profile=bilrost-tests.profdata \
 --object $(ls target/debug/deps/* | \
    grep -P '^target/debug/deps/(bilrost|derived_message_tests)-[0-9a-f]+$') \
 -sources src \
 --show-line-counts-or-regions \
 > bilrost-test-coverage.html
