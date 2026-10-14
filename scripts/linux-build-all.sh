#!/bin/bash
# 构建 aa-photo-bridge 的全部 C/C++ 依赖，按依赖 DAG 分两阶段并行。
#
# 用法: ./scripts/linux-build-all.sh [deps_dir] [install_dir]
#   deps_dir     上游源码与构建目录，默认 deps
#   install_dir  安装前缀，默认 pkg-config
#
# 并行度由环境变量 JOBS 控制，默认 nproc。
#
# 依赖关系（由各上游 CMakeLists 确认）:
#   阶段 A: libjpeg   libde265   libffmpeg     （三者互相独立）
#   阶段 B: libuhdr   libheif                  （uhdr 需 jpeg；heif 需 jpeg + de265）
set -euo pipefail

mkdir -p "${1:-deps}" "${2:-pkg-config}"
root=$(realpath "${1:-deps}")
install=$(realpath "${2:-pkg-config}")
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

JOBS="${JOBS:-$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)}"
export JOBS

# 让后续依赖能通过 pkg-config / CMake 找到已装进 $install 的库。
# 尤其是 libheif：它的 cmake/modules/FindLIBDE265.cmake 只通过 pkg-config 找
# libde265，缺了 PKG_CONFIG_PATH 会静默编出一个没有 HEVC 解码能力的 libheif，
# 运行时解 HEIC 直接失败。
export PKG_CONFIG_PATH="$install/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export CMAKE_PREFIX_PATH="$install${CMAKE_PREFIX_PATH:+:$CMAKE_PREFIX_PATH}"

echo "==> JOBS=$JOBS  deps=$root  prefix=$install"

# 跑某个依赖的构建脚本，成功后安装到 $install。输出全部落到 $root/<name>.log。
build() {
    local name=$1 script=$2 build_dir=$3 install_cmd=$4
    local log="$root/$name.log"
    echo "==> [$name] 开始，日志 $log"
    if ! "$script_dir/$script" "$root" "$install" >"$log" 2>&1; then
        echo "!!! [$name] 构建失败，见 $log" >&2
        return 1
    fi
    if ! eval "$install_cmd" >>"$log" 2>&1; then
        echo "!!! [$name] 安装失败，见 $log" >&2
        return 1
    fi
    echo "==> [$name] 完成"
}

# 并发跑若干个 build 调用，全部结束后汇总退出码。
parallel() {
    local pids=() names=()
    for spec in "$@"; do
        names+=("${spec%%|*}")
        ( eval "${spec#*|}" ) &
        pids+=($!)
    done
    local failed=0
    for i in "${!pids[@]}"; do
        wait "${pids[$i]}" || { echo "!!! ${names[$i]} 失败" >&2; failed=1; }
    done
    return $failed
}

echo "==> 阶段 A: libjpeg / libde265 / libffmpeg"
parallel \
    "libjpeg|build libjpeg linux-build-libjpeg.sh \"$root/libjpeg/build\" 'cmake --install \"$root/libjpeg/build\"'" \
    "libde265|build libde265 linux-build-libde265.sh \"$root/libde265/build\" 'cmake --install \"$root/libde265/build\"'" \
    "libffmpeg|build libffmpeg linux-build-libffmpeg.sh \"$root/libffmpeg/build\" 'make -C \"$root/libffmpeg\" install'"

echo "==> 阶段 B: libuhdr / libheif"
parallel \
    "libuhdr|build libuhdr linux-build-libuhdr.sh \"$root/libuhdr/build\" 'cmake --install \"$root/libuhdr/build\"'" \
    "libheif|build libheif linux-build-libheif.sh \"$root/libheif/build\" 'cmake --install \"$root/libheif/build\"'"

echo "==> 校验 pkg-config 可见性"
for p in libjpeg libturbojpeg libde265 libheif libuhdr; do
    printf '    %-14s ' "$p"
    pkg-config --modversion "$p" 2>/dev/null || { echo "缺失!"; exit 1; }
done

echo "==> 全部依赖构建完成，安装前缀: $install"
