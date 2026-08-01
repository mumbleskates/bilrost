#!/usr/bin/env bash

set -euxo pipefail

cp Cargo.lock.msrv Cargo.lock
cp test-old-hashbrown/Cargo.lock.msrv test-old-hashbrown/Cargo.lock
