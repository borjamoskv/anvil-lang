use axum::{
    routing::{get, post},
    Router, Json, response::{IntoResponse, Html},
};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use std::net::SocketAddr;
use colored::Colorize;
use tokio::net::TcpListener;

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
    let app = Router::new()
        .route("/", get(serve_portal))
        .route("/health", get(health_check))
        .route("/v1/verify", post(verify_contract));

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    eprintln!("  {} Anvil Proof Market SaaS running on http://{}", "🚀".bright_green(), addr);

    let listener = TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "online",
        "engine": "Anvil+Z3 Formal Verification",
        "version": "0.5.0"
    }))
}

async fn serve_portal() -> Html<&'static str> {
    Html(include_str!("../frontend/index.html"))
}

async fn verify_contract(Json(payload): Json<VerifyRequest>) -> impl IntoResponse {
    // 1. Parse
    let program = match parser::parse_program(&payload.source_code) {
        Ok(p) => p,
        Err(e) => {
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

    Json(VerifyResponse {
        status: "VERIFIED".to_string(),
        message: "Code mathematically proven. Cryptographic certificate issued.".to_string(),
        certificate_hash: Some(format!("0x{}", hex_hash)),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}
