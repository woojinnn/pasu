# Multi-stage: build the binary in a Rust toolchain image, ship a slim runtime.
FROM rust:1.82-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p registry-mock

FROM gcr.io/distroless/cc-debian12
COPY --from=build /src/target/release/registry-mock /usr/local/bin/registry-mock
ENV REGISTRY_BIND=0.0.0.0:8080
ENV REGISTRY_STATE=/var/lib/registry
VOLUME ["/var/lib/registry"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/registry-mock"]
