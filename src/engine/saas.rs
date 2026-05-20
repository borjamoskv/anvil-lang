use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderMap, Request, StatusCode, header},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use metrics::{counter, histogram};
use metrics_exporter_prometheus::PrometheusHandle;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{Instrument, error, info, info_span};

use crate::core::parser;
use crate::core::typechecker;
use crate::engine::verifier;
use sqlx::sqlite::SqlitePool;

const MAX_SOURCE_BYTES: usize = 50 * 1024;
const MAX_JSON_BODY_BYTES: usize = MAX_SOURCE_BYTES * 6 + 4 * 1024;

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    metrics: PrometheusHandle,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyRequest {
    pub source_code: String,
}

#[derive(Serialize)]
pub struct VerifyResponse {
    pub status: String,
    pub message: String,
    pub certificate_hash: Option<String>,
    pub timestamp: String,
}

pub async fn start_server(port: u16) {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:anvil.db".to_string());
    let pool = SqlitePool::connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Initialize Prometheus metrics exporter on the same server
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    let state = AppState {
        pool,
        metrics: handle,
    };

    let app = Router::new()
        .route("/", get(serve_portal))
        .route("/health", get(health_check))
        .route("/v1/verify", post(verify_contract))
        .route("/v1/auth/validate", post(validate_key))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!(port = port, addr = %addr, "Anvil Proof Market SaaS starting");
    eprintln!("  🚀 Anvil Proof Market SaaS running on http://{}", addr);
    eprintln!("  📊 Prometheus metrics at http://{}/metrics", addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "online",
        "engine": "Anvil+Z3 Formal Verification",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn serve_portal() -> Html<&'static str> {
    Html(include_str!("../../frontend/index.html"))
}

fn auth_key(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-exergy-key")
        .and_then(|h| h.to_str().ok())
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

async fn headers_authorized(pool: &SqlitePool, headers: &HeaderMap) -> Result<bool, sqlx::Error> {
    match auth_key(headers) {
        Some(key) => check_key_validity(pool, key).await,
        None => Ok(false),
    }
}

async fn metrics_handler(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    match headers_authorized(&state.pool, &headers).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::UNAUTHORIZED,
                "metrics require a valid x-exergy-key",
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, "Auth backend unavailable while serving /metrics");
            return (StatusCode::SERVICE_UNAVAILABLE, "auth backend unavailable").into_response();
        }
    }

    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        state.metrics.render(),
    )
        .into_response()
}

async fn verify_contract(
    State(state): State<AppState>,
    request: Request<Body>,
) -> impl IntoResponse {
    counter!("anvil_verify_requests_total").increment(1);

    // --- SOVEREIGN SHIELD (Ω₉ ENFORCEMENT) ---
    match headers_authorized(&state.pool, request.headers()).await {
        Ok(true) => {}
        Ok(false) => {
            error!("Authorization required for /v1/verify");
            return (
                StatusCode::UNAUTHORIZED,
                Json(VerifyResponse {
                    status: "AUTHORIZATION_REQUIRED".to_string(),
                    message: "Every /v1/verify request requires a valid x-exergy-key.".to_string(),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }),
            )
                .into_response();
        }
        Err(e) => {
            error!(error = %e, "Auth backend unavailable for /v1/verify");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(VerifyResponse {
                    status: "AUTH_BACKEND_UNAVAILABLE".to_string(),
                    message: "Authentication backend is unavailable. Verification was not run."
                        .to_string(),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }),
            )
                .into_response();
        }
    }

    if !is_json_content_type(request.headers()) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(VerifyResponse {
                status: "REJECTED".to_string(),
                message: "Unsupported Media Type: expected application/json.".to_string(),
                certificate_hash: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
        )
            .into_response();
    }

    let body = match to_bytes(request.into_body(), MAX_JSON_BODY_BYTES).await {
        Ok(body) => body,
        Err(e) => {
            counter!("anvil_verify_result", "status" => "rejected", "reason" => "payload_too_large")
                .increment(1);
            error!(error = %e, limit_bytes = MAX_JSON_BODY_BYTES, "Verification request rejected: JSON body too large");
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(VerifyResponse {
                    status: "PAYLOAD_TOO_LARGE".to_string(),
                    message: "Payload Too Large: request body exceeds the 50KB source limit plus JSON envelope allowance.".to_string(),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }),
            )
                .into_response();
        }
    };

    let payload: VerifyRequest = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(e) => {
            counter!("anvil_verify_result", "status" => "rejected", "reason" => "invalid_json")
                .increment(1);
            error!(error = %e, "Invalid JSON in verification request");
            return (
                StatusCode::BAD_REQUEST,
                Json(VerifyResponse {
                    status: "REJECTED".to_string(),
                    message: format!("Invalid JSON: {}", e),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }),
            )
                .into_response();
        }
    };

    let source_bytes = payload.source_code.as_bytes().len();
    let span = info_span!("verify_contract", source_len = source_bytes);

    if source_bytes > MAX_SOURCE_BYTES {
        counter!("anvil_verify_result", "status" => "rejected", "reason" => "payload_too_large")
            .increment(1);
        error!(
            source_bytes = source_bytes,
            limit_bytes = MAX_SOURCE_BYTES,
            "Verification request rejected: source payload too large"
        );
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(VerifyResponse {
                status: "PAYLOAD_TOO_LARGE".to_string(),
                message: "Payload Too Large: source_code exceeds the 50KB strict limit."
                    .to_string(),
                certificate_hash: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }),
        )
            .into_response();
    }

    async move {
        let start = std::time::Instant::now();

        // 1. Parse
        let program = match parser::parse_program(&payload.source_code) {
            Ok(p) => p,
            Err(e) => {
                counter!("anvil_verify_result", "status" => "rejected", "reason" => "parse_error").increment(1);
                histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
                error!(error = %e, "Parse error in verification request");
                return (StatusCode::UNPROCESSABLE_ENTITY, Json(VerifyResponse {
                    status: "REJECTED".to_string(),
                    message: format!("Parse Error: {}", e),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })).into_response();
            }
        };

        // 2. Type Check
        let type_env = typechecker::check_program(&program);
        if !type_env.errors.is_empty() {
            counter!("anvil_verify_result", "status" => "rejected", "reason" => "type_error").increment(1);
            histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
            error!(errors = type_env.errors.len(), "Type check failed in verification request");
            return (StatusCode::UNPROCESSABLE_ENTITY, Json(VerifyResponse {
                status: "REJECTED".to_string(),
                message: "Type Check Error: Mismatched types or undefined variables.".to_string(),
                certificate_hash: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            })).into_response();
        }

        // 3. Verify with Z3
        let results = verifier::verify_program(&program, &type_env);
        let all_ok = !results.is_empty() && results.iter().all(|r| r.verified);

        if !all_ok {
            if results.is_empty() {
                counter!("anvil_verify_result", "status" => "rejected", "reason" => "no_obligations").increment(1);
                histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
                info!("Verification rejected: no verification obligations found");
                return (StatusCode::UNPROCESSABLE_ENTITY, Json(VerifyResponse {
                    status: "REJECTED".to_string(),
                    message: "Verification Failed: no verification obligations found. Add invariants before requesting a certificate.".to_string(),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })).into_response();
            }

            if !has_non_exhaustion_failure(&results) {
                if let Some(detail) = z3_exhaustion_detail(&results) {
                counter!("anvil_verify_result", "status" => "rejected", "reason" => "z3_resource_exhausted").increment(1);
                histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
                error!(detail = %detail, "Verification exhausted Z3 resources");
                return (StatusCode::GATEWAY_TIMEOUT, Json(VerifyResponse {
                    status: "Z3_RESOURCE_EXHAUSTED".to_string(),
                    message: format!("Z3 exhausted its solver limit before completing verification. No certificate was issued. {}", detail),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })).into_response();
                }
            }

            counter!("anvil_verify_result", "status" => "rejected", "reason" => "verification_failed").increment(1);
            histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
            info!("Verification rejected: invariants not provable");
            return Json(VerifyResponse {
                status: "REJECTED".to_string(),
                message: "Verification Failed: Invariants could not be proven mathematically.".to_string(),
                certificate_hash: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }).into_response();
        }

        // 4. Issue Cryptographic Certificate
        let mut hasher = Sha256::new();
        hasher.update(b"ANVIL_VERIFIED_V1:");
        hasher.update(payload.source_code.as_bytes());
        let hash_result = hasher.finalize();
        let hex_hash = hex::encode(hash_result);

        counter!("anvil_verify_result", "status" => "verified").increment(1);
        let duration = start.elapsed().as_secs_f64();
        histogram!("anvil_verify_duration_seconds").record(duration);
        info!(
            certificate = %format!("0x{}", &hex_hash[..16]),
            duration_ms = duration * 1000.0,
            "Verification successful — certificate issued"
        );

        Json(VerifyResponse {
            status: "VERIFIED".to_string(),
            message: "Code mathematically proven. Cryptographic certificate issued.".to_string(),
            certificate_hash: Some(format!("0x{}", hex_hash)),
            timestamp: chrono::Utc::now().to_rfc3339(),
        }).into_response()
    }.instrument(span).await
}

fn z3_exhaustion_detail(results: &[verifier::VerifyResult]) -> Option<String> {
    results.iter().find_map(|result| {
        result
            .counterexample
            .as_deref()
            .filter(|detail| is_z3_exhaustion_detail(detail))
            .map(|detail| format!("{}: {}", result.fn_name, detail))
    })
}

fn has_non_exhaustion_failure(results: &[verifier::VerifyResult]) -> bool {
    results.iter().any(|result| {
        !result.verified
            && !result
                .counterexample
                .as_deref()
                .is_some_and(is_z3_exhaustion_detail)
    })
}

fn is_z3_exhaustion_detail(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("z3 undecidable")
        || lower.contains("z3 unknown")
        || lower.contains("solver unknown")
        || lower.contains("is undecidable")
        || lower.contains("out of memory")
        || lower.contains("cannot allocate memory")
        || lower.contains("memory allocation")
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(|media_type| {
            let media_type = media_type.trim();
            media_type.eq_ignore_ascii_case("application/json")
                || media_type.to_ascii_lowercase().ends_with("+json")
        })
        .unwrap_or(false)
}

#[derive(Deserialize)]
pub struct AuthRequest {
    pub key: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub valid: bool,
    pub owner: Option<String>,
    pub tier: Option<String>,
}

async fn validate_key(
    State(state): State<AppState>,
    Json(payload): Json<AuthRequest>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT owner_id, tier FROM exergy_keys WHERE key_id = ? AND status = 'ACTIVE'",
    )
    .bind(&payload.key)
    .fetch_optional(&state.pool)
    .await;

    match result {
        Ok(Some((owner_id, tier))) => Json(AuthResponse {
            valid: true,
            owner: Some(owner_id),
            tier: Some(tier.unwrap_or_else(|| "SOVEREIGN".to_string())),
        })
        .into_response(),
        Ok(None) => Json(AuthResponse {
            valid: false,
            owner: None,
            tier: None,
        })
        .into_response(),
        Err(e) => {
            error!(error = %e, "Auth backend unavailable while validating key");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(AuthResponse {
                    valid: false,
                    owner: None,
                    tier: None,
                }),
            )
                .into_response()
        }
    }
}

pub async fn check_key_validity(pool: &SqlitePool, key: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM exergy_keys WHERE key_id = ? AND status = 'ACTIVE'",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map(|r| r.is_some())
}
