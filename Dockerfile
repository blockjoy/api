# Build container
FROM rust:alpine as build

# We are indirectly depending on libbrotli.
RUN apk update && apk add libc-dev protobuf-dev

WORKDIR /usr/src/api

COPY . .

# ENV PATH="${PATH}:/usr/include:/usr/include/google:/usr/include/google/protobuf"
ENV RUSTFLAGS -Ctarget-feature=-crt-static
# RUN alias protoc=/usr/bin/protoc
RUN cargo build --release
RUN strip target/release/api

# Slim output image not containing any build tools / artefacts
FROM alpine:latest

RUN apk add libgcc

COPY --from=build /usr/src/api/target/release/api /usr/bin/api
COPY --from=build /usr/src/api/conf /etc/api/conf

CMD ["api"]
