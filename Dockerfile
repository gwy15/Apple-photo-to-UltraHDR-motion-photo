FROM rust:bookworm AS builder
WORKDIR /code
COPY . /code

RUN apt-get update && \
    apt-get install -y build-essential apt-utils cmake clang && \
    mkdir /pkg-config

RUN ./scripts/linux-build-libjpeg.sh /libjpeg /pkg-config && \
    cmake --install /libjpeg/build
RUN ./scripts/linux-build-libheif.sh /libheif /pkg-config && \
    cmake --install /libheif/build
RUN ./scripts/linux-build-libuhdr.sh /libuhdr /pkg-config && \
    cmake --install /libuhdr/build

RUN env PKG_CONFIG_PATH=/pkg-config/lib/pkgconfig PKG_CONFIG_LIBDIR=/pkg-config/lib \
        PKG_CONFIG_ALL_STATIC=true \
        TURBOJPEG_STATIC=1 TURBOJPEG_LIB_DIR=/pkg-config/lib TURBOJPEG_INCLUDE_PATH=/pkg-config/include \
    cargo build --example main --release

# runtime
FROM debian:bookworm
WORKDIR /code
COPY --from=builder /code/target/release/examples/main ./main
