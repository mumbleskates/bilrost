#!/usr/bin/env bash

set -euxo pipefail

cd "$(dirname "$0")"
cargo install rustfilt

cargo clean

mkdir -p coverage/tests/profdata
rm coverage/tests/profdata/* || echo ok

RUSTFLAGS="-C instrument-coverage" cargo test --features full-test-suite

mv ./*.profraw coverage/tests/profdata
llvm-profdata merge -sparse \
 coverage/tests/profdata/*.profraw \
 -o coverage/tests/profdata/merged.profdata

llvm-cov show --format=html -Xdemangler=rustfilt \
 --instr-profile=coverage/tests/profdata/merged.profdata \
 -object "$(ls target/debug/deps/* | \
    grep -P '^target/debug/deps/(bilrost|derived_message_tests)-[0-9a-f]+$')" \
 -sources src \
 --show-line-counts-or-regions \
 --output-dir=coverage/tests
