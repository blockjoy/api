# Build container
FROM rust:alpine as build

# We are indirectly depending on libbrotli.
RUN apk update && apk add protobuf libc-dev protobuf-dev protoc libpq-dev


ENV RUSTFLAGS -Ctarget-feature=-crt-static

WORKDIR /usr/src/api
# Cache dependencies
#RUN cd /usr/src && cargo new --lib /usr/src/api
RUN cargo init
COPY rust-toolchain.toml /usr/src/api/
COPY Cargo.lock /usr/src/api/
COPY Cargo.toml /usr/src/api/
COPY build.rs /usr/src/api/src
COPY proto /usr/src/api/proto
COPY cookbook_protos /usr/src/api/cookbook_protos
COPY build.rs /usr/src/api/
RUN cargo build --release