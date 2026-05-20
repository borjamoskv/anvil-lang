# ── Stage 1: Builder ──────────────────────────────────────────
FROM rust:1.85-bookworm AS builder

WORKDIR /app

ARG CARGO_BUILD_JOBS=1
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS} \
    CARGO_INCREMENTAL=0

# Install Z3 dev libraries (required by z3 crate)
RUN apt-get update && apt-get install -y \
    libz3-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Cache dependency downloads first
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch --locked

# Build the actual binary
COPY src ./src
COPY frontend ./frontend
RUN cargo build --release --locked --bin anvil -j "$CARGO_BUILD_JOBS"

# ── Stage 2: Runtime ──────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libz3-dev \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/anvil /usr/local/bin/anvil
COPY --from=builder /app/frontend ./frontend

EXPOSE 4242

ENV RUST_LOG=info

CMD ["anvil", "saas", "--port", "4242"]
