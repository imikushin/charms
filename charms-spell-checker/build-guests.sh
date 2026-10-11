#!/usr/bin/env bash
# Build the zkVM guests and copy the ELFs, keys, and generated sources into the repo.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

docker build -f charms-spell-checker/Dockerfile -t charms-guests .

cid=$(docker create charms-guests)
trap 'docker rm "$cid" >/dev/null' EXIT

mkdir -p src/bin
copy() {
  docker cp "$cid:$1" "$2"
}

copy /out/charms-spell-checker src/bin/charms-spell-checker
copy /out/charms-proof-wrapper src/bin/charms-proof-wrapper
copy /out/charms-spell-checker-vk.bin src/bin/charms-spell-checker-vk.bin
copy /out/charms-proof-wrapper-vk.bin src/bin/charms-proof-wrapper-vk.bin
copy /out/charms-proof-wrapper-lib.rs charms-proof-wrapper/src/lib.rs
copy /out/charms-lib.rs charms-lib/src/lib.rs

chmod +x src/bin/charms-spell-checker src/bin/charms-proof-wrapper
file src/bin/charms-spell-checker src/bin/charms-proof-wrapper
