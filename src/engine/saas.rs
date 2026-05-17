use axum::{
    routing::{get, post},
    Router, Json, response::{IntoResponse, Html},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, error, info_span, Instrument};
use metrics::{counter, histogram};

use crate::core::parser;
use crate::core::typechecker;
use crate::engine::verifier;
use sqlx::sqlite::SqlitePool;

#[derive(Deserialize)]
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
    let pool = SqlitePool::connect(&db_url).await.expect("Failed to connect to database");

    // Initialize Prometheus metrics exporter on the same server
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    let app = Router::new()
        .route("/", get(serve_portal))
        .route("/health", get(health_check))
        .route("/v1/verify", post(verify_contract))
        .route("/v1/auth/validate", post(validate_key))
        .route("/metrics", get(move || {
            let handle = handle.clone();
            async move { handle.render() }
        }))
        .with_state(pool);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!(port = port, addr = %addr, "Anvil Proof Market SaaS starting");
    eprintln!("  {} Anvil Proof Market SaaS running on http://{}", "🚀", addr);
    eprintln!("  {} Prometheus metrics at http://{}/metrics", "📊", addr);

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

async fn verify_contract(
    axum::extract::State(pool): axum::extract::State<SqlitePool>,
    headers: HeaderMap,
    Json(payload): Json<VerifyRequest>
) -> impl IntoResponse {
    let span = info_span!("verify_contract", source_len = payload.source_code.len());

    // --- SOVEREIGN SHIELD (Ω₉ ENFORCEMENT) ---
    let auth_key = headers.get("x-exergy-key").and_then(|h| h.to_str().ok());
    
    // Validate against DB
    let is_authorized = if let Some(key) = auth_key {
        check_key_validity(&pool, key).await
    } else {
        false
    };

    if payload.source_code.len() > 500 && !is_authorized {
        error!("Authorization required for high-exergy verification ({} chars)", payload.source_code.len());
        return (StatusCode::PAYMENT_REQUIRED, Json(VerifyResponse {
            status: "AUTHORIZATION_REQUIRED".to_string(),
            message: "High-Exergy verification (>500 chars) requires a valid x-exergy-key. Get one at agents.archi".to_string(),
            certificate_hash: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
        })).into_response();
    }

    async move {
        let start = std::time::Instant::now();
        counter!("anvil_verify_requests_total").increment(1);

        // 1. Parse
        let program = match parser::parse_program(&payload.source_code) {
            Ok(p) => p,
            Err(e) => {
                counter!("anvil_verify_result", "status" => "rejected", "reason" => "parse_error").increment(1);
                histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
                error!(error = %e, "Parse error in verification request");
                return Json(VerifyResponse {
                    status: "REJECTED".to_string(),
                    message: format!("Parse Error: {}", e),
                    certificate_hash: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                }).into_response();
            }
        };

        // 2. Type Check
        let type_env = typechecker::check_program(&program);
        if !type_env.errors.is_empty() {
            counter!("anvil_verify_result", "status" => "rejected", "reason" => "type_error").increment(1);
            histogram!("anvil_verify_duration_seconds").record(start.elapsed().as_secs_f64());
            error!(errors = type_env.errors.len(), "Type check failed in verification request");
            return Json(VerifyResponse {
                status: "REJECTED".to_string(),
                message: "Type Check Error: Mismatched types or undefined variables.".to_string(),
                certificate_hash: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
            }).into_response();
        }

        // 3. Verify with Z3
        let results = verifier::verify_program(&program, &type_env);
        let all_ok = results.iter().all(|r| r.verified);

        if !all_ok {
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
    axum::extract::State(pool): axum::extract::State<SqlitePool>,
    Json(payload): Json<AuthRequest>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT owner_id, tier FROM exergy_keys WHERE key_id = ? AND status = 'ACTIVE'"
    )
    .bind(&payload.key)
    .fetch_optional(&pool)
    .await;

    match result {
        Ok(Some((owner_id, tier))) => Json(AuthResponse {
            valid: true,
            owner: Some(owner_id),
            tier: Some(tier.unwrap_or_else(|| "SOVEREIGN".to_string())),
        }),
        _ => Json(AuthResponse {
            valid: false,
            owner: None,
            tier: None,
        }),
    }
}

pub async fn check_key_validity(pool: &SqlitePool, key: &str) -> bool {
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM exergy_keys WHERE key_id = ? AND status = 'ACTIVE'"
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .map(|r| r.is_some())
    .unwrap_or(false)
}
