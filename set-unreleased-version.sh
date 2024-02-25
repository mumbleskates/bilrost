#!/usr/bin/env bash

# Script which automates modifying source version fields, and creating a
# pre-release commit, without a tag. The commit is not automatically pushed, nor
# are the crates published (see publish-release.sh).

set -euxo pipefail

cd $(dirname $0)

if [ "$#" -ne 1 ]
then
  echo "Usage: $0 <version>"
  exit 1
fi
VERSION="$1"

# Prepend a new section to the changelog
cat <(
  echo "## v${VERSION}"
  echo ""
  echo "### Breaking changes"
  echo ""
  echo "### New features"
  echo ""
  echo "### Fixes"
  echo ""
  echo "### Cleanups"
  echo ""
) CHANGELOG.md > NEW_CHANGELOG.md
mv NEW_CHANGELOG.md CHANGELOG.md

$(dirname $0)/update-version.sh ${VERSION}
git commit -a -m "update version to ${VERSION}"
