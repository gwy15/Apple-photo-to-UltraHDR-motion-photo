#!/bin/bash
set -ex
root=$(realpath "${1:-/}")
path=$root/libuhdr
install=$(realpath "${2:-/opt/pkg-config}")
echo "Building libuhdr in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone --depth 1 --branch v1.4.0 https://github.com/google/libultrahdr.git $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $build
cmake -DCMAKE_BUILD_TYPE=Release -DUHDR_BUILD_EXAMPLES=OFF -DCMAKE_INSTALL_PREFIX=$install \
     -DBUILD_SHARED_LIBS=OFF -DUHDR_ENABLE_INSTALL=ON \
    $path
cmake --build $build
