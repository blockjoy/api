# Build container
FROM rust:alpine as build

# We are indirectly depending on libbrotli.
RUN apk update && apk add protobuf libc-dev protobuf-dev protoc libpq

WORKDIR /usr/src/api

COPY . .

ENV RUSTFLAGS -Ctarget-feature=-crt-static
RUN cargo build --release
RUN strip target/release/api

# Slim output image not containing any build tools / artefacts
FROM alpine:latest

RUN apk add --no-cache libgcc libpq

COPY --from=build /usr/src/api/target/release/api /usr/bin/api
COPY --from=build /usr/src/api/conf /etc/api/conf

CMD ["api"]
