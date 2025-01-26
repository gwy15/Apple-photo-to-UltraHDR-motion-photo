#!/bin/bash
set -ex
scripts=$(realpath "$(dirname $0)")
root=/workspaces
install=$root/pkg-config

$scripts/linux-build-libjpeg.sh $root $install
sudo cmake --install $root/libjpeg/build

$scripts/linux-build-libde265.sh $root $install
sudo cmake --install $root/libde265/build

$scripts/linux-build-libheif.sh $root $install
sudo cmake --install $root/libheif/build

$scripts/linux-build-libuhdr.sh $root $install
sudo cmake --install $root/libuhdr/build

env PKG_CONFIG_PATH=$install/lib/pkgconfig PKG_CONFIG_LIBDIR=$install/lib \
    PKG_CONFIG_ALL_STATIC=true \
    TURBOJPEG_STATIC=1 TURBOJPEG_LIB_DIR=$install/lib TURBOJPEG_INCLUDE_PATH=$install/include \
    cargo build --example main --release
ldd target/release/examples/main
