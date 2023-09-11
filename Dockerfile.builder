### Build container
FROM rust:alpine as build

# We are indirectly depending on libbrotli.
RUN apk update && apk add protobuf libc-dev protobuf-dev protoc libpq-dev

ENV RUSTFLAGS -Ctarget-feature=-crt-static

WORKDIR /src

COPY proto /src

RUN cargo init

COPY build.rs /src
COPY Cargo.lock /src
COPY Cargo.toml /src

COPY rust-toolchain.toml /src

RUN cargo build --release
