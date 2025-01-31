#!/usr/bin/env bash

# Script for testing that the oldest possible versions of hashbrown are still supported

set -euxo pipefail

cd "$(dirname "$0")/old-hashbrown-check"

HIGH_HASHBROWN_VERSION=$(cargo tree \
 --quiet \
 --prefix=none \
 --edges=normal,dev,no-proc-macro \
 --format='{p}' \
 --no-dedupe \
 | grep -P '^hashbrown\b' | sort -n | uniq | tail -n 1 | grep -Po '\d+\.\d+\.\d+$')

cargo update --package "hashbrown@$HIGH_HASHBROWN_VERSION" --precise 0.1.0
cargo test
