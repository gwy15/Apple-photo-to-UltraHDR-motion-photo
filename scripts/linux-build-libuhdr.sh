#!/bin/bash
set -ex
path=$(realpath "${1:-/libuhdr}")
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
cmake -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON \
    -DUHDR_BUILD_EXAMPLES=OFF -DUHDR_ENABLE_INSTALL=ON \
    $path
cmake --build $build
