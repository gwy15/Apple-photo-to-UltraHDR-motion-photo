#!/bin/bash
set -ex
path=$(realpath "${1:-/libheif}")
install=$(realpath "${2:-/usr/local}")
echo "Building libheif in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone https://github.com/strukturag/libheif.git $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $build
cmake --preset=release-noplugins \
    -DBUILD_SHARED_LIBS=OFF -DCMAKE_INSTALL_PREFIX=$install \
    -DWITH_UNCOMPRESSED_CODEC=OFF -DWITH_HEADER_COMPRESSION=OFF \
    -DWITH_AOM_DECODER=OFF -DWITH_AOM_ENCODER=OFF \
    -DWITH_EXAMPLES=OFF \
    $path
cmake --build $build
