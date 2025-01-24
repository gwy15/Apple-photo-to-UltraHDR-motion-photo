#!/bin/bash
set -ex
scripts=$(realpath "$(dirname $0)")

$scripts/linux-build-libjpeg.sh /workspaces/libjpeg
cmake --install /workspaces/libjpeg/build

$scripts/linux-build-libheif.sh /workspaces/libheif
cmake --install /workspaces/libheif/build

$scripts/linux-build-libuhdr.sh /workspaces/libuhdr
cmake --install /workspaces/libuhdr/build

env PKG_CONFIG_ALL_STATIC=true ULTRAHDR_STATIC=true \
    cargo build --example main --release
