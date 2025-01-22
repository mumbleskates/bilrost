#!/usr/bin/env bash

# Script for testing that the oldest possible versions of hashbrown are still supported

set -euxo pipefail

cd $(dirname $0)/old-hashbrown-check
cargo update hashbrown@0.15 --precise 0.1.0
cargo test
