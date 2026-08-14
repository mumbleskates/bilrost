#!/usr/bin/env bash

set -euxo pipefail

for feature in std \
               detailed-errors \
               extended-diagnostics \
               arrayvec \
               bstr \
               bytestring \
               chrono \
               hashbrown \
               jiff \
               smallvec \
               smol_str \
               thin-vec \
               time \
               tinyvec \
               self-copy-optimization \
               unroll-varint-encoding \
               ; do
  cargo clippy --workspace --all-targets --no-default-features --features $feature -- -D warnings
  cargo test --workspace --all-targets --no-default-features --features $feature
done
