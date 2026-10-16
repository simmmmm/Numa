FROM rust:1.98-trixie AS build
ARG JOBS=4
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p numa-server -j "${JOBS}" \
 && cp target/release/numa-server /usr/local/bin/numa-server

FROM debian:trixie-slim
LABEL org.opencontainers.image.source=https://github.com/simmmmm/numa-home
LABEL org.opencontainers.image.licenses=GPL-3.0-or-later
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl tar tzdata libvulkan1 mesa-vulkan-drivers \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/bin/numa-server /usr/local/bin/numa-server
COPY LICENSE data/THIRD_PARTY_LICENSES-server.txt data/MODELS-LICENSES.txt /usr/share/doc/numa-home/
ENV NUMA_HOME_PHOTOS=/photos NUMA_HOME_SENT=/sent NUMA_HOME_DATA=/data NUMA_HOME_CARDS=/cards NUMA_HOME_PORT=8283
VOLUME ["/photos", "/sent", "/data"]
EXPOSE 8283
HEALTHCHECK --interval=1m --timeout=5s CMD curl -fsS http://localhost:8283/api/v1/status || exit 1
CMD ["numa-server"]
