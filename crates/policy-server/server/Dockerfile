FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p policy-server

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/policy-server /usr/local/bin/policy-server
COPY --from=builder /app/target/release/sync_worker /usr/local/bin/scopeball-sync-worker
COPY --from=builder /app/target/release/migrate /usr/local/bin/policy-server-migrate
COPY scopeball-sync.toml /app/scopeball-sync.toml
ENV POLICY_SERVER_ADDR=0.0.0.0:8788
EXPOSE 8788
CMD ["policy-server"]
