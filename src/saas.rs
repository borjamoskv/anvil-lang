use axum::{
    routing::{get, post},
    Router, Json, response::{IntoResponse, Html},
};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, error, info_span, Instrument};
use metrics::{counter, histogram};

use crate::parser;
use crate::typechecker;
use crate::verifier;

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
    // Initialize Prometheus metrics exporter on the same server
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("Failed to install Prometheus recorder");

    let app = Router::new()
        .route("/", get(serve_portal))
        .route("/health", get(health_check))
        .route("/v1/verify", post(verify_contract))
        .route("/metrics", get(move || {
            let handle = handle.clone();
            async move { handle.render() }
        }));

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
    Html(include_str!("../frontend/index.html"))
}

async fn verify_contract(Json(payload): Json<VerifyRequest>) -> impl IntoResponse {
    let span = info_span!("verify_contract", source_len = payload.source_code.len());

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
                });
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
            });
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
            });
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
        })
    }.instrument(span).await
}
