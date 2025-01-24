FROM rust:bookworm
WORKDIR /code
COPY . /code

RUN apt-get update && \
    apt-get install -y build-essential apt-utils cmake clang

RUN /code/scripts/linux-build-libheif.sh /libheif && \
    cmake --install /libheif/build && \
    ldconfig

RUN /code/scripts/linux-build-libuhdr.sh /libuhdr && \
    cmake --install /libuhdr/build && \
    ldconfig

RUN apt-get remove -y libheif1

RUN ./scripts/linux-build-static.sh && \
    ldd target/release/examples/main

