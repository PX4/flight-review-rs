FROM rust:1.94-bookworm AS builder

WORKDIR /build

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary with PostgreSQL support
RUN cargo build --release -p flight-review-server --features postgres \
    && cargo build --release -p flight-review --bin ulog-convert

# Runtime image — minimal, no Rust toolchain
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/flight-review-server /usr/local/bin/
COPY --from=builder /build/target/release/ulog-convert /usr/local/bin/

# Default data directory
RUN mkdir -p /data/files

EXPOSE 8080

ENTRYPOINT ["flight-review-server"]
CMD ["serve", "--db", "sqlite:///data/flight-review.db", "--storage", "file:///data/files"]
