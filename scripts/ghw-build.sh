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

$scripts/linux-build-libffmpeg.sh $root $install
cd $root/libffmpeg && make install && cd $scripts/..

env PKG_CONFIG_PATH=$install/lib/pkgconfig PKG_CONFIG_LIBDIR=$install/lib \
    PKG_CONFIG_ALL_STATIC=true \
    TURBOJPEG_STATIC=1 TURBOJPEG_LIB_DIR=$install/lib TURBOJPEG_INCLUDE_PATH=$install/include \
    FFMPEG_PKG_CONFIG_PATH=$install/lib/pkgconfig \
    cargo build --example aa-photo-bridge --release
ldd target/release/examples/aa-photo-bridge
