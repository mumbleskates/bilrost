#!/usr/bin/env bash

set -euxo pipefail

cp Cargo.lock.msrv Cargo.lock
cp old-hashbrown-check/Cargo.lock.msrv old-hashbrown-check/Cargo.lock
