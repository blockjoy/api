# Build container
FROM us-docker.pkg.dev/blockjoy-deployer/blockvisor-api/blockvisor-api-builder:latest as build

ENV RUSTFLAGS -Ctarget-feature=-crt-static

WORKDIR /usr/src/api

## Build the project
COPY . /usr/src/api/
RUN cargo build --release

#RUN strip api/target/release/blockvisor_api
RUN strip /usr/src/api/target/release/blockvisor_api

# Slim output image not containing any build tools / artefacts
FROM alpine:latest

RUN apk add --no-cache libgcc libpq

COPY --from=build /usr/src/api/target/release/blockvisor_api /usr/bin/api

CMD ["api"]
