#!/usr/bin/env bash

# Script that runs README.md and its code blocks through rustfmt. Requires the nightly toolchain.
# This doesn't actually update the formatting, just prints out any desired changes.

set -euo pipefail

cd $(dirname $0)

cat README.md | (
  while IFS= read -r LINE; do
    if [[ -z ${LINE} ]]; then
      echo "///"
    else
      echo "/// ${LINE}"
    fi
  done
  echo "fn dummy() {}"
) | rustfmt --check --config unstable_features=true,format_code_in_doc_comments=true,max_width=80

# Also check the rest of the repo with format_code_in_doc_comments while we're at it
cargo fmt --check -- --config unstable_features=true,format_code_in_doc_comments=true
