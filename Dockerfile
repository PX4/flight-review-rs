FROM rust:1.94-bookworm AS builder

WORKDIR /build

# Copy workspace
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Build release binary with PostgreSQL support by default
ARG SERVER_FEATURES=postgres
RUN cargo build --release -p flight-review-server --features "$SERVER_FEATURES" \
    && cargo build --release -p flight-review --bin ulog-convert

FROM node:22-bookworm-slim AS frontend

WORKDIR /build/frontend

COPY frontend/package*.json ./
RUN npm ci --include=optional --no-audit --no-fund

COPY frontend/ ./
ARG PUBLIC_MAPBOX_TOKEN=""
RUN npm run build

# Runtime image — minimal, no Rust toolchain
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/flight-review-server /usr/local/bin/
COPY --from=builder /build/target/release/ulog-convert /usr/local/bin/
COPY --from=frontend /build/frontend/build /usr/share/flight-review/frontend

# Default data directory
RUN mkdir -p /data/files

EXPOSE 8080

ENTRYPOINT ["flight-review-server"]
CMD ["serve", "--db", "sqlite:///data/flight-review.db", "--storage", "file:///data/files", "--frontend-dir", "/usr/share/flight-review/frontend"]
