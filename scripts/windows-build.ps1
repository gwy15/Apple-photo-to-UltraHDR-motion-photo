$ErrorActionPreference = "Stop"

New-Item -ItemType Directory -Path "deps/install" -Force | Out-Null

$root = [System.IO.Path]::GetFullPath(".")
$install = [System.IO.Path]::GetFullPath("./deps/install")

Function Build-Jpeg {
    $src = [System.IO.Path]::GetFullPath("./deps/libjpeg")

    if (-Not (Test-Path -Path $src)) {
        git clone https://github.com/libjpeg-turbo/libjpeg-turbo.git $src
    }

    cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$install" -S $src -B $src/build
    cmake --build $src/build --config Release
    cmake --install $src/build
}
Function Build-Heif {
    $src = [System.IO.Path]::GetFullPath("./deps/libheif")

    if (-Not (Test-Path -Path $src)) {
        git clone https://github.com/strukturag/libheif.git $src
    }

    cmake --preset=release-noplugins `
        -DBUILD_SHARED_LIBS=OFF -DCMAKE_INSTALL_PREFIX="$install" `
        -DWITH_UNCOMPRESSED_CODEC=OFF -DWITH_HEADER_COMPRESSION=OFF `
        -DWITH_AOM_DECODER=OFF -DWITH_AOM_ENCODER=OFF `
        -DWITH_EXAMPLES=OFF -S $src -B $src/build
    cmake --build $src/build --config Release
    cmake --install $src/build
}
Function Build-Uhdr {
    $src = [System.IO.Path]::GetFullPath("./deps/libuhdr")

    if (-Not (Test-Path -Path $src)) {
        git clone https://github.com/google/libultrahdr.git $src
    }

    cmake -DCMAKE_BUILD_TYPE=Release -DUHDR_BUILD_EXAMPLES=OFF -DCMAKE_INSTALL_PREFIX="$install" `
        -DBUILD_SHARED_LIBS=OFF -DUHDR_ENABLE_INSTALL=ON `
        -S $src -B $src/build
    cmake --build $src/build --config Release
    # not working
    # cmake --install $src/build
    cp $src/build/Release/uhdr.lib $install/lib/
    cp $src/ultrahdr_api.h $install/include/
}

Function Compile-Rust {
    # $env:PKG_CONFIG_PATH = "$install/lib/pkgconfig"
    # $env:PKG_CONFIG_LIBDIR = "$install/lib"
    # $env:PKG_CONFIG_ALL_STATIC = "true"
    $env:UHDR_LIB_PATH = "$root/deps/libuhdr/build/Release"
    $env:UHDR_HEADER = "$install/include/ultrahdr_api.h"
    $env:PATH = "$env:PATH;$env:UHDR_LIB_PATH"
    # $env:UHDR_STATIC = "true"
    # $env:TURBOJPEG_LIB_DIR = "$install/lib"
    # $env:TURBOJPEG_INCLUDE_DIR = "$install/include"
    # $env:TURBOJPEG_STATIC = "true"
    cargo build --example main --release
    cargo run --example main --release
}

# Build-Jpeg
# Build-Heif
# vcpkg install libheif --triplet x64-windows-static-md
Build-Uhdr
Compile-Rust

