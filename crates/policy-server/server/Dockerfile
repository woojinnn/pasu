FROM rust:1.96-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p policy-server

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
RUN useradd --uid 10001 --create-home appuser
WORKDIR /app
COPY --from=builder /app/target/release/policy-server /usr/local/bin/policy-server
COPY --from=builder /app/target/release/sync_worker   /usr/local/bin/dambi-sync-worker
COPY --from=builder /app/target/release/migrate       /usr/local/bin/policy-server-migrate
# A4 pair: migrations must live in the runtime image; the compile-time path is absent here.
COPY crates/policy-server/db/migrations               /app/migrations
ENV POLICY_DB_MIGRATIONS_DIR=/app/migrations
USER 10001
ENV POLICY_SERVER_ADDR=0.0.0.0:8788
EXPOSE 8788
CMD ["policy-server"]
