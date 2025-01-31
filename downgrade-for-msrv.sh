#!/usr/bin/env bash

# Script which downgrades dependent packages to match our target MSRV.

set -euxo pipefail

while IFS= read -r LINE; do
  CRATE="$(echo "${LINE}" | cut --delimiter=' ' --fields=1)"
  VERSION="$(echo "${LINE}" | cut --delimiter=' ' --fields=2)"
  cargo update --package "${CRATE}" --precise "${VERSION}"
done < msrv-pins.txt
