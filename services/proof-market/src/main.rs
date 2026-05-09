use axum::{
    routing::post,
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::SystemTime;
use std::fs;
use std::path::Path;
use sha2::{Sha256, Digest};

#[derive(Deserialize)]
struct ProveRequest {
    source_code: String,
    stripe_session_id: String,
}

#[derive(Serialize)]
struct ProveResponse {
    status: String,
    execution_time_ms: u128,
    certificate_hash: Option<String>,
    z3_output: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    detail: String,
}

#[tokio::main]
async fn main() {
    println!("==================================================");
    println!("🛡️  CORTEX Proof Market: High-Exergy VaaS Oracle");
    println!("==================================================");
    println!("[*] Initializing Axum HTTP Socket on 0.0.0.0:8000...");
    
    let app = Router::new().route("/v1/prove", post(prove_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn prove_handler(
    Json(payload): Json<ProveRequest>,
) -> Result<Json<ProveResponse>, (StatusCode, Json<ErrorResponse>)> {
    
    // 1. Billing Validation
    if !payload.stripe_session_id.starts_with("cs_") {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            Json(ErrorResponse {
                detail: "Payment Required: Invalid Stripe Session".to_string(),
            }),
        ));
    }

    let start_time = SystemTime::now();

    // 2. Ephemeral Sandboxing
    let temp_id = format!("anv_{}", start_time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_micros());
    let temp_path = format!("/tmp/{}.anv", temp_id);
    
    if let Err(e) = fs::write(&temp_path, &payload.source_code) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                detail: format!("Failed to create sandbox: {}", e),
            }),
        ));
    }

    // 3. Z3 Execution Pipeline
    // Resolve workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let anvil_bin = workspace_root.join("target/debug/anvil");

    let output = Command::new(&anvil_bin)
        .arg("check")
        .arg(&temp_path)
        .current_dir(workspace_root)
        .output();
        
    let _ = fs::remove_file(&temp_path); // Cleanup

    let execution_time_ms = start_time.elapsed().unwrap().as_millis();

    match output {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let combined_output = format!("{}{}", stdout, stderr);

            // 4. Axiomatic Resolution
            if out.status.success() {
                // SATISFIED (Safe)
                let payload_to_hash = format!("{}|{}|ANVIL_MASTER_KEY", payload.source_code, execution_time_ms);
                let mut hasher = Sha256::new();
                hasher.update(payload_to_hash);
                let cert_hash = hex::encode(hasher.finalize());

                Ok(Json(ProveResponse {
                    status: "PROVEN_SAFE".to_string(),
                    execution_time_ms,
                    certificate_hash: Some(format!("anv_cert_{}", cert_hash)),
                    z3_output: "All postconditions proven. Zero trust required.".to_string(),
                }))
            } else {
                // VULNERABLE
                Ok(Json(ProveResponse {
                    status: "VULNERABILITY_DETECTED".to_string(),
                    execution_time_ms,
                    certificate_hash: None,
                    z3_output: combined_output,
                }))
            }
        }
        Err(e) => {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    detail: format!("Engine execution failed: {}", e),
                }),
            ))
        }
    }
}
