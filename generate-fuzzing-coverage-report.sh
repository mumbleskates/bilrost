#!/usr/bin/env bash

set -euxo --pipefail

cd "$(dirname "$0")"
cargo install rustfilt

cargo fuzz coverage bilrost_fuzz

llvm-cov show --format=html -Xdemangler=rustfilt \
 --instr-profile=fuzz/coverage/bilrost_fuzz/coverage.profdata \
 -object target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/bilrost_fuzz \
 -sources src bilrost-types \
 --output-dir=coverage/bilrost_fuzz
