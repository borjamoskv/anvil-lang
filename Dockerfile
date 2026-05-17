# ── Stage 1: Builder ──────────────────────────────────────────
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install Z3 dev libraries (required by z3 crate)
RUN apt-get update && apt-get install -y \
    libz3-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies first
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -f src/main.rs

# Build the actual binary
COPY src ./src
COPY frontend ./frontend
RUN cargo build --release

# ── Stage 2: Runtime ──────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    libz3-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/anvil /usr/local/bin/anvil
COPY --from=builder /app/frontend ./frontend

EXPOSE 4242

ENV RUST_LOG=info

CMD ["anvil", "saas", "--port", "4242"]
