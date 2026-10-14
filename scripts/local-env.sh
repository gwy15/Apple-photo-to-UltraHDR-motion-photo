#!/bin/bash
# 本地测试/构建所需的环境变量：指向 ./pkg-config 里自建的静态依赖。
# 用法: source scripts/local-env.sh
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
root=$(realpath "$root")
export PKG_CONFIG_PATH="$root/pkg-config/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export PKG_CONFIG_LIBDIR="$root/pkg-config/lib"
export PKG_CONFIG_ALL_STATIC=true
export TURBOJPEG_STATIC=1
export TURBOJPEG_LIB_DIR="$root/pkg-config/lib"
export TURBOJPEG_INCLUDE_PATH="$root/pkg-config/include"
export FFMPEG_PKG_CONFIG_PATH="$root/pkg-config/lib/pkgconfig"
# libheif 静态链接是 C++ 产物，且 Requires.private 里的 libde265 不会由
# libheif-sys 传递链接，需要显式补上。
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=$root/pkg-config/lib -C link-arg=-lstdc++ -C link-arg=-lde265"
