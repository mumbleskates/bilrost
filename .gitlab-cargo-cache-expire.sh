#!/usr/bin/env bash

if [[ \
  !(-f '.cache-info/last-cleaned') || \
  "$(($(date +%s) - $(date +%s -r '.cache-info/last-cleaned')))" -ge "$((7*24*60*60))" \
]] then
  echo "cleaning cargo cache..."
  cargo clean
  mkdir -p .cache-info
  touch .cache-info/last-cleaned
else
  echo "retaining cargo cache"
fi
