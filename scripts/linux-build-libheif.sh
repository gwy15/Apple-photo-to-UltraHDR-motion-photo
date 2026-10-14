#!/bin/bash
set -ex
root=$(realpath "${1:-/}")
path=$root/libheif
install=$(realpath "${2:-/opt/pkg-config}")
JOBS="${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"
echo "Building libheif in $path"
if [ ! -d "$path" ]; then
    mkdir $path
    git clone --depth 1 --branch v1.19.5 https://github.com/strukturag/libheif.git $path
fi
build=$path/build
if [ ! -d "$build" ]; then
    mkdir $build
fi
cd $build
cmake --preset=release \
    -DBUILD_SHARED_LIBS=OFF -DCMAKE_INSTALL_PREFIX=$install \
    -DWITH_EXAMPLES=OFF -DCMAKE_COMPILE_WARNING_AS_ERROR=ON \
    -DWITH_HEADER_COMPRESSION=OFF -DWITH_UNCOMPRESSED_CODEC=OFF \
    -DWITH_LIBSHARPYUV=OFF -DWITH_FFMPEG_DECODER=OFF \
    -DENABLE_PLUGIN_LOADING=OFF \
    $path
cmake --build $build --parallel "$JOBS"
