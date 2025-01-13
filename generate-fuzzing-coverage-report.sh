#!/usr/bin/env bash
cd $(dirname $0)
cargo install rustfilt

cargo fuzz coverage bilrost_fuzz

llvm-cov show --format=html -Xdemangler=rustfilt \
 --instr-profile=fuzz/coverage/bilrost_fuzz/coverage.profdata \
 target/x86_64-unknown-linux-gnu/coverage/x86_64-unknown-linux-gnu/release/bilrost_fuzz \
 src bilrost-types \
 > coverage.html
