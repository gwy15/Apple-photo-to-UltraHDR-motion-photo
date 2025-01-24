FROM rust:bookworm
WORKDIR /code
COPY . /code

RUN apt-get update && \
    apt-get install -y build-essential apt-utils cmake clang

RUN mkdir /libheif && \
    git clone https://github.com/strukturag/libheif.git /libheif && \
    mkdir /libheif/build && cd /libheif/build && \
    cmake --preset=release-noplugins \
        -DBUILD_SHARED_LIBS=OFF \
        -DWITH_UNCOMPRESSED_CODEC=OFF -DWITH_HEADER_COMPRESSION=OFF \
        -DWITH_AOM_DECODER=OFF -DWITH_AOM_ENCODER=OFF \
        -DWITH_EXAMPLES=OFF \
        .. && \
    cmake --build . && \
    cmake --install . && \
    ldconfig

RUN mkdir /libuhdr && \
    git clone https://github.com/google/libultrahdr.git /libuhdr && \
    mkdir /libuhdr/build && cd /libuhdr/build && \
    cmake -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF .. && \
    cmake --build . && \
    cmake --install . && \
    ldconfig

RUN apt-get remove -y libheif1

RUN env PKG_CONFIG_ALL_STATIC=true ULTRAHDR_STATIC=true cargo build --example main --release && \
    ldd target/release/examples/main

