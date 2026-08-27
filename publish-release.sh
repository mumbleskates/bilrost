#!/usr/bin/env bash
set -ex
cd "$(dirname "$0")"

cargo publish --workspace
