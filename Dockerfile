### Build container
FROM us-docker.pkg.dev/blockjoy-deployer/blockvisor-api/blockvisor-api-builder:latest as build

ENV RUSTFLAGS -Ctarget-feature=-crt-static

WORKDIR /src

COPY . /src

RUN cargo build --release
RUN strip /src/target/release/blockvisor_api


### App container
FROM alpine:latest

RUN apk add --no-cache libgcc libpq

COPY --from=build /src/target/release/blockvisor_api /usr/bin/api

CMD ["api"]
