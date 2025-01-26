FROM rust:bookworm AS builder
WORKDIR /code
COPY . /code

RUN apt-get update && \
    apt-get install -y build-essential apt-utils cmake clang && \
    mkdir deps && mkdir pkg-config

RUN ./scripts/linux-build-libjpeg.sh deps pkg-config && \
    cmake --install deps/libjpeg/build
RUN ./scripts/linux-build-libde265.sh deps pkg-config && \
    cmake --install deps/libde265/build
RUN ./scripts/linux-build-libheif.sh deps pkg-config && \
    cmake --install deps/libheif/build
RUN ./scripts/linux-build-libuhdr.sh deps pkg-config && \
    cmake --install deps/libuhdr/build

RUN env PKG_CONFIG_PATH=$(realpath pkg-config/lib/pkgconfig) PKG_CONFIG_LIBDIR=$(realpath pkg-config/lib) \
        PKG_CONFIG_ALL_STATIC=true \
        TURBOJPEG_STATIC=1 TURBOJPEG_LIB_DIR=$(realpath pkg-config/lib) TURBOJPEG_INCLUDE_PATH=$(realpath pkg-config/include) \
    cargo build --example main --release

# runtime
FROM debian:bookworm
WORKDIR /code

RUN apt-get update && apt-get install exiftool -y
COPY --from=builder /code/target/release/examples/main ./main
ENTRYPOINT ["/code/main"]
