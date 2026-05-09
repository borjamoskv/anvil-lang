# Anvil Proof Market — SaaS Guide

The Anvil Proof Market is an HTTP API that accepts Anvil source code, verifies it with Z3, and issues cryptographic certificates for proven code.

## Quick Start

```bash
# Start the server
anvil saas --port 3000

# In another terminal, submit code for verification
curl -X POST http://localhost:3000/v1/verify \
  -H "Content-Type: application/json" \
  -d '{
    "source_code": "fn add(mut a: u64, b: u64) -> u64 where { a > 0, b > 0, a'\'' == a + b } { a += b; return a; }"
  }'
```

## API Reference

### `GET /health`

Health check endpoint.

**Response:**
```json
{
  "status": "online",
  "engine": "Anvil+Z3 Formal Verification",
  "version": "0.5.1"
}
```

### `POST /v1/verify`

Submit Anvil source code for formal verification.

**Request:**
```json
{
  "source_code": "fn add(mut a: u64, b: u64) -> u64 where { a > 0, b > 0, a' == a + b } { a += b; return a; }"
}
```

**Success Response (VERIFIED):**
```json
{
  "status": "VERIFIED",
  "message": "Code mathematically proven. Cryptographic certificate issued.",
  "certificate_hash": "0xabcdef0123456789...",
  "timestamp": "2026-05-09T10:00:00Z"
}
```

**Failure Response (REJECTED):**
```json
{
  "status": "REJECTED",
  "message": "Verification Failed: Invariants could not be proven mathematically.",
  "certificate_hash": null,
  "timestamp": "2026-05-09T10:00:00Z"
}
```

**Error Responses:**
- Parse errors: `"Parse Error: ..."`
- Type errors: `"Type Check Error: ..."`

### `GET /`

Serves the Proof Market web portal (HTML frontend).

### `GET /metrics`

Prometheus metrics endpoint for monitoring.

## Prometheus Metrics

The SaaS server exports the following metrics at `/metrics`:

| Metric | Type | Description |
|---|---|---|
| `anvil_verify_requests_total` | Counter | Total verification requests received |
| `anvil_verify_duration_seconds` | Histogram | Duration of verification requests |
| `anvil_verify_result` | Counter | Results by status (`verified`, `rejected`) |

### Prometheus Configuration

Add to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'anvil-proof-market'
    static_configs:
      - targets: ['localhost:3000']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

### Grafana Dashboard

Example PromQL queries:

```promql
# Request rate (per second)
rate(anvil_verify_requests_total[5m])

# Verification success rate
rate(anvil_verify_result{status="verified"}[5m]) /
rate(anvil_verify_requests_total[5m])

# P99 verification latency
histogram_quantile(0.99, rate(anvil_verify_duration_seconds_bucket[5m]))
```

## Structured Logging

The SaaS server uses `tracing` for structured JSON-compatible logging:

```bash
# Default (info level)
anvil saas --port 3000

# Debug level (includes per-request traces)
RUST_LOG=debug anvil saas --port 3000

# JSON output (for log aggregators)
RUST_LOG=info anvil saas --port 3000
```

## Docker Deployment

### Dockerfile

```dockerfile
FROM rust:1.80-slim AS builder

RUN apt-get update && apt-get install -y z3 libz3-dev clang llvm pkg-config

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libz3-4 && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/anvil /usr/local/bin/anvil

EXPOSE 3000
CMD ["anvil", "saas", "--port", "3000"]
```

### Build & Run

```bash
docker build -t anvil-proof-market .
docker run -p 3000:3000 anvil-proof-market
```

### Docker Compose

```yaml
version: '3.8'
services:
  anvil:
    build: .
    ports:
      - "3000:3000"
    environment:
      - RUST_LOG=info

  prometheus:
    image: prom/prometheus
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml

  grafana:
    image: grafana/grafana
    ports:
      - "3001:3000"
    depends_on:
      - prometheus
```

## Production Configuration

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info` | Log level (`trace`, `debug`, `info`, `warn`, `error`) |
| `PORT` | `3000` | Server port (use `--port` flag) |

### Reverse Proxy (Nginx)

```nginx
server {
    listen 443 ssl;
    server_name proofmarket.yourdomain.com;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
    }

    location /metrics {
        # Restrict metrics to internal network
        allow 10.0.0.0/8;
        deny all;
        proxy_pass http://127.0.0.1:3000/metrics;
    }
}
```
