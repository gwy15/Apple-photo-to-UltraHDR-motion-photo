$ErrorActionPreference = "Stop"

New-Item -ItemType Directory -Path "deps/install" -Force | Out-Null

$root = [System.IO.Path]::GetFullPath(".")
$install = [System.IO.Path]::GetFullPath("./deps/install")

Function Build-Jpeg {
    $src = [System.IO.Path]::GetFullPath("./deps/libjpeg")

    if (-Not (Test-Path -Path $src)) {
        git clone --depth 1 --branch 3.1.0 https://github.com/libjpeg-turbo/libjpeg-turbo.git $src
    }

    cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$install" `
        -DWITH_JPEG8=1 -DENABLE_SHARED=OFF -DENABLE_STATIC=ON `
        -S $src -B $src/build
    cmake --build $src/build --config Release
    cmake --install $src/build
    # workaround for turbojpeg
    cp $install/lib/jpeg-static.lib $install/lib/jpeg.lib
    cp $install/lib/turbojpeg-static.lib $install/lib/turbojpeg.lib
}
Function Build-Heif {
    # As per https://github.com/Cykooz/libheif-sys/blob/master/build.rs, libheif is only supported through vcpkg
    vcpkg install libheif --triplet x64-windows-static-md
}
# depends on libjpeg
Function Build-Uhdr {
    $src = [System.IO.Path]::GetFullPath("./deps/libuhdr")

    if (-Not (Test-Path -Path $src)) {
        git clone --depth 1 --branch v1.4.0 https://github.com/google/libultrahdr.git $src
    }

    cmake -DCMAKE_BUILD_TYPE=Release -DUHDR_BUILD_EXAMPLES=OFF -DCMAKE_INSTALL_PREFIX="$install" `
        -DBUILD_SHARED_LIBS=OFF -DUHDR_ENABLE_INSTALL=ON `
        -S $src -B $src/build
    cmake --build $src/build --config Release
    # not working
    # cmake --install $src/build
    cp "$src/build/Release/uhdr.lib" "$install/lib/"
    cp "$src/ultrahdr_api.h" "$install/include/"
}

Function Build-FFmpeg {
    vcpkg install ffmpeg --triplet x64-windows-static-md
}

Function Compile-Rust {
    $env:PKG_CONFIG_ALL_STATIC = "true"

    # for uhd-sys, set precompiled lib
    $env:UHDR_LIB_PATH = "$install/lib"
    $env:UHDR_HEADER = "$install/include/ultrahdr_api.h"

    # for turbojpeg-sys, set precompiled lib
    $env:TURBOJPEG_STATIC = "1"
    $env:TURBOJPEG_LIB_DIR = "$install/lib"
    $env:TURBOJPEG_INCLUDE_DIR = "$install/include"

    $env:PATH = "$env:PATH;$env:UHDR_LIB_PATH;$root/deps/install/bin"

    cargo build --example aa-photo-bridge --release
    # cargo run --example aa-photo-bridge --release
}

#Build-Heif
#Build-Jpeg
#Build-Uhdr
#Compile-Rust

