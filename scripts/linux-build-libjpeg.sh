#!/bin/bash
set -ex
path=$(realpath "${1:-/libjpeg}")
install=$(realpath "${2:-/usr/local}")
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
cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$install $path
cmake --build $build
