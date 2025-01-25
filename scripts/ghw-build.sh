#!/bin/bash
set -ex
scripts=$(realpath "$(dirname $0)")
install=$(realpath ../pkg-config)

$scripts/linux-build-libjpeg.sh /workspaces/libjpeg $install
sudo cmake --install /workspaces/libjpeg/build

$scripts/linux-build-libheif.sh /workspaces/libheif $install
sudo cmake --install /workspaces/libheif/build

$scripts/linux-build-libuhdr.sh /workspaces/libuhdr $install
sudo cmake --install /workspaces/libuhdr/build

env PKG_CONFIG_PATH=$install/lib/pkgconfig PKG_CONFIG_LIBDIR=$install/lib \
    PKG_CONFIG_ALL_STATIC=true \
    cargo build --example main --release
ldd target/release/examples/main
