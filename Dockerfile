# syntax=docker/dockerfile:1.3-labs
# Build container
FROM rust:alpine as build

# We are indirectly depending on libbrotli.
RUN apk update && apk add protobuf libc-dev protobuf-dev protoc libpq-dev

WORKDIR /usr/src/api
COPY . .

ENV RUSTFLAGS -Ctarget-feature=-crt-static
RUN cargo build --release
RUN strip target/release/blockvisor_api

# WORKDIR /usr/src
# Cache dependencies
#RUN cargo new --lib /usr/src/api
#COPY Cargo.lock /usr/src/api/
#COPY Cargo.toml /usr/src/api/
#COPY proto /usr/src/api/proto
#COPY cookbook_protos /usr/src/api/cookbook_protos
#COPY build.rs /usr/src/api/
#RUN --mount=type=cache,target=/usr/local/cargo/registry cd api && cargo build --release
#
## Build the project
#COPY . /usr/src/api/
#RUN --mount=type=cache,target=/usr/local/cargo/registry <<EOF
#set -e
#touch /usr/src/api/src/lib.rs
#cd /usr/src/api
#cargo build --release
#EOF
#
#RUN strip api/target/release/blockvisor_api

# Slim output image not containing any build tools / artefacts
FROM alpine:latest

RUN apk add --no-cache libgcc libpq

COPY --from=build /usr/src/api/target/release/blockvisor_api /usr/bin/api
COPY --from=build /usr/src/api/conf /etc/api/conf

CMD ["api"]
