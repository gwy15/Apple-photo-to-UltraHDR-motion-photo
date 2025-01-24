#!/bin/bash
set -ex
path=$(realpath "${1:-/libjpeg}")
echo "Building libjpeg in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone https://github.com/libjpeg-turbo/libjpeg-turbo.git $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $build
cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local $path
cmake --build $build
