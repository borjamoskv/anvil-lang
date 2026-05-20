# Anvil Proof Market — SaaS Guide

The Anvil Proof Market is an HTTP API that accepts Anvil source code, verifies it with Z3, and issues cryptographic certificates for proven code.

## Quick Start

```bash
# Start the server
sqlite3 anvil.db 'CREATE TABLE IF NOT EXISTS exergy_keys (key_id TEXT PRIMARY KEY, owner_id TEXT NOT NULL, tier TEXT DEFAULT "SOVEREIGN", status TEXT DEFAULT "ACTIVE")'
anvil keys add --key "$ANVIL_EXERGY_KEY" --owner local-dev
anvil saas --port 3000

# In another terminal, submit code for verification
curl -X POST http://localhost:3000/v1/verify \
  -H "Content-Type: application/json" \
  -H "x-exergy-key: $ANVIL_EXERGY_KEY" \
  -d '{
    "source_code": "fn add(mut a: u64, b: u64) -> u64 where { a > 0, b > 0, a'\'' == a + b } { a += b; return a; }"
  }'
```

## Local Proof Market Page

The standalone Proof Market oracle serves the local page at `http://127.0.0.1:8000/` and posts to the real `/v1/prove` endpoint.

```bash
scripts/proof-market-local.sh
```

The script builds the local Anvil CLI, points `ANVIL_BIN` at Cargo's target directory, sets `ANVIL_CERTIFICATE_SECRET=local-dev-secret`, and enables mock payment explicitly with `ANVIL_ALLOW_MOCK_PAYMENT=1`.

Manual equivalent:

```bash
# Build the Anvil CLI used by the oracle subprocess
cargo build --bin anvil
ANVIL_BIN="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"] + "/debug/anvil")')"

# Local demo mode: real verifier, mock payment enabled explicitly
cd services/proof-market
ANVIL_BIN="$ANVIL_BIN" \
ANVIL_CERTIFICATE_SECRET=local-dev-secret \
ANVIL_ALLOW_MOCK_PAYMENT=1 \
cargo run
```

For Stripe mode, omit `ANVIL_ALLOW_MOCK_PAYMENT` and provide a paid Checkout session from your Stripe test/live account:

```bash
cd services/proof-market
ANVIL_BIN="$ANVIL_BIN" \
ANVIL_CERTIFICATE_SECRET="$ANVIL_CERTIFICATE_SECRET" \
STRIPE_API_KEY="$STRIPE_API_KEY" \
cargo run
```

`/v1/prove` accepts:

```json
{
  "client_id": "local-demo",
  "payment_mode": "mock",
  "source_code": "fn add(mut a: u64, b: u64) -> u64 where { a > 0, b > 0, a' == a + b } { a += b; return a; }"
}
```

For Stripe mode, omit `payment_mode` or set it to `"stripe"` and send `"stripe_session_id": "cs_test_..."` or `"cs_live_..."`. The Checkout Session must have `payment_status=paid` and either `client_reference_id` or `metadata.client_id` matching the request `client_id`. `ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS` and `ANVIL_STRIPE_EXPECTED_CURRENCY` are required in Stripe mode so price/currency checks fail closed.

## API Reference

### `GET /health`

Health check endpoint.

**Response:**
```json
{
  "status": "online",
  "engine": "Anvil+Z3 Formal Verification",
  "version": "0.6.0"
}
```

### `POST /v1/verify`

Submit Anvil source code for formal verification.

Requires a valid `x-exergy-key` header.
Requests must use `Content-Type: application/json`.

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
- Missing or invalid auth: HTTP `401`, status `"AUTHORIZATION_REQUIRED"`
- Parse errors: `"Parse Error: ..."`
- Type errors: `"Type Check Error: ..."`
- Payload too large: HTTP `413`, `"source_code exceeds the 50KB strict limit"`
- Z3 exhausted: HTTP `504`, status `"Z3_RESOURCE_EXHAUSTED"`, no certificate issued

### `GET /`

Serves the Proof Market web portal (HTML frontend).

### `GET /metrics`

Prometheus metrics endpoint for monitoring. Requires a valid `x-exergy-key` header.

## `anvil check --json` Schema v1

The SaaS services and any custom API workers should call the CLI with `--json` when they need a stable machine-readable contract:

```bash
anvil check --json --timeout 5000 path/to/file.anv
```

The command writes one JSON object to stdout and keeps stderr quiet by default. The stable discriminator is:

```json
{
  "schema_version": "anvil.check.v1",
  "kind": "check",
  "status": "VERIFIED",
  "ok": true
}
```

### Statuses and Exit Codes

| Status | `ok` | Exit code | Meaning |
|---|---:|---:|---|
| `VERIFIED` | `true` | `0` | Every function result verified |
| `REJECTED` | `false` | `1` | Z3 found a failed proof or counterexample |
| `PARSE_ERROR` | `false` | `1` | Parser rejected the source |
| `TYPE_ERROR` | `false` | `1` | Type checker rejected the source |
| `IO_ERROR` | `false` | `1` | Input file could not be read |
| `Z3_RESOURCE_EXHAUSTED` | `false` | `1` | Solver returned unknown or exhausted resources |

### Schema Shape

Fields in schema v1 are designed to be safe for CI, API sidecars, and browser frontends:

```json
{
  "schema_version": "anvil.check.v1",
  "anvil_version": "0.6.0",
  "kind": "check",
  "status": "VERIFIED",
  "ok": true,
  "message": "All postconditions proven.",
  "error": null,
  "file": "examples/transfer.anv",
  "timeout_ms": 5000,
  "functions": 1,
  "invariants": 4,
  "all_verified": true,
  "proof_hash": "64_hex_chars_or_null",
  "duration_ms": 12.34,
  "durations": {
    "parse_ms": 1.0,
    "typecheck_ms": 2.0,
    "verification_ms": 9.0,
    "total_ms": 12.0
  },
  "summary": {
    "functions_total": 1,
    "functions_verified": 1,
    "functions_failed": 0,
    "invariants_total": 4,
    "preconditions_total": 2,
    "postconditions_total": 2,
    "type_constraints_total": 2,
    "errors_total": 0,
    "warnings_total": 0,
    "counterexamples_total": 0
  },
  "proof": {
    "hash_algorithm": "sha3-256",
    "aggregate_hash": "64_hex_chars_or_null",
    "function_hashes": [
      {
        "fn_name": "transfer",
        "status": "VERIFIED",
        "proof_hash": "64_hex_chars"
      }
    ]
  },
  "errors": [],
  "warnings": [],
  "counterexamples": [],
  "results": [
    {
      "fn_name": "transfer",
      "status": "VERIFIED",
      "verified": true,
      "invariants": 4,
      "invariants_checked": 4,
      "preconditions": 2,
      "preconditions_count": 2,
      "postconditions": 2,
      "postconditions_count": 2,
      "proof_hash": "64_hex_chars",
      "duration_ms": 9.0,
      "counterexample": null,
      "counterexamples": [],
      "warnings": []
    }
  ]
}
```

### Recommended Consumer Fields

| Consumer | Use these fields |
|---|---|
| CI | `ok`, `status`, process exit code, `summary.errors_total`, `summary.counterexamples_total`, `duration_ms` |
| API sidecar | `schema_version`, `anvil_version`, `timeout_ms`, `proof_hash`, `proof.hash_algorithm`, `errors`, `warnings` |
| Frontend | `message`, `results[].fn_name`, `results[].status`, `results[].duration_ms`, `counterexamples[].lines`, `warnings` |

Treat unknown future fields as additive. Pin behavior on `schema_version`, `status`, and `ok`, not on the human-readable `message`.

## Prometheus Metrics

The SaaS server exports the following metrics at `/metrics`:

| Metric | Type | Description |
|---|---|---|
| `anvil_verify_requests_total` | Counter | Total verification requests received |
| `anvil_verify_duration_seconds` | Histogram | Duration of verification requests |
| `anvil_verify_result` | Counter | Results by status (`verified`, `rejected`) |

### Prometheus Configuration

Add to your `prometheus.yml`. Because `/metrics` requires `x-exergy-key`, scrape it through a small internal reverse proxy that injects the header, or use your Prometheus version's custom HTTP header support.

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
FROM rust:1.85-bookworm AS builder

RUN apt-get update && apt-get install -y libz3-dev pkg-config

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libz3-dev ca-certificates curl && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/anvil /usr/local/bin/anvil

EXPOSE 4242
CMD ["anvil", "saas", "--port", "4242"]
```

### Build & Run

```bash
docker build -t anvil-proof-market .
docker run -p 4242:4242 anvil-proof-market
```

### Docker Compose

```yaml
services:
  anvil:
    build: .
    ports:
      - "4242:4242"
    environment:
      - RUST_LOG=info
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:4242/health"]
```

## Production Configuration

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `RUST_LOG` | `info` | Log level (`trace`, `debug`, `info`, `warn`, `error`) |
| `DATABASE_URL` | `sqlite:anvil.db` | SQLite database used to validate active `x-exergy-key` values |
| `--port` | `3000` | SaaS server port flag |
| `ANVIL_BIN` | auto-detected | Explicit `anvil` binary path used by `/v1/prove` sidecars |
| `CARGO_TARGET_DIR` | Cargo default | Fallback target directory for sidecar binary discovery |
| `PROOF_MARKET_ADDR` | `127.0.0.1:8000` | Bind address for the standalone Rust Proof Market sidecar |
| `ANVIL_PROCESS_TIMEOUT_SECS` | `10` | Proof Market subprocess timeout for `anvil check` |
| `ANVIL_PROCESS_MEMORY_MB` | `512` | Proof Market subprocess memory cap on Linux/Android (`0` disables; macOS runs without this cap) |
| `ANVIL_MAX_CONCURRENT_PROOFS` | `2` | Maximum concurrent `/v1/prove` subprocesses |
| `ANVIL_QUEUE_TIMEOUT_SECS` | `5` | Maximum time a `/v1/prove` request waits for proof execution capacity before returning `PROOF_QUEUE_FULL` |
| `ANVIL_STRIPE_TIMEOUT_SECS` | `10` | Stripe API timeout for `/v1/prove` sidecars |
| `STRIPE_API_KEY` | none | Required by `/v1/prove` proof-market sidecars to verify Checkout sessions |
| `ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS` | none | Required exact Checkout Session amount guard in Stripe mode |
| `ANVIL_STRIPE_EXPECTED_CURRENCY` | none | Required exact Checkout Session currency guard in Stripe mode |
| `ANVIL_CERTIFICATE_SECRET` | none | Required by `/v1/prove` proof-market sidecars before issuing certificate hashes |
| `ANVIL_ALLOW_MOCK_PAYMENT` | unset | Enables local-only mock payment mode for the standalone `services/proof-market` page |
| `ANVIL_ALLOW_LEGACY_ANVIL_OUTPUT` | unset | Allows old non-JSON `anvil check` output; leave unset in production so certificates require structured JSON |

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
        # Restrict metrics to internal network and inject a dedicated active key.
        allow 10.0.0.0/8;
        deny all;
        proxy_set_header x-exergy-key "REPLACE_WITH_INTERNAL_METRICS_KEY";
        proxy_pass http://127.0.0.1:3000/metrics;
    }
}
```
