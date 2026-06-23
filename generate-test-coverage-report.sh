#!/usr/bin/env bash

set -euxo pipefail

cd "$(dirname "$0")"
cargo install rustfilt

cargo clean

mkdir -p coverage/tests/profdata
rm coverage/tests/profdata/* || echo ok

RUSTFLAGS="-C instrument-coverage" cargo test --features full-test-suite,forbid-unsafe

mv ./*.profraw coverage/tests/profdata
llvm-profdata merge -sparse \
 coverage/tests/profdata/*.profraw \
 -o coverage/tests/profdata/merged.profdata

set +x
BINARIES=()
for f in target/debug/deps/*
do
  if echo "$f" | grep -Pq '^target/debug/deps/(bilrost|derived_message_tests)-[0-9a-f]+$' ;
  then
    BINARIES+=("$f")
  fi
done
set -x

llvm-cov show --format=html -Xdemangler=rustfilt \
 --instr-profile=coverage/tests/profdata/merged.profdata \
 -object "${BINARIES[@]}" \
 -sources src \
 --show-line-counts-or-regions \
 --output-dir=coverage/tests
