# BeaconSync hub image. Build from the repo root:
#   docker build -f docker/hub.Dockerfile -t beacon-hub .
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p beacon-hub

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/beacon-hub /usr/local/bin/beacon-hub
ENV BEACON_HUB_DATA=/data \
    BEACON_HUB_ADDR=0.0.0.0:8787
VOLUME /data
EXPOSE 8787
CMD ["beacon-hub"]
