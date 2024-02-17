#!/usr/bin/env bash

# Script which automates publishing a crates.io release of the bilrost crates.

set -ex

if [ "$#" -ne 0 ]
then
  echo "Usage: $0"
  exit 1
fi

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

CRATES=( \
  "bilrost-derive" \
  "." \
  "bilrost-types" \
)

for CRATE in "${CRATES[@]}"; do
  pushd "$DIR/$CRATE"

  echo "Publishing $CRATE"

  cargo publish

  echo "Sleeping 5 seconds...for the release to be visible"
  sleep 5

  popd
done
