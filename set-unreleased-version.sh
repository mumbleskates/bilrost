#!/bin/bash

# Script which automates modifying source version fields, and creating a
# pre-release commit, without a tag. The commit is not automatically pushed, nor
# are the crates published (see publish-release.sh).

set -euxo pipefail

if [ "$#" -ne 1 ]
then
  echo "Usage: $0 <version>"
  exit 1
fi

$(dirname $0)/update-version.sh $1
git commit -a -m "update version to ${VERSION}"