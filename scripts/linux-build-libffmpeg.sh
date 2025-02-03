#!/bin/bash
set -ex
root=$(realpath "${1:-/}")
path=$root/libffmpeg
install=$(realpath "${2:-/opt/pkg-config}")
echo "Building libffmpeg in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone --depth 1 --branch release/7.1 https://github.com/ffmpeg/ffmpeg $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $path
# rsmpeg only supports full build as of now. https://github.com/CCExtractor/rusty_ffmpeg/issues/128
./configure --prefix=$install \
    --disable-programs --disable-doc --disable-network \
    --disable-metal
make -j3
