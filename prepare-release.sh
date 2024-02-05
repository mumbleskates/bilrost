#!/bin/bash

# Script which automates modifying source version fields, and creating a release
# commit and tag. The commit and tag are not automatically pushed, nor are the
# crates published (see publish-release.sh).

set -euxo pipefail

if [ "$#" -ne 1 ]
then
  echo "Usage: $0 <version>"
  exit 1
fi

$(dirname $0)/update-version.sh $1
git commit -a -m "release ${VERSION}"
git tag -a "v${VERSION}" -m "release ${VERSION}"
