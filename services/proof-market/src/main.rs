use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use tokio::sync::Semaphore;

const MAX_SOURCE_BYTES: usize = 50 * 1024;
const MAX_JSON_BODY_BYTES: usize = MAX_SOURCE_BYTES * 6 + 4 * 1024;
const DEFAULT_PROCESS_TIMEOUT_SECS: u64 = 10;
const DEFAULT_STRIPE_TIMEOUT_SECS: u64 = 10;
const DEFAULT_PROCESS_MEMORY_MB: u64 = 512;
const DEFAULT_MAX_CONCURRENT_PROOFS: usize = 2;
const DEFAULT_QUEUE_TIMEOUT_SECS: u64 = 5;
const MAX_RESPONSE_OUTPUT_BYTES: usize = 16 * 1024;
const MAX_CAPTURE_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProveRequest {
    client_id: String,
    stripe_session_id: Option<String>,
    payment_mode: Option<String>,
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
    client_reference_id: Option<String>,
    amount_total: Option<i64>,
    currency: Option<String>,
    metadata: Option<HashMap<String, String>>,
}

#[derive(Clone, Copy)]
struct ProcessLimits {
    timeout: Duration,
    memory_bytes: Option<u64>,
}

struct ProcessOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
    truncated: bool,
}

#[derive(Deserialize)]
struct AnvilCheckJsonReport {
    schema_version: Option<String>,
    anvil_version: Option<String>,
    kind: Option<String>,
    status: Option<String>,
    ok: Option<bool>,
    message: Option<String>,
    error: Option<String>,
    proof_hash: Option<String>,
    errors: Option<Vec<AnvilCheckJsonDiagnostic>>,
    warnings: Option<Vec<AnvilCheckJsonDiagnostic>>,
    counterexamples: Option<Vec<AnvilCheckJsonCounterexample>>,
}

#[derive(Deserialize)]
struct AnvilCheckJsonDiagnostic {
    location: Option<String>,
    message: String,
    detail: Option<String>,
}

#[derive(Deserialize)]
struct AnvilCheckJsonCounterexample {
    fn_name: Option<String>,
    text: String,
}

enum AnvilProcessError {
    Spawn { path: PathBuf, source: io::Error },
    Io(io::Error),
    TimedOut { timeout: Duration, output: String },
    OutputLimitExceeded { output: String },
}

#[tokio::main]
async fn main() {
    println!("==========================================================");
    println!("🐍 [ANVIL] CORTEX-Persist: Proof Market Oracle (Axum/Rust)");
    println!("==========================================================");
    let bind_addr = env::var("PROOF_MARKET_ADDR").unwrap_or_else(|_| "127.0.0.1:8000".to_string());
    if mock_payment_enabled() && !mock_payment_allowed_on_bind(&bind_addr) {
        eprintln!(
            "Refusing to start with ANVIL_ALLOW_MOCK_PAYMENT=1 on non-loopback bind address {bind_addr}. Use Stripe mode for exposed deployments."
        );
        std::process::exit(1);
    }
    println!(
        "[C5-REAL] Inicializando oráculo criptográfico en {}",
        bind_addr
    );

    let app = Router::new()
        .route(
            "/health",
            get(|| async { "Proof Market API is active (C5-REAL)" }),
        )
        .route("/", get(serve_portal))
        .route("/v1/prove", post(prove_handler));

    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn serve_portal() -> Html<&'static str> {
    Html(include_str!("../../web/index.html"))
}

async fn fetch_stripe_session(session_id: &str) -> Result<Option<StripeSessionResponse>, String> {
    let api_key = env::var("STRIPE_API_KEY")
        .map_err(|_| "STRIPE_API_KEY is required to verify Stripe checkout sessions".to_string())?;

    let stripe_timeout = Duration::from_secs(
        env_u64("ANVIL_STRIPE_TIMEOUT_SECS", DEFAULT_STRIPE_TIMEOUT_SECS).max(1),
    );
    let client = reqwest::Client::builder()
        .timeout(stripe_timeout)
        .build()
        .map_err(|e| format!("Stripe HTTP client error: {}", e))?;
    let res = client
        .get(format!(
            "https://api.stripe.com/v1/checkout/sessions/{}",
            session_id
        ))
        .basic_auth(&api_key, Some(""))
        .send()
        .await;

    match res {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                let json = response
                    .json::<StripeSessionResponse>()
                    .await
                    .map_err(|e| format!("Stripe API response parse error: {}", e))?;
                return Ok(Some(json));
            }
            if matches!(status.as_u16(), 401 | 403) {
                return Err(format!("Stripe API rejected STRIPE_API_KEY ({})", status));
            }
            if matches!(status.as_u16(), 400 | 404) {
                return Ok(None);
            }
            Err(format!("Stripe API returned {}", status))
        }
        Err(e) => Err(format!("Stripe API Error: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    static ENV_LOCK: StdMutex<()> = StdMutex::new(());

    fn paid_session_for(client_id: &str) -> StripeSessionResponse {
        let mut metadata = HashMap::new();
        metadata.insert("client_id".to_string(), client_id.to_string());

        StripeSessionResponse {
            payment_status: "paid".to_string(),
            client_reference_id: None,
            amount_total: Some(500),
            currency: Some("usd".to_string()),
            metadata: Some(metadata),
        }
    }

    fn restore_env(name: &str, previous: Option<String>) {
        match previous {
            Some(value) => env::set_var(name, value),
            None => env::remove_var(name),
        }
    }

    fn with_pricing_env<T>(f: impl FnOnce() -> T) -> T {
        let old_amount = env::var("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS").ok();
        let old_currency = env::var("ANVIL_STRIPE_EXPECTED_CURRENCY").ok();
        env::set_var("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS", "500");
        env::set_var("ANVIL_STRIPE_EXPECTED_CURRENCY", "usd");
        let result = f();
        restore_env("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS", old_amount);
        restore_env("ANVIL_STRIPE_EXPECTED_CURRENCY", old_currency);
        result
    }

    #[test]
    fn stripe_validation_rejects_invalid_expected_amount_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let amount_key = "ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS";
        let old_amount = env::var(amount_key).ok();
        let old_currency = env::var("ANVIL_STRIPE_EXPECTED_CURRENCY").ok();
        env::set_var(amount_key, "abc");
        env::set_var("ANVIL_STRIPE_EXPECTED_CURRENCY", "usd");

        let result = validate_stripe_session_details(&paid_session_for("client-1"), "client-1");

        restore_env(amount_key, old_amount);
        restore_env("ANVIL_STRIPE_EXPECTED_CURRENCY", old_currency);
        assert_eq!(
            result.unwrap_err(),
            format!("{amount_key} must be an integer")
        );
    }

    #[test]
    fn stripe_validation_requires_matching_client_binding() {
        let _guard = ENV_LOCK.lock().unwrap();
        let result = with_pricing_env(|| {
            validate_stripe_session_details(&paid_session_for("client-1"), "client-2")
        });
        assert!(result.unwrap_err().contains("does not match client_id"));
    }

    #[test]
    fn stripe_validation_rejects_conflicting_client_bindings() {
        let _guard = ENV_LOCK.lock().unwrap();
        let mut session = paid_session_for("client-1");
        session.client_reference_id = Some("client-1".to_string());
        if let Some(metadata) = session.metadata.as_mut() {
            metadata.insert("client_id".to_string(), "client-2".to_string());
        }

        let result = with_pricing_env(|| validate_stripe_session_details(&session, "client-1"));

        assert!(result.unwrap_err().contains("metadata.client_id"));
    }

    #[test]
    fn stripe_validation_rejects_zero_expected_amount() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_amount = env::var("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS").ok();
        let old_currency = env::var("ANVIL_STRIPE_EXPECTED_CURRENCY").ok();
        env::set_var("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS", "0");
        env::set_var("ANVIL_STRIPE_EXPECTED_CURRENCY", "usd");

        let result = validate_stripe_session_details(&paid_session_for("client-1"), "client-1");

        restore_env("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS", old_amount);
        restore_env("ANVIL_STRIPE_EXPECTED_CURRENCY", old_currency);
        assert!(result.unwrap_err().contains("greater than zero"));
    }

    #[test]
    fn stripe_validation_requires_pricing_configuration() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_amount = env::var("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS").ok();
        let old_currency = env::var("ANVIL_STRIPE_EXPECTED_CURRENCY").ok();
        env::remove_var("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS");
        env::set_var("ANVIL_STRIPE_EXPECTED_CURRENCY", "usd");

        let result = validate_stripe_session_details(&paid_session_for("client-1"), "client-1");

        restore_env("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS", old_amount);
        restore_env("ANVIL_STRIPE_EXPECTED_CURRENCY", old_currency);
        assert!(result.unwrap_err().contains("EXPECTED_AMOUNT_CENTS"));
    }

    #[test]
    fn solver_unknown_postcondition_is_not_concrete_failure() {
        assert!(!has_concrete_verification_failure(
            "Postcondition #1: Z3 undecidable"
        ));
        assert!(has_concrete_verification_failure(
            "VERIFICATION FAILED\nPostcondition #1 failed"
        ));
    }

    #[test]
    fn valid_stripe_session_id_rejects_path_unicode_and_too_short_ids() {
        assert!(valid_stripe_session_id("cs_test_ok-123"));
        assert!(valid_stripe_session_id("cs_live_ok-123"));
        assert!(!valid_stripe_session_id("cs_"));
        assert!(!valid_stripe_session_id("cs_other_ok-123"));
        assert!(!valid_stripe_session_id("pi_test_ok-123"));
        assert!(!valid_stripe_session_id("cs_test/abc"));
        assert!(!valid_stripe_session_id("cs_test abc"));
        assert!(!valid_stripe_session_id("cs_test_é"));
    }

    #[test]
    fn reserve_stripe_session_rejects_reuse() {
        let unique = format!(
            "cs_test_reuse_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        assert!(reserve_stripe_session(&unique));
        assert!(!reserve_stripe_session(&unique));
    }

    #[test]
    fn truncate_for_response_preserves_utf8_boundary() {
        let mut output = "a".repeat(MAX_RESPONSE_OUTPUT_BYTES - 1);
        output.push('é');

        let truncated = truncate_for_response(output);

        assert!(truncated.ends_with("\n[output truncated]"));
        assert!(!truncated.contains('\u{fffd}'));
    }

    #[test]
    fn process_limits_from_env_clamps_timeout_and_disables_memory() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old_timeout = env::var("ANVIL_PROCESS_TIMEOUT_SECS").ok();
        let old_memory = env::var("ANVIL_PROCESS_MEMORY_MB").ok();
        env::set_var("ANVIL_PROCESS_TIMEOUT_SECS", "0");
        env::set_var("ANVIL_PROCESS_MEMORY_MB", "0");

        let limits = ProcessLimits::from_env();

        restore_env("ANVIL_PROCESS_TIMEOUT_SECS", old_timeout);
        restore_env("ANVIL_PROCESS_MEMORY_MB", old_memory);
        assert_eq!(limits.timeout, Duration::from_secs(1));
        assert_eq!(limits.memory_bytes, None);
    }

    #[test]
    fn mock_payment_requires_loopback_bind() {
        assert!(mock_payment_allowed_on_bind("127.0.0.1:8000"));
        assert!(mock_payment_allowed_on_bind("[::1]:8000"));
        assert!(!mock_payment_allowed_on_bind("0.0.0.0:8000"));
        assert!(!mock_payment_allowed_on_bind("[::]:8000"));
    }

    #[test]
    fn structured_success_requires_full_check_report() {
        let report: AnvilCheckJsonReport = serde_json::from_str(
            r#"{
                "schema_version": "anvil.check.v1",
                "anvil_version": "0.6.0",
                "kind": "check",
                "status": "VERIFIED",
                "ok": true,
                "proof_hash": "abc123"
            }"#,
        )
        .unwrap();
        assert!(check_report_is_success(&report));

        let empty_report: AnvilCheckJsonReport = serde_json::from_str("{}").unwrap();
        assert!(!check_report_is_success(&empty_report));

        let rejected_report: AnvilCheckJsonReport = serde_json::from_str(
            r#"{
                "schema_version": "anvil.check.v1",
                "anvil_version": "0.6.0",
                "kind": "check",
                "status": "REJECTED",
                "ok": false,
                "proof_hash": "abc123"
            }"#,
        )
        .unwrap();
        assert!(!check_report_is_success(&rejected_report));
    }
}

impl StripeSessionResponse {
    fn client_binding_error(&self, client_id: &str) -> Option<String> {
        let client_reference_id = self
            .client_reference_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let metadata_client_id = self
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("client_id"))
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());

        let mut matched = false;
        for (label, value) in [
            ("client_reference_id", client_reference_id),
            ("metadata.client_id", metadata_client_id),
        ] {
            if let Some(value) = value {
                if value != client_id {
                    return Some(format!("Stripe session {label} does not match client_id."));
                }
                matched = true;
            }
        }

        if matched {
            None
        } else {
            Some(
                "Stripe session must include client_reference_id or metadata.client_id matching client_id."
                    .to_string(),
            )
        }
    }
}

fn validate_stripe_session_details(
    session: &StripeSessionResponse,
    client_id: &str,
) -> Result<(), String> {
    if session.payment_status != "paid" {
        return Err("Stripe session is not marked as paid.".to_string());
    }

    if let Some(error) = session.client_binding_error(client_id) {
        return Err(error);
    }

    let expected_amount = env_i64("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS")?.ok_or_else(|| {
        "ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS is required in Stripe mode.".to_string()
    })?;
    if expected_amount <= 0 {
        return Err("ANVIL_STRIPE_EXPECTED_AMOUNT_CENTS must be greater than zero.".to_string());
    }
    if session.amount_total != Some(expected_amount) {
        return Err(format!(
            "Stripe session amount_total does not match expected amount: {:?} != {}",
            session.amount_total, expected_amount
        ));
    }

    let expected_currency = env::var("ANVIL_STRIPE_EXPECTED_CURRENCY")
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "ANVIL_STRIPE_EXPECTED_CURRENCY is required in Stripe mode.".to_string())?;
    if session.currency.as_deref().map(str::to_ascii_lowercase) != Some(expected_currency.clone()) {
        return Err(format!(
            "Stripe session currency does not match expected currency: {:?} != {}",
            session.currency, expected_currency
        ));
    }

    Ok(())
}

fn valid_stripe_session_id(session_id: &str) -> bool {
    (session_id.starts_with("cs_test_") || session_id.starts_with("cs_live_"))
        && (9..=255).contains(&session_id.len())
        && session_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}

fn reserve_stripe_session(session_id: &str) -> bool {
    static CONSUMED_STRIPE_SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let sessions = CONSUMED_STRIPE_SESSIONS.get_or_init(|| Mutex::new(HashSet::new()));
    let Ok(mut sessions) = sessions.lock() else {
        return false;
    };

    sessions.insert(session_id.to_string())
}

fn mock_payment_enabled() -> bool {
    env::var("ANVIL_ALLOW_MOCK_PAYMENT")
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

fn mock_payment_allowed_on_bind(bind_addr: &str) -> bool {
    bind_addr
        .parse::<SocketAddr>()
        .map(|addr| addr.ip().is_loopback())
        .unwrap_or(false)
}

fn wants_mock_payment(payload: &ProveRequest) -> bool {
    payload
        .payment_mode
        .as_deref()
        .map(|mode| mode.eq_ignore_ascii_case("mock"))
        .unwrap_or(false)
}

fn anvil_binary_path() -> PathBuf {
    if let Ok(path) = env::var("ANVIL_BIN") {
        return PathBuf::from(path);
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut candidates = Vec::new();

    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join("anvil");
            candidates.push(sibling);
        }
    }

    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        candidates.push(PathBuf::from(target_dir).join("debug").join("anvil"));
    }

    if let Some(target_dir) = cargo_metadata_target_dir(&workspace_root) {
        candidates.push(target_dir.join("debug").join("anvil"));
    }

    candidates.push(workspace_root.join("target").join("debug").join("anvil"));

    candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .cloned()
        .unwrap_or_else(|| candidates.remove(0))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn cargo_metadata_target_dir(workspace_root: &PathBuf) -> Option<PathBuf> {
    let mut child = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(workspace_root.join("Cargo.toml"))
        .current_dir(workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + Duration::from_secs(2);
    while child.try_wait().ok()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(25));
    }

    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    metadata["target_directory"].as_str().map(PathBuf::from)
}

fn certificate_secret() -> Result<String, String> {
    match env::var("ANVIL_CERTIFICATE_SECRET") {
        Ok(secret) if !secret.trim().is_empty() => Ok(secret),
        _ => Err("ANVIL_CERTIFICATE_SECRET is required to issue certificates".to_string()),
    }
}

impl ProcessLimits {
    fn from_env() -> Self {
        let timeout_secs =
            env_u64("ANVIL_PROCESS_TIMEOUT_SECS", DEFAULT_PROCESS_TIMEOUT_SECS).max(1);
        let memory_mb = env_u64("ANVIL_PROCESS_MEMORY_MB", DEFAULT_PROCESS_MEMORY_MB);

        Self {
            timeout: Duration::from_secs(timeout_secs),
            memory_bytes: if memory_mb == 0 {
                None
            } else {
                Some(memory_mb.saturating_mul(1024 * 1024))
            },
        }
    }

    fn memory_mb(self) -> Option<u64> {
        self.memory_bytes.map(|bytes| bytes / (1024 * 1024))
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_i64(name: &str) -> Result<Option<i64>, String> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => value
            .parse::<i64>()
            .map(Some)
            .map_err(|_| format!("{name} must be an integer")),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(e) => Err(format!("Failed to read {name}: {e}")),
    }
}

fn proof_semaphore() -> Arc<Semaphore> {
    static PROOF_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();
    PROOF_SEMAPHORE
        .get_or_init(|| {
            Arc::new(Semaphore::new(
                env_usize("ANVIL_MAX_CONCURRENT_PROOFS", DEFAULT_MAX_CONCURRENT_PROOFS).max(1),
            ))
        })
        .clone()
}

fn run_anvil_check_process(
    cmd_path: &Path,
    temp_path: &str,
    workspace_root: &Path,
    limits: ProcessLimits,
) -> Result<ProcessOutput, AnvilProcessError> {
    let output = run_anvil_check_process_once(cmd_path, temp_path, workspace_root, limits, true)?;
    if should_retry_without_check_json(&output) {
        return run_anvil_check_process_once(cmd_path, temp_path, workspace_root, limits, false);
    }

    Ok(output)
}

fn run_anvil_check_process_once(
    cmd_path: &Path,
    temp_path: &str,
    workspace_root: &Path,
    limits: ProcessLimits,
    json_output: bool,
) -> Result<ProcessOutput, AnvilProcessError> {
    let stdout_file = tempfile::tempfile().map_err(AnvilProcessError::Io)?;
    let stderr_file = tempfile::tempfile().map_err(AnvilProcessError::Io)?;
    let stdout_reader = stdout_file.try_clone().map_err(AnvilProcessError::Io)?;
    let stderr_reader = stderr_file.try_clone().map_err(AnvilProcessError::Io)?;

    let mut command = Command::new(cmd_path);
    command.arg("check");
    if json_output {
        command.arg("--json");
    }
    command
        .arg(temp_path)
        .current_dir(workspace_root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));

    configure_process_group(&mut command);
    apply_memory_limit(&mut command, limits.memory_bytes);

    let mut child = command.spawn().map_err(|source| AnvilProcessError::Spawn {
        path: cmd_path.to_path_buf(),
        source,
    })?;
    let deadline = Instant::now() + limits.timeout;

    loop {
        match child.try_wait().map_err(AnvilProcessError::Io)? {
            Some(_) => {
                let status = child.wait().map_err(AnvilProcessError::Io)?;
                return capture_process_output(status, stdout_reader, stderr_reader);
            }
            None if Instant::now() >= deadline => {
                if let Err(kill_error) = kill_child_tree(&mut child) {
                    if let Some(_) = child.try_wait().map_err(AnvilProcessError::Io)? {
                        let status = child.wait().map_err(AnvilProcessError::Io)?;
                        return capture_process_output(status, stdout_reader, stderr_reader);
                    }

                    return Err(AnvilProcessError::Io(kill_error));
                }
                let status = child.wait().map_err(AnvilProcessError::Io)?;
                let output = capture_process_output(status, stdout_reader, stderr_reader)?;
                return Err(AnvilProcessError::TimedOut {
                    timeout: limits.timeout,
                    output: output_text(&output),
                });
            }
            None if output_files_exceed_limit(&stdout_reader, &stderr_reader)
                .map_err(AnvilProcessError::Io)? =>
            {
                if let Err(kill_error) = kill_child_tree(&mut child) {
                    if let Some(_) = child.try_wait().map_err(AnvilProcessError::Io)? {
                        let status = child.wait().map_err(AnvilProcessError::Io)?;
                        let output = capture_process_output(status, stdout_reader, stderr_reader)?;
                        return Err(AnvilProcessError::OutputLimitExceeded {
                            output: output_text(&output),
                        });
                    }

                    return Err(AnvilProcessError::Io(kill_error));
                }
                let status = child.wait().map_err(AnvilProcessError::Io)?;
                let output = capture_process_output(status, stdout_reader, stderr_reader)?;
                return Err(AnvilProcessError::OutputLimitExceeded {
                    output: output_text(&output),
                });
            }
            None => thread::sleep(Duration::from_millis(25)),
        }
    }
}

fn output_files_exceed_limit(stdout: &File, stderr: &File) -> io::Result<bool> {
    Ok(stdout.metadata()?.len() > MAX_CAPTURE_OUTPUT_BYTES as u64
        || stderr.metadata()?.len() > MAX_CAPTURE_OUTPUT_BYTES as u64)
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn kill_child_tree(child: &mut Child) -> io::Result<()> {
    let pid = child.id() as libc::pid_t;
    let rc = unsafe { libc::killpg(pid, libc::SIGKILL) };
    if rc == 0 {
        Ok(())
    } else {
        child.kill()
    }
}

#[cfg(not(unix))]
fn kill_child_tree(child: &mut Child) -> io::Result<()> {
    child.kill()
}

fn should_retry_without_check_json(output: &ProcessOutput) -> bool {
    if !legacy_anvil_output_enabled() {
        return false;
    }
    if output.status.success() || serde_json::from_str::<serde_json::Value>(&output.stdout).is_ok()
    {
        return false;
    }

    let combined = raw_output_text(output).to_ascii_lowercase();
    combined.contains("unexpected argument '--json'")
        || combined.contains("unrecognized option '--json'")
        || combined.contains("found argument '--json'")
}

fn legacy_anvil_output_enabled() -> bool {
    env::var("ANVIL_ALLOW_LEGACY_ANVIL_OUTPUT")
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

fn capture_process_output(
    status: ExitStatus,
    stdout_file: File,
    stderr_file: File,
) -> Result<ProcessOutput, AnvilProcessError> {
    let (stdout, stdout_truncated) = read_capped(stdout_file).map_err(AnvilProcessError::Io)?;
    let (stderr, stderr_truncated) = read_capped(stderr_file).map_err(AnvilProcessError::Io)?;

    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
        truncated: stdout_truncated || stderr_truncated,
    })
}

fn read_capped(mut file: File) -> io::Result<(String, bool)> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = Vec::new();
    file.take((MAX_CAPTURE_OUTPUT_BYTES + 1) as u64)
        .read_to_end(&mut buf)?;

    let truncated = buf.len() > MAX_CAPTURE_OUTPUT_BYTES;
    if truncated {
        buf.truncate(MAX_CAPTURE_OUTPUT_BYTES);
    }

    Ok((String::from_utf8_lossy(&buf).to_string(), truncated))
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn apply_memory_limit(command: &mut Command, memory_bytes: Option<u64>) {
    use std::os::unix::process::CommandExt;

    if let Some(memory_bytes) = memory_bytes {
        unsafe {
            command.pre_exec(move || {
                let limit = libc::rlimit {
                    rlim_cur: memory_bytes as libc::rlim_t,
                    rlim_max: memory_bytes as libc::rlim_t,
                };

                if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }

                Ok(())
            });
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "android")))]
fn apply_memory_limit(_command: &mut Command, _memory_bytes: Option<u64>) {}

fn raw_output_text(output: &ProcessOutput) -> String {
    let mut combined = if output.stderr.trim().is_empty() {
        output.stdout.clone()
    } else if output.stdout.trim().is_empty() {
        output.stderr.clone()
    } else {
        format!("{}\n{}", output.stderr, output.stdout)
    };

    if output.truncated {
        combined.push_str("\n[output capture truncated]");
    }

    combined
}

fn output_text(output: &ProcessOutput) -> String {
    truncate_for_response(response_output_text(output))
}

fn truncate_for_response(mut output: String) -> String {
    if output.len() <= MAX_RESPONSE_OUTPUT_BYTES {
        return output;
    }

    let mut end = MAX_RESPONSE_OUTPUT_BYTES;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    output.truncate(end);
    output.push_str("\n[output truncated]");
    output
}

fn response_output_text(output: &ProcessOutput) -> String {
    let raw = raw_output_text(output);
    let Some(report) = parse_check_json(output) else {
        return raw;
    };

    let mut lines = Vec::new();
    if let Some(message) = report.message {
        lines.push(message);
    }
    if let Some(error) = report.error {
        if !lines.iter().any(|line| line == &error) {
            lines.push(error);
        }
    }
    append_json_diagnostics("error", report.errors, &mut lines);
    append_json_diagnostics("warning", report.warnings, &mut lines);
    if let Some(counterexamples) = report.counterexamples {
        for counterexample in counterexamples {
            match counterexample.fn_name {
                Some(fn_name) => lines.push(format!("{fn_name}: {}", counterexample.text)),
                None => lines.push(counterexample.text),
            }
        }
    }

    if lines.is_empty() {
        raw
    } else {
        lines.join("\n")
    }
}

fn append_json_diagnostics(
    label: &str,
    diagnostics: Option<Vec<AnvilCheckJsonDiagnostic>>,
    lines: &mut Vec<String>,
) {
    if let Some(diagnostics) = diagnostics {
        for diagnostic in diagnostics {
            let prefix = diagnostic
                .location
                .map(|location| format!("{label} at {location}"))
                .unwrap_or_else(|| label.to_string());
            lines.push(format!("{prefix}: {}", diagnostic.message));
            if let Some(detail) = diagnostic.detail {
                lines.push(detail);
            }
        }
    }
}

fn parse_check_json(output: &ProcessOutput) -> Option<AnvilCheckJsonReport> {
    let report: AnvilCheckJsonReport = serde_json::from_str(output.stdout.trim()).ok()?;
    (report.kind.as_deref() == Some("check")).then_some(report)
}

fn has_structured_success(output: &ProcessOutput) -> bool {
    parse_check_json(output)
        .map(|report| check_report_is_success(&report))
        .unwrap_or(false)
}

fn check_report_is_success(report: &AnvilCheckJsonReport) -> bool {
    report.schema_version.as_deref() == Some("anvil.check.v1")
        && report
            .anvil_version
            .as_ref()
            .is_some_and(|version| !version.trim().is_empty())
        && report.kind.as_deref() == Some("check")
        && report.status.as_deref() == Some("VERIFIED")
        && report.ok == Some(true)
        && report
            .proof_hash
            .as_ref()
            .is_some_and(|hash| !hash.trim().is_empty())
}

fn is_solver_exhaustion_text(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("z3 undecidable")
        || lower.contains("z3 unknown")
        || lower.contains("solver unknown")
        || lower.contains("is undecidable")
        || lower.contains("out of memory")
        || lower.contains("cannot allocate memory")
        || lower.contains("memory allocation")
}

fn has_concrete_verification_failure(output: &str) -> bool {
    output.lines().any(|line| {
        !is_solver_exhaustion_text(line)
            && (line.contains("VERIFICATION FAILED")
                || line.contains("GLOBAL INVARIANT VIOLATED")
                || line.contains("Postcondition")
                || line.contains("Contract invariant"))
    })
}

fn has_structured_resource_exhaustion(output: &ProcessOutput) -> bool {
    parse_check_json(output)
        .and_then(|report| report.status)
        .map(|status| status == "Z3_RESOURCE_EXHAUSTED")
        .unwrap_or(false)
}

fn memory_limit_label(limits: ProcessLimits) -> String {
    limits
        .memory_mb()
        .map(|mb| format!("{mb}MB process memory limit"))
        .unwrap_or_else(|| "available process memory".to_string())
}

fn resource_exhaustion_message(output: &ProcessOutput, limits: ProcessLimits) -> Option<String> {
    let combined = raw_output_text(output);
    let lower = combined.to_ascii_lowercase();

    if lower.contains("out of memory")
        || lower.contains("memory allocation")
        || lower.contains("cannot allocate memory")
    {
        let memory = memory_limit_label(limits);
        return Some(format!(
            "Z3 exhausted {memory}. Verification did not complete; no certificate was issued.\n{}",
            truncate_for_response(combined)
        ));
    }

    if lower.contains("z3 undecidable")
        || lower.contains("z3 unknown")
        || lower.contains("solver unknown")
        || lower.contains("is undecidable")
    {
        return Some(format!(
            "Z3 exhausted its solver limit before completing verification. No certificate was issued.\n{}",
            truncate_for_response(combined)
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if let Some(signal) = output.status.signal() {
            if matches!(signal, 6 | 9) {
                let memory = memory_limit_label(limits);
                return Some(format!(
                    "Z3 process terminated by signal {signal}, likely after exhausting {memory}. Verification did not complete; no certificate was issued.\n{}",
                    truncate_for_response(combined)
                ));
            }
        }
    }

    None
}

async fn prove_handler(request: Request<Body>) -> impl IntoResponse {
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();

    if !is_json_request(&request) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(ProveResponse {
                status: "REJECTED".to_string(),
                execution_time_ms: 0.0,
                certificate_hash: None,
                z3_output: "Unsupported Media Type: expected application/json".to_string(),
            }),
        );
    }

    let body = match to_bytes(request.into_body(), MAX_JSON_BODY_BYTES).await {
        Ok(body) => body,
        Err(e) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ProveResponse {
                    status: "REJECTED".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: format!(
                        "Payload Too Large: JSON body exceeds the 50KB source limit plus envelope allowance ({})",
                        e
                    ),
                }),
            );
        }
    };

    let payload: ProveRequest = match serde_json::from_slice(&body) {
        Ok(payload) => payload,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ProveResponse {
                    status: "REJECTED".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: format!("Invalid JSON: {}", e),
                }),
            );
        }
    };

    if payload.client_id.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ProveResponse {
                status: "REJECTED".to_string(),
                execution_time_ms: 0.0,
                certificate_hash: None,
                z3_output: "Invalid JSON: client_id must be a non-empty string".to_string(),
            }),
        );
    }

    if payload.source_code.as_bytes().len() > MAX_SOURCE_BYTES {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ProveResponse {
                status: "REJECTED".to_string(),
                execution_time_ms: 0.0,
                certificate_hash: None,
                z3_output: "Payload Too Large: Exceeds 50KB strict limit".to_string(),
            }),
        );
    }

    if payload.source_code.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ProveResponse {
                status: "REJECTED".to_string(),
                execution_time_ms: 0.0,
                certificate_hash: None,
                z3_output: "Invalid JSON: source_code must be a non-empty string".to_string(),
            }),
        );
    }

    let certificate_secret = match certificate_secret() {
        Ok(secret) => secret,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProveResponse {
                    status: "CONFIGURATION_ERROR".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: e,
                }),
            );
        }
    };

    let mut paid_stripe_session = None;

    if wants_mock_payment(&payload) {
        if !mock_payment_enabled() {
            return (
                StatusCode::FORBIDDEN,
                Json(ProveResponse {
                    status: "MOCK_PAYMENT_DISABLED".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output:
                        "Mock payment is disabled. Set ANVIL_ALLOW_MOCK_PAYMENT=1 for local demos."
                            .to_string(),
                }),
            );
        }
    } else {
        let stripe_session_id = match payload.stripe_session_id.as_deref() {
            Some(session_id) if valid_stripe_session_id(session_id) => session_id,
            _ => {
                return (
                    StatusCode::PAYMENT_REQUIRED,
                    Json(ProveResponse {
                        status: "REJECTED".to_string(),
                        execution_time_ms: 0.0,
                        certificate_hash: None,
                        z3_output: "Invalid Stripe Session. Exergy requires capital.".to_string(),
                    }),
                );
            }
        };

        match fetch_stripe_session(stripe_session_id).await {
            Ok(Some(session)) => {
                if let Err(e) = validate_stripe_session_details(&session, &payload.client_id) {
                    return (
                        StatusCode::PAYMENT_REQUIRED,
                        Json(ProveResponse {
                            status: "PAYMENT_REJECTED".to_string(),
                            execution_time_ms: 0.0,
                            certificate_hash: None,
                            z3_output: e,
                        }),
                    );
                }
            }
            Ok(None) => {
                return (
                    StatusCode::PAYMENT_REQUIRED,
                    Json(ProveResponse {
                        status: "PAYMENT_REJECTED".to_string(),
                        execution_time_ms: 0.0,
                        certificate_hash: None,
                        z3_output: "Stripe session was not found.".to_string(),
                    }),
                );
            }
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    Json(ProveResponse {
                        status: "STRIPE_API_ERROR".to_string(),
                        execution_time_ms: 0.0,
                        certificate_hash: None,
                        z3_output: e,
                    }),
                );
            }
        }

        paid_stripe_session = Some(stripe_session_id.to_string());
    }

    let mut temp_file = match NamedTempFile::new() {
        Ok(file) => file,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProveResponse {
                    status: "EXECUTION_ERROR".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: format!("Failed to create temporary source file: {}", e),
                }),
            );
        }
    };
    if let Err(e) = write!(temp_file, "{}", payload.source_code) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProveResponse {
                status: "EXECUTION_ERROR".to_string(),
                execution_time_ms: 0.0,
                certificate_hash: None,
                z3_output: format!("Failed to write temporary source file: {}", e),
            }),
        );
    }
    let temp_path = match temp_file.path().to_str() {
        Some(path) => path.to_string(),
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProveResponse {
                    status: "EXECUTION_ERROR".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: "Temporary source path is not valid UTF-8.".to_string(),
                }),
            );
        }
    };

    let limits = ProcessLimits::from_env();
    let queue_timeout =
        Duration::from_secs(env_u64("ANVIL_QUEUE_TIMEOUT_SECS", DEFAULT_QUEUE_TIMEOUT_SECS).max(1));
    let proof_permit =
        match tokio::time::timeout(queue_timeout, proof_semaphore().acquire_owned()).await {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ProveResponse {
                        status: "EXECUTION_ERROR".to_string(),
                        execution_time_ms: 0.0,
                        certificate_hash: None,
                        z3_output: "Proof execution queue is unavailable.".to_string(),
                    }),
                );
            }
            Err(_) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(ProveResponse {
                        status: "PROOF_QUEUE_FULL".to_string(),
                        execution_time_ms: 0.0,
                        certificate_hash: None,
                        z3_output: format!(
                            "Proof execution queue did not admit the request within {}s.",
                            queue_timeout.as_secs()
                        ),
                    }),
                );
            }
        };

    if let Some(stripe_session_id) = paid_stripe_session.as_deref() {
        if !reserve_stripe_session(stripe_session_id) {
            return (
                StatusCode::CONFLICT,
                Json(ProveResponse {
                    status: "PAYMENT_SESSION_REUSED".to_string(),
                    execution_time_ms: 0.0,
                    certificate_hash: None,
                    z3_output: "Stripe session has already been used for a proof attempt."
                        .to_string(),
                }),
            );
        }
    }

    let output = {
        let temp_path = temp_path.clone();
        tokio::task::spawn_blocking(move || {
            let _proof_permit = proof_permit;
            let cmd_path = anvil_binary_path();
            let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
            run_anvil_check_process(&cmd_path, &temp_path, &workspace_root, limits)
        })
        .await
    };

    let exec_time = (SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        - start_time) as f64;

    match output {
        Ok(Ok(out)) => {
            let combined = output_text(&out);
            let raw_combined = raw_output_text(&out);

            if out.status.success() {
                if !has_structured_success(&out) && !legacy_anvil_output_enabled() {
                    return (
                        StatusCode::BAD_GATEWAY,
                        Json(ProveResponse {
                            status: "EXECUTION_ERROR".to_string(),
                            execution_time_ms: exec_time,
                            certificate_hash: None,
                            z3_output: "Anvil did not return a structured successful check report. Refusing to issue a certificate.".to_string(),
                        }),
                    );
                }

                let mut hasher = Sha256::new();
                let hash_payload = format!(
                    "{}|{}|{}|{}",
                    payload.client_id, payload.source_code, start_time, certificate_secret
                );
                hasher.update(hash_payload.as_bytes());
                let result = hasher.finalize();
                let cert_hash = format!("anv_cert_{}", hex::encode(result));

                (
                    StatusCode::OK,
                    Json(ProveResponse {
                        status: "PROVEN_SAFE".to_string(),
                        execution_time_ms: exec_time,
                        certificate_hash: Some(cert_hash),
                        z3_output: combined,
                    }),
                )
            } else {
                match (
                    has_concrete_verification_failure(&raw_combined),
                    structured_or_legacy_resource_exhaustion_message(&out, limits),
                ) {
                    (false, Some(message)) => (
                        StatusCode::GATEWAY_TIMEOUT,
                        Json(ProveResponse {
                            status: "Z3_RESOURCE_EXHAUSTED".to_string(),
                            execution_time_ms: exec_time,
                            certificate_hash: None,
                            z3_output: message,
                        }),
                    ),
                    _ => (
                        StatusCode::OK,
                        Json(ProveResponse {
                            status: "VULNERABILITY_DETECTED".to_string(),
                            execution_time_ms: exec_time,
                            certificate_hash: None,
                            z3_output: combined,
                        }),
                    ),
                }
            }
        }
        Ok(Err(AnvilProcessError::TimedOut { timeout, output })) => (
            StatusCode::GATEWAY_TIMEOUT,
            Json(ProveResponse {
                status: "Z3_RESOURCE_EXHAUSTED".to_string(),
                execution_time_ms: exec_time,
                certificate_hash: None,
                z3_output: format!(
                    "Z3 exhausted the {}s process timeout. Verification did not complete; no certificate was issued.\n{}",
                    timeout.as_secs(),
                    output
                ),
            }),
        ),
        Ok(Err(AnvilProcessError::OutputLimitExceeded { output })) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ProveResponse {
                status: "EXECUTION_ERROR".to_string(),
                execution_time_ms: exec_time,
                certificate_hash: None,
                z3_output: format!(
                    "Anvil output exceeded the {} byte capture limit. Verification was stopped; no certificate was issued.\n{}",
                    MAX_CAPTURE_OUTPUT_BYTES, output
                ),
            }),
        ),
        Ok(Err(AnvilProcessError::Spawn { path, source })) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProveResponse {
                status: "EXECUTION_ERROR".to_string(),
                execution_time_ms: exec_time,
                certificate_hash: None,
                z3_output: format!(
                    "Failed to spawn Z3 Engine process at {}: {}",
                    path.display(),
                    source
                ),
            }),
        ),
        Ok(Err(AnvilProcessError::Io(e))) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProveResponse {
                status: "EXECUTION_ERROR".to_string(),
                execution_time_ms: exec_time,
                certificate_hash: None,
                z3_output: format!("Failed while supervising Z3 Engine process: {}", e),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProveResponse {
                status: "EXECUTION_ERROR".to_string(),
                execution_time_ms: exec_time,
                certificate_hash: None,
                z3_output: format!("Z3 supervisor task failed: {}", e),
            }),
        ),
    }
}

fn is_json_request(request: &Request<Body>) -> bool {
    request
        .headers()
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

fn structured_or_legacy_resource_exhaustion_message(
    output: &ProcessOutput,
    limits: ProcessLimits,
) -> Option<String> {
    if has_structured_resource_exhaustion(output) {
        return Some(format!(
            "Z3 exhausted its solver limit before completing verification. No certificate was issued.\n{}",
            output_text(output)
        ));
    }

    resource_exhaustion_message(output, limits)
}
