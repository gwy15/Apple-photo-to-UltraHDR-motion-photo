#!/bin/bash
set -ex
root=$(realpath "${1:-/}")
path=$(realpath $root/libde265)
install=$(realpath "${2:-/opt/pkg-config}")
echo "Building libheif in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone --depth 1 --branch v1.0.15 https://github.com/strukturag/libde265.git $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $build
cmake -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
    -DCMAKE_INSTALL_PREFIX=$install \
    $path
cmake --build $build
