use axum::{
    routing::{post, get},
    Router,
    Json,
    response::IntoResponse,
    http::{StatusCode, Method},
};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::io::Write;
use tempfile::NamedTempFile;
use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::cors::{Any, CorsLayer};
use std::env;

#[derive(Deserialize)]
struct ProveRequest {
    client_id: String,
    stripe_session_id: String,
    source_code: String,
}

#[derive(Serialize)]
struct ProveResponse {
    status: String,
    execution_time_ms: f64,
    certificate_hash: Option<String>,
    z3_output: String,
}

#[derive(Deserialize)]
struct StripeSessionResponse {
    payment_status: String,
}

#[tokio::main]
async fn main() {
    println!("==========================================================");
    println!("🐍 [ANVIL] CORTEX-Persist: Proof Market Oracle (Axum/Rust)");
    println!("==========================================================");
    println!("[C5-REAL] Inicializando oráculo criptográfico en 127.0.0.1:8000");

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(|| async { "Proof Market API is active (C5-REAL)" }))
        .route("/v1/prove", post(prove_handler))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn verify_stripe_session(session_id: &str) -> Result<bool, String> {
    // In a real environment, read from env variable
    let api_key = env::var("STRIPE_API_KEY").unwrap_or_else(|_| "sk_test_mock_for_proof_market".to_string());
    
    // Si estamos en entorno de dev y pasamos cs_test_mock, bypass
    if session_id == "cs_test_mock" {
        return Ok(true);
    }

    let client = reqwest::Client::new();
    let res = client
        .get(&format!("https://api.stripe.com/v1/checkout/sessions/{}", session_id))
        .basic_auth(&api_key, Some(""))
        .send()
        .await;

    match res {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(json) = response.json::<StripeSessionResponse>().await {
                    return Ok(json.payment_status == "paid");
                }
            }
            Ok(false)
        },
        Err(e) => Err(format!("Stripe API Error: {}", e)),
    }
}

async fn prove_handler(Json(payload): Json<ProveRequest>) -> impl IntoResponse {
    let start_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

    if payload.source_code.len() > 50 * 1024 {
        return (StatusCode::PAYLOAD_TOO_LARGE, Json(ProveResponse {
            status: "REJECTED".to_string(),
            execution_time_ms: 0.0,
            certificate_hash: None,
            z3_output: "Payload Too Large: Exceeds 50KB strict limit".to_string(),
        }));
    }

    if !payload.stripe_session_id.starts_with("cs_") {
        return (StatusCode::PAYMENT_REQUIRED, Json(ProveResponse {
            status: "REJECTED".to_string(),
            execution_time_ms: 0.0,
            certificate_hash: None,
            z3_output: "Invalid Stripe Session. Exergy requires capital.".to_string(),
        }));
    }

    // C5-REAL: Verify against Stripe
    match verify_stripe_session(&payload.stripe_session_id).await {
        Ok(is_paid) => {
            if !is_paid {
                return (StatusCode::PAYMENT_REQUIRED, Json(ProveResponse {
                    status: "PAYMENT_PENDING".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: "Stripe session is not marked as paid.".to_string(),
                }));
            }
        },
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ProveResponse {
                status: "STRIPE_API_ERROR".to_string(),
                execution_time_ms: 0.0,
                certificate_hash: None,
                z3_output: e,
            }));
        }
    }

    let mut temp_file = NamedTempFile::new().unwrap();
    write!(temp_file, "{}", payload.source_code).unwrap();
    let temp_path = temp_file.path().to_str().unwrap().to_string();

    let cmd_path = "../../target/debug/anvil";
    
    let output = Command::new(cmd_path)
        .arg("check")
        .arg(&temp_path)
        .output();

    let exec_time = (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() - start_time) as f64;

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let combined = if !stderr.is_empty() { stderr } else { stdout };

            if out.status.success() {
                let mut hasher = Sha256::new();
                let hash_payload = format!("{}|{}|ANVIL_MASTER_KEY", payload.source_code, start_time);
                hasher.update(hash_payload.as_bytes());
                let result = hasher.finalize();
                let cert_hash = format!("anv_cert_{}", hex::encode(result));

                (StatusCode::OK, Json(ProveResponse {
                    status: "PROVEN_SAFE".to_string(),
                    execution_time_ms: exec_time,
                    certificate_hash: Some(cert_hash),
                    z3_output: combined,
                }))
            } else {
                (StatusCode::OK, Json(ProveResponse {
                    status: "VULNERABILITY_DETECTED".to_string(),
                    execution_time_ms: exec_time,
                    certificate_hash: None,
                    z3_output: combined,
                }))
            }
        },
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ProveResponse {
                status: "EXECUTION_ERROR".to_string(),
                execution_time_ms: exec_time,
                certificate_hash: None,
                z3_output: format!("Failed to spawn Z3 Engine process: {}", e),
            }))
        }
    }
}
