#!/usr/bin/env bash

# Script which downgrades dependent packages to match our target MSRV.

set -euxo pipefail

cat msrv-pins.txt | while IFS= read -r LINE; do
  CRATE="$(echo "${LINE}" | cut --delimiter=' ' --fields=1)"
  VERSION="$(echo "${LINE}" | cut --delimiter=' ' --fields=2)"
  cargo update --package "${CRATE}" --precise "${VERSION}"
done
