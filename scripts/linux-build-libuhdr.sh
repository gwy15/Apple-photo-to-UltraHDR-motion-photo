#!/bin/bash
set -ex
path=$(realpath "${1:-/libuhdr}")
install=$(realpath "${2:-/usr/local}")
echo "Building libuhdr in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone https://github.com/google/libultrahdr.git $path
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
