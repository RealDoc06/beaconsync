# The published hub image: the release's static musl builds, no compiling.
# Built by .github/workflows/release.yml with hub-bin/beacon-hub-{amd64,arm64} in the context.
# To build from source instead, use hub.Dockerfile.
FROM gcr.io/distroless/static-debian12
ARG TARGETARCH
COPY hub-bin/beacon-hub-${TARGETARCH} /usr/local/bin/beacon-hub
ENV BEACON_HUB_DATA=/data \
    BEACON_HUB_ADDR=0.0.0.0:8787
VOLUME /data
EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/beacon-hub"]
