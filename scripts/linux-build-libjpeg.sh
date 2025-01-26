#!/bin/bash
set -ex
root=$(realpath "${1:-/}")
path=$(realpath $root/libjpeg)
install=$(realpath "${2:-/opt/pkg-config}")
echo "Building libjpeg in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone --depth 1 --branch 3.1.0 https://github.com/libjpeg-turbo/libjpeg-turbo.git $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $build
cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$install \
      -DBUILD_SHARED_LIBS=OFF $path
cmake --build $build
