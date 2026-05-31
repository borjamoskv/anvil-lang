// ============================================================
// ANVIL INTEGRATION TESTS — Full Pipeline Verification
// Parse → Type Check → Z3 Verify → Codegen
//
// Every .anv example file is exercised end-to-end.
// These tests actually invoke Z3 — they are the real proof.
// ============================================================

// Since this is an integration test, we need to compile as a library.
// We import via the binary crate's modules directly.
// Integration tests run against the compiled binary, so we invoke anvil CLI.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const ANVIL_PROCESS_TIMEOUT_SECONDS: u64 = 60;

fn anvil_cli_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn anvil_binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_anvil") {
        return PathBuf::from(path);
    }

    if let Ok(path) = std::env::var("CARGO_BIN_EXE_anvil") {
        return PathBuf::from(path);
    }

    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove 'deps'
    path.push("anvil");
    path
}

fn output_with_timeout(mut command: Command, label: &str) -> Output {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn {label}: {e}"));
    let mut stdout = child.stdout.take().expect("stdout should be piped");
    let mut stderr = child.stderr.take().expect("stderr should be piped");

    let stdout_handle = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .read_to_end(&mut bytes)
            .expect("failed to read child stdout");
        bytes
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr
            .read_to_end(&mut bytes)
            .expect("failed to read child stderr");
        bytes
    });

    let timeout = Duration::from_secs(ANVIL_PROCESS_TIMEOUT_SECONDS);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .unwrap_or_else(|e| panic!("Failed to poll {label}: {e}"))
        {
            break status;
        }

        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let stdout = stdout_handle.join().expect("stdout reader panicked");
            let stderr = stderr_handle.join().expect("stderr reader panicked");
            panic!(
                "{label} exceeded {timeout:?}\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr)
            );
        }

        std::thread::sleep(Duration::from_millis(25));
    };

    Output {
        status,
        stdout: stdout_handle.join().expect("stdout reader panicked"),
        stderr: stderr_handle.join().expect("stderr reader panicked"),
    }
}

fn run_anvil_check(example: &str) -> (bool, String) {
    let _guard = anvil_cli_lock().lock().unwrap_or_else(|e| e.into_inner());
    let binary = anvil_binary();
    let example_path = Path::new("examples").join(example);

    let mut command = Command::new(&binary);
    command.args([
        "check",
        "--timeout",
        "30000",
        example_path.to_str().unwrap(),
    ]);
    let output = output_with_timeout(command, "anvil check");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
}

fn run_anvil_check_json(example: &str) -> (bool, serde_json::Value, String, String) {
    let example_path = Path::new("examples").join(example);
    run_anvil_check_json_path(&example_path)
}

fn run_anvil_check_json_path(path: &Path) -> (bool, serde_json::Value, String, String) {
    let _guard = anvil_cli_lock().lock().unwrap_or_else(|e| e.into_inner());
    let binary = anvil_binary();

    let mut command = Command::new(&binary);
    command.args([
        "check",
        "--timeout",
        "30000",
        "--json",
        path.to_str().unwrap(),
    ]);
    let output = output_with_timeout(command, "anvil check --json");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);
    let json = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("Invalid check --json output: {}\n{}", e, combined));

    (output.status.success(), json, stdout, stderr)
}

fn temp_anvil_source(name: &str, source: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "anvil_json_{}_{}_{}.anv",
        std::process::id(),
        nanos,
        name
    ));
    std::fs::write(&path, source).expect("failed to write temp Anvil source");
    path
}

fn run_anvil_build(example: &str, target: &str) -> (bool, String) {
    let example_path = Path::new("examples").join(example);
    run_anvil_build_path(&example_path, target)
}

fn run_anvil_build_path(path: &Path, target: &str) -> (bool, String) {
    let _guard = anvil_cli_lock().lock().unwrap_or_else(|e| e.into_inner());
    let binary = anvil_binary();
    let output_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("source");
    let output_path = std::env::temp_dir().join(format!("anvil_test_{}", output_name));

    let mut args = vec![
        "build".to_string(),
        "--timeout".to_string(),
        "30000".to_string(),
        path.to_str().unwrap().to_string(),
        "-o".to_string(),
        output_path.to_str().unwrap().to_string(),
    ];

    if target == "llvm" {
        args.push("--target".to_string());
        args.push("llvm".to_string());
    }

    let mut command = Command::new(&binary);
    command.args(&args);
    let output = output_with_timeout(command, "anvil build");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
}

fn run_anvil_ast(example: &str) -> (bool, String) {
    let example_path = Path::new("examples").join(example);
    run_anvil_ast_path(&example_path)
}

fn run_anvil_ast_path(path: &Path) -> (bool, String) {
    let _guard = anvil_cli_lock().lock().unwrap_or_else(|e| e.into_inner());
    let binary = anvil_binary();

    let mut command = Command::new(&binary);
    command.args(["ast", path.to_str().unwrap()]);
    let output = output_with_timeout(command, "anvil ast");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (output.status.success(), format!("{}\n{}", stdout, stderr))
}

// ============================================================
// PHASE A: Parse-Only Tests (every .anv file must parse)
// ============================================================

macro_rules! parse_test {
    ($name:ident, $file:expr) => {
        #[test]
        fn $name() {
            let (success, output) = run_anvil_ast($file);
            assert!(success, "Failed to parse {}: {}", $file, output);
            // AST dump should be valid JSON
            let stdout_part: String = output
                .lines()
                .take_while(|l| !l.contains("ANVIL"))
                .collect::<Vec<_>>()
                .join("\n");
            // Just check it's non-empty and starts with valid JSON
            assert!(
                !stdout_part.trim().is_empty() || output.contains("items"),
                "AST output is empty for {}",
                $file
            );
        }
    };
}

parse_test!(parse_hello, "hello.anv");
parse_test!(parse_transfer, "transfer.anv");
parse_test!(parse_safe_transfer, "safe_transfer.anv");
parse_test!(parse_token, "token.anv");
parse_test!(parse_vault, "vault.anv");
parse_test!(parse_reentrancy, "reentrancy.anv");
parse_test!(parse_overflow, "overflow.anv");
parse_test!(parse_amm_pool, "amm_pool.anv");
parse_test!(parse_oracle_manipulation, "oracle_manipulation.anv");
parse_test!(parse_loops, "loops.anv");
parse_test!(parse_while_loops, "while_loops.anv");
parse_test!(parse_ssa, "ssa.anv");
parse_test!(parse_stress_test, "stress_test.anv");
parse_test!(parse_bounty_hunter, "bounty_hunter.anv");
parse_test!(parse_bitflow_drift, "bitflow_drift.anv");
parse_test!(parse_k2_close_factor, "k2_close_factor_bypass.anv");
parse_test!(parse_k2_mempool_bandit, "k2_mempool_bandit_strike.anv");
parse_test!(parse_firedancer, "firedancer_ghosting_strike.anv");
parse_test!(parse_uniswap_ingested, "uniswap_ingested.anv");
parse_test!(parse_vacuous, "vacuous.anv");

// ============================================================
// PHASE B: Full Pipeline — Verification Result Tests
// These actually invoke Z3 and check correctness.
// ============================================================

// --- Files that should be fully PROVEN ---

#[test]
fn verify_hello_proven() {
    let (success, output) = run_anvil_check("hello.anv");
    assert!(success, "hello.anv should verify: {}", output);
    assert!(
        output.contains("proven") || output.contains("PROVEN") || output.contains("✓"),
        "Expected proven output for hello.anv: {}",
        output
    );
}

#[test]
fn verify_transfer_proven() {
    let (success, output) = run_anvil_check("transfer.anv");
    assert!(success, "transfer.anv should verify: {}", output);
    assert!(
        output.contains("✓"),
        "Expected proven checkmark: {}",
        output
    );
}

#[test]
fn verify_safe_transfer_proven() {
    let (success, output) = run_anvil_check("safe_transfer.anv");
    assert!(success, "safe_transfer.anv should verify: {}", output);
}

#[test]
fn verify_overflow_proven() {
    let (success, output) = run_anvil_check("overflow.anv");
    assert!(success, "overflow.anv should verify: {}", output);
}

// --- Files that should contain REJECTIONS (counterexamples found) ---

#[test]
fn verify_reentrancy_rejected() {
    let (success, output) = run_anvil_check("reentrancy.anv");
    // reentrancy.anv has both safe and broken withdraw — at least one fails
    assert!(
        !success || output.contains("FAILED") || output.contains("✗"),
        "reentrancy.anv should contain at least one rejection: {}",
        output
    );
}

// --- Mixed files: some functions proven, some rejected ---

#[test]
fn verify_token_mixed() {
    let (_success, output) = run_anvil_check("token.anv");
    // token.anv has contract invariants — some should pass, some might fail
    assert!(
        output.contains("✓") || output.contains("✗") || output.contains("postcondition"),
        "token.anv should produce verification output: {}",
        output
    );
}

#[test]
fn verify_vault_mixed() {
    let (_success, output) = run_anvil_check("vault.anv");
    // vault.anv has safe deposit, broken withdraw, and donation attack
    assert!(
        output.contains("VERIFICATION REPORT"),
        "vault.anv should produce a verification report: {}",
        output
    );
}

#[test]
fn verify_loops_mixed() {
    let (_success, output) = run_anvil_check("loops.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "loops.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_ssa_mixed() {
    let (_success, output) = run_anvil_check("ssa.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "ssa.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_while_loops_mixed() {
    let (_success, output) = run_anvil_check("while_loops.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "while_loops.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_stress_test() {
    let (_success, output) = run_anvil_check("stress_test.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "stress_test.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_amm_pool() {
    let (_success, output) = run_anvil_check("amm_pool.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "amm_pool.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_oracle_manipulation() {
    let (_success, output) = run_anvil_check("oracle_manipulation.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "oracle_manipulation.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_bounty_hunter() {
    let (_success, output) = run_anvil_check("bounty_hunter.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "bounty_hunter.anv should produce a report: {}",
        output
    );
}

#[test]
fn verify_k2_close_factor() {
    let (_success, output) = run_anvil_check("k2_close_factor_bypass.anv");
    assert!(
        output.contains("VERIFICATION REPORT"),
        "k2_close_factor_bypass.anv should produce a report: {}",
        output
    );
}

// ============================================================
// PHASE C: Codegen Tests
// ============================================================

#[test]
fn build_transfer_rust() {
    let (success, output) = run_anvil_build("transfer.anv", "rust");
    assert!(success, "transfer.anv should build to Rust: {}", output);
    assert!(
        output.contains("Generated") || output.contains(".rs"),
        "Should indicate Rust file generation: {}",
        output
    );
}

#[test]
fn build_transfer_llvm() {
    let (success, output) = run_anvil_build("transfer.anv", "llvm");
    assert!(success, "transfer.anv should build to LLVM IR: {}", output);
    assert!(
        output.contains("Generated") || output.contains(".ll"),
        "Should indicate LLVM IR generation: {}",
        output
    );
}

#[test]
fn build_safe_transfer_rust() {
    let (success, output) = run_anvil_build("safe_transfer.anv", "rust");
    assert!(
        success,
        "safe_transfer.anv should build to Rust: {}",
        output
    );
}

#[test]
fn build_hello_rust() {
    let (success, output) = run_anvil_build("hello.anv", "rust");
    assert!(success, "hello.anv should build to Rust: {}", output);
}

// ============================================================
// PHASE D: Regression Tests for Specific Bugs
// ============================================================

#[test]
fn regression_proof_hash_present() {
    let (success, output) = run_anvil_check("transfer.anv");
    assert!(success);
    // Proof hash should appear in output (🔐 prefix)
    assert!(
        output.contains("proof:") || output.contains("🔐"),
        "Proof hash should be present in output: {}",
        output
    );
}

#[test]
fn regression_check_json_success_contract() {
    let (success, json, stdout, stderr) = run_anvil_check_json("transfer.anv");
    assert!(
        success,
        "transfer.anv should verify:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stderr.trim().is_empty(),
        "check --json should keep stderr clean by default: {}",
        stderr
    );

    assert_eq!(json["schema_version"], "anvil.check.v1");
    assert_eq!(json["kind"], "check");
    assert_eq!(json["status"], "VERIFIED");
    assert_eq!(json["ok"], true);
    assert_eq!(json["all_verified"], true);
    assert_eq!(json["timeout_ms"], 30000);
    assert!(
        json["duration_ms"].is_number(),
        "missing duration: {}",
        json
    );
    assert_eq!(json["proof"]["hash_algorithm"], "sha3-256");
    assert_eq!(
        json["proof_hash"].as_str().map(str::len),
        Some(64),
        "aggregate proof hash missing: {}",
        json
    );
    assert!(json["warnings"].is_array(), "warnings must be stable JSON");
    assert_eq!(json["summary"]["errors_total"], 0);
    assert!(
        json["counterexamples"].as_array().unwrap().is_empty(),
        "verified file should not report counterexamples: {}",
        json
    );

    let results = json["results"]
        .as_array()
        .expect("results must be an array");
    assert!(!results.is_empty(), "expected at least one proof result");
    for result in results {
        assert_eq!(result["status"], "VERIFIED");
        assert_eq!(result["verified"], true);
        assert_eq!(
            result["proof_hash"].as_str().map(str::len),
            Some(64),
            "function proof hash missing: {}",
            result
        );
        assert!(result["duration_ms"].is_number());
        assert!(result["warnings"].is_array());
        assert!(result["counterexamples"].is_array());
    }
}

#[test]
fn regression_check_timeout_flag_is_accepted() {
    let _guard = anvil_cli_lock().lock().unwrap_or_else(|e| e.into_inner());
    let binary = anvil_binary();
    let example_path = Path::new("examples").join("transfer.anv");

    let mut command = Command::new(&binary);
    command.args([
        "check",
        "--timeout",
        "30000",
        example_path.to_str().unwrap(),
    ]);
    let output = output_with_timeout(command, "anvil check --timeout 30000");

    assert!(
        output.status.success(),
        "check --timeout should verify transfer.anv: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn regression_check_rejects_zero_timeout() {
    let _guard = anvil_cli_lock().lock().unwrap_or_else(|e| e.into_inner());
    let binary = anvil_binary();
    let example_path = Path::new("examples").join("transfer.anv");

    let mut command = Command::new(&binary);
    command.args(["check", "--timeout", "0", example_path.to_str().unwrap()]);
    let output = output_with_timeout(command, "anvil check --timeout 0");

    assert!(
        !output.status.success(),
        "check --timeout 0 should fail clap validation"
    );
}

#[test]
fn regression_check_json_counterexample_contract() {
    let (success, json, stdout, stderr) = run_anvil_check_json("reentrancy.anv");
    assert!(
        !success,
        "reentrancy.anv should expose a rejected proof:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stderr.trim().is_empty(),
        "check --json should keep stderr clean by default: {}",
        stderr
    );

    assert_eq!(json["schema_version"], "anvil.check.v1");
    assert_eq!(json["kind"], "check");
    assert_eq!(json["ok"], false);
    assert_eq!(json["status"], "REJECTED");
    assert!(
        json["duration_ms"].is_number(),
        "missing duration: {}",
        json
    );
    assert!(
        json["counterexamples"]
            .as_array()
            .is_some_and(|counterexamples| !counterexamples.is_empty()),
        "counterexamples must be structured: {}",
        json
    );

    let first = &json["counterexamples"][0];
    assert!(first["fn_name"].is_string(), "fn_name missing: {}", first);
    assert!(first["kind"].is_string(), "kind missing: {}", first);
    assert!(first["text"].is_string(), "text missing: {}", first);
    assert!(first["lines"].is_array(), "lines missing: {}", first);
}

#[test]
fn regression_check_json_parse_error_contract() {
    let path = temp_anvil_source("parse_error", "fn broken(");
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "invalid source should fail:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stderr.trim().is_empty(),
        "parse errors in JSON mode should keep stderr clean: {}",
        stderr
    );
    assert_eq!(json["schema_version"], "anvil.check.v1");
    assert_eq!(json["status"], "PARSE_ERROR");
    assert_eq!(json["ok"], false);
    assert_eq!(json["proof_hash"], serde_json::Value::Null);
    assert_eq!(json["summary"]["errors_total"], 1);
    assert!(
        json["errors"]
            .as_array()
            .is_some_and(|errors| errors.len() == 1)
    );
    assert!(json["results"].as_array().unwrap().is_empty());
}

#[test]
fn regression_check_json_type_error_contract() {
    let path = temp_anvil_source(
        "type_error",
        r#"
fn bad(a: u64) -> bool
{
    return a;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "type-invalid source should fail:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stderr.trim().is_empty(),
        "type errors in JSON mode should keep stderr clean: {}",
        stderr
    );
    assert_eq!(json["schema_version"], "anvil.check.v1");
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["ok"], false);
    assert_eq!(json["functions"], 1);
    assert_eq!(json["summary"]["functions_failed"], 1);
    assert_eq!(json["summary"]["errors_total"], 1);
    assert!(
        json["errors"]
            .as_array()
            .is_some_and(|errors| errors.len() == 1)
    );
    assert!(json["results"].as_array().unwrap().is_empty());
}

#[test]
fn regression_check_json_io_error_contract() {
    let missing = std::env::temp_dir().join(format!(
        "anvil_json_missing_{}_{}.anv",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&missing);

    assert!(
        !success,
        "missing source should fail:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert!(
        stderr.trim().is_empty(),
        "IO errors in JSON mode should keep stderr clean: {}",
        stderr
    );
    assert_eq!(json["schema_version"], "anvil.check.v1");
    assert_eq!(json["status"], "IO_ERROR");
    assert_eq!(json["ok"], false);
    assert_eq!(json["summary"]["errors_total"], 1);
    assert!(
        json["errors"]
            .as_array()
            .is_some_and(|errors| errors.len() == 1)
    );
    assert!(json["results"].as_array().unwrap().is_empty());
}

#[test]
fn regression_check_json_contract_function_counts() {
    let (_success, json, _stdout, stderr) = run_anvil_check_json("token.anv");
    assert!(
        stderr.trim().is_empty(),
        "contract JSON mode should keep stderr clean: {}",
        stderr
    );

    let results_len = json["results"].as_array().unwrap().len() as u64;
    assert_eq!(
        results_len, 3,
        "token.anv fixture should verify 3 functions"
    );
    assert_eq!(json["functions"], results_len);
    assert_eq!(json["summary"]["functions_total"], results_len);
    assert_eq!(
        json["summary"]["functions_verified"].as_u64().unwrap()
            + json["summary"]["functions_failed"].as_u64().unwrap(),
        results_len
    );
    assert!(
        json["invariants"].as_u64().unwrap() >= results_len,
        "contract-level invariants should be included in invariant totals: {}",
        json
    );
}

#[test]
fn regression_cortex_manifest_emitted() {
    let (success, output) = run_anvil_build("transfer.anv", "rust");
    assert!(success);
    assert!(
        output.contains("CORTEX manifest") || output.contains("cortex_manifest"),
        "CORTEX manifest should be emitted on build: {}",
        output
    );
}

#[test]
fn regression_type_constraints_registered() {
    let (success, output) = run_anvil_check("transfer.anv");
    assert!(success);
    assert!(
        output.contains("type constraints"),
        "Type constraints should be reported: {}",
        output
    );
}

#[test]
fn regression_cli_version() {
    let binary = anvil_binary();
    let mut command = Command::new(&binary);
    command.arg("--version");
    let output = output_with_timeout(command, "anvil --version");
    let version_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        version_str.contains("0.6.0") || version_str.contains("0.7.0"),
        "CLI version should match Cargo.toml: got '{}'",
        version_str.trim()
    );
}

// ============================================================
// PHASE E: Edge Cases and Soundness Checks
// ============================================================

#[test]
fn soundness_vacuous_invariant() {
    let (success, json, stdout, stderr) = run_anvil_check_json("vacuous.anv");

    assert!(
        !success,
        "vacuous proof must be rejected:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["ok"], false);
    assert_eq!(json["all_verified"], false);
    assert_eq!(json["results"][0]["fn_name"], "foo");
    assert_eq!(json["results"][0]["status"], "REJECTED");
    assert!(
        json["counterexamples"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .contains("Pre-state constraints are inconsistent"),
        "counterexample should explain the vacuous rejection: {}",
        json
    );
}

#[test]
fn soundness_bitflow_drift() {
    let (_success, output) = run_anvil_check("bitflow_drift.anv");
    // Should at minimum parse and produce verification output
    assert!(
        output.contains("VERIFICATION REPORT") || output.contains("postcondition"),
        "bitflow_drift.anv should produce output: {}",
        output
    );
}

#[test]
fn soundness_no_invariants_not_verified() {
    let path = temp_anvil_source(
        "no_invariants",
        "fn no_invariants(mut x: u64) -> u64 { x += 1; return x; }",
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "zero-invariant source must not verify:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["ok"], false);
    assert_eq!(json["all_verified"], false);
    assert_eq!(json["results"].as_array().unwrap().len(), 0);
    assert_eq!(json["proof_hash"], serde_json::Value::Null);
}

#[test]
fn soundness_impossible_if_branch_rejected() {
    let path = temp_anvil_source(
        "impossible_if",
        r#"
fn impossible_if(mut x: u64) -> u64
    where { x == 0, x' == 1 }
{
    if x > 0 {
        x = 1;
    }
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "impossible if branch must be rejected:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["fn_name"], "impossible_if");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_assert_false_rejected() {
    let path = temp_anvil_source(
        "assert_false",
        r#"
fn assert_false(mut x: u64) -> u64
    where { x' == x }
{
    assert(false, "must fail");
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "assert(false) must fail verification:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["fn_name"], "assert_false");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_signed_local_assertion_rejected() {
    let path = temp_anvil_source(
        "signed_local_assertion",
        r#"
fn signed_local_assertion(mut x: u64) -> u64
    where { x' == x }
{
    let y: i8 = -1;
    assert(y > 0, "signed local must use signed comparison");
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "signed local false assertion must fail verification:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["fn_name"], "signed_local_assertion");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_fncall_args_not_collapsed() {
    let path = temp_anvil_source(
        "fncall_args",
        r#"
fn fncall_args(mut a: u64, mut b: u64) -> u64
    where { a' == b' }
{
    a = f(0);
    b = f(1);
    return a;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "f(0) and f(1) must not collapse to the same symbolic value:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["fn_name"], "fncall_args");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_loop_invariant_must_be_established() {
    let path = temp_anvil_source(
        "loop_invariant",
        r#"
fn loop_invariant(mut x: u64) -> u64
    where { x == 0, x' == 10 }
{
    while x < 1 where { x == 10 } {
        x += 1;
    }
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "loop invariants must be established before being used:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["fn_name"], "loop_invariant");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_large_literal_not_silently_zero() {
    let path = temp_anvil_source(
        "large_literal",
        r#"
fn large_literal(mut x: u256) -> u256
    where { x == 0, x' == 0 }
{
    x = 18446744073709551616;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "large literals must not silently collapse to zero:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_ne!(json["status"], "VERIFIED");
}

#[test]
fn soundness_large_u256_literal_can_verify() {
    let path = temp_anvil_source(
        "large_u256_literal",
        r#"
fn large_u256_literal(mut x: u256) -> u256
    where { x == 0, x' == 18446744073709551616 }
{
    x = 18446744073709551616;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "u256 literals above u64 must be encoded at full width:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
    assert_eq!(json["results"][0]["status"], "VERIFIED");
}

#[test]
fn soundness_u256_max_literal_can_verify() {
    let path = temp_anvil_source(
        "u256_max_literal",
        r#"
fn u256_max_literal(mut x: u256) -> u256
    where { x == 0, x' == 115792089237316195423570985008687907853269984665640564039457584007913129639935 }
{
    x = 115792089237316195423570985008687907853269984665640564039457584007913129639935;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "u256::MAX must be encoded and verified at full width:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
    assert_eq!(json["results"][0]["status"], "VERIFIED");
}

#[test]
fn soundness_u256_literal_above_max_rejected() {
    let path = temp_anvil_source(
        "u256_above_max",
        r#"
fn u256_above_max(mut x: u256) -> u256
    where { x == 0, x' == 0 }
{
    x = 115792089237316195423570985008687907853269984665640564039457584007913129639936;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "literal above u256::MAX must be rejected:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "PARSE_ERROR");
    assert_eq!(json["ok"], false);
}

#[test]
fn soundness_hex_u256_boundaries() {
    let max_path = temp_anvil_source(
        "hex_u256_max",
        r#"
fn hex_u256_max(mut x: u256) -> u256
    where { x == 0, x' == 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff }
{
    x = 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;
    return x;
}
"#,
    );
    let (max_success, max_json, max_stdout, max_stderr) = run_anvil_check_json_path(&max_path);
    let _ = std::fs::remove_file(&max_path);

    assert!(
        max_success,
        "64-nibble hex u256::MAX must verify:\nstdout:\n{}\nstderr:\n{}",
        max_stdout, max_stderr
    );
    assert_eq!(max_json["status"], "VERIFIED");

    let leading_zero_path = temp_anvil_source(
        "hex_leading_zero_u256_max",
        r#"
fn hex_leading_zero_u256_max(mut x: u256) -> u256
    where { x == 0, x' == 0x0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff }
{
    x = 0x0ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff;
    return x;
}
"#,
    );
    let (leading_success, leading_json, leading_stdout, leading_stderr) =
        run_anvil_check_json_path(&leading_zero_path);
    let _ = std::fs::remove_file(&leading_zero_path);

    assert!(
        leading_success,
        "leading-zero 65-nibble hex u256::MAX must canonicalize and verify:\nstdout:\n{}\nstderr:\n{}",
        leading_stdout, leading_stderr
    );
    assert_eq!(leading_json["status"], "VERIFIED");

    let above_path = temp_anvil_source(
        "hex_u256_above_max",
        r#"
fn hex_u256_above_max(mut x: u256) -> u256
    where { x == 0, x' == 0 }
{
    x = 0x10000000000000000000000000000000000000000000000000000000000000000;
    return x;
}
"#,
    );
    let (above_success, above_json, above_stdout, above_stderr) =
        run_anvil_check_json_path(&above_path);
    let _ = std::fs::remove_file(&above_path);

    assert!(
        !above_success,
        "hex literal above u256::MAX must be rejected:\nstdout:\n{}\nstderr:\n{}",
        above_stdout, above_stderr
    );
    assert_eq!(above_json["status"], "PARSE_ERROR");
}

#[test]
fn soundness_signed_lt_uses_signed_comparison() {
    let path = temp_anvil_source(
        "signed_lt",
        r#"
fn signed_lt(mut x: i8) -> i8
    where { x == -1, x < 0, x' == x }
{
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "signed i8 comparison must treat -1 < 0 as true:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
    assert_eq!(json["results"][0]["status"], "VERIFIED");
}

#[test]
fn soundness_signed_not_gt_zero() {
    let path = temp_anvil_source(
        "signed_not_gt_zero",
        r#"
fn signed_not_gt_zero(mut x: i8) -> i8
    where { x == -1, x > 0, x' == x }
{
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "signed i8 comparison must reject -1 > 0:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_no_u8_literal_truncation() {
    let path = temp_anvil_source(
        "no_u8_literal_truncation",
        r#"
fn no_u8_literal_truncation() -> u8
{
    let x: u8 = 256;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "u8 literal overflow must fail typechecking:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}

#[test]
fn soundness_no_negative_to_unsigned() {
    let path = temp_anvil_source(
        "no_negative_to_unsigned",
        r#"
fn no_negative_to_unsigned() -> u8
{
    let x: u8 = -1;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "negative literals must not be assigned to unsigned types:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}

#[test]
fn soundness_no_negative_state_default_to_unsigned() {
    let path = temp_anvil_source(
        "no_negative_state_default",
        r#"
contract C {
    state x: u8 = -1;

    fn keep(mut y: u8) -> u8
        where { y' == y }
    {
        return y;
    }
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "negative state defaults must fail typechecking:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}

#[test]
fn soundness_no_negative_top_level_ghost_to_unsigned() {
    let path = temp_anvil_source(
        "no_negative_top_level_ghost",
        r#"
ghost g: u8 = -1;

fn keep(mut y: u8) -> u8
    where { y' == y }
{
    return y;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "negative top-level ghosts must fail typechecking:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}

#[test]
fn soundness_no_negative_stmt_ghost_to_unsigned() {
    let path = temp_anvil_source(
        "no_negative_stmt_ghost",
        r#"
fn no_negative_stmt_ghost(mut y: u8) -> u8
    where { y' == y }
{
    ghost g: u8 = -1;
    return y;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "negative statement ghosts must fail typechecking:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}

#[test]
fn soundness_i128_min_literal_typechecks() {
    let path = temp_anvil_source(
        "i128_min_literal",
        r#"
fn i128_min_literal() -> i128
    where { true }
{
    let x: i128 = -170141183460469231731687303715884105728;
    return x;
}
"#,
    );
    let (_success, json, _stdout, _stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert_ne!(
        json["status"], "TYPE_ERROR",
        "i128::MIN should typecheck: {}",
        json
    );
}

#[test]
fn soundness_i128_below_min_rejected() {
    let path = temp_anvil_source(
        "i128_below_min",
        r#"
fn i128_below_min() -> i128
    where { true }
{
    let x: i128 = -170141183460469231731687303715884105729;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "below i128::MIN must fail typechecking:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
}

#[test]
fn soundness_signed_positive_literal_allowed() {
    let path = temp_anvil_source(
        "signed_positive_literal",
        r#"
fn signed_positive_literal() -> i8
    where { true }
{
    let x: i8 = 1;
    return x;
}
"#,
    );
    let (_success, json, _stdout, _stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert_ne!(
        json["status"], "TYPE_ERROR",
        "positive literals that fit signed types should typecheck: {}",
        json
    );
}

#[test]
fn soundness_signedness_is_function_local() {
    let path = temp_anvil_source(
        "signedness_local",
        r#"
fn signed_name(mut x: i8) -> i8
    where { x == -1, x' == x }
{
    return x;
}

fn unsigned_name(mut x: u8) -> u8
    where { x == 255, x' == x }
{
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "signed constraints from one function must not poison same-named unsigned params:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
}

#[test]
fn soundness_signed_div_assign_uses_signed_division() {
    let path = temp_anvil_source(
        "signed_div_assign",
        r#"
fn signed_div_assign(mut x: i64, y: i64) -> i64
    where { x == 1, y == -1, x' == -1 }
{
    x /= y;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "signed /= must use signed division in the verifier:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
}

#[test]
fn soundness_signed_huge_literal_rejected_by_verifier() {
    let path = temp_anvil_source(
        "signed_huge_literal",
        r#"
fn signed_huge_literal(mut x: i128) -> i128
    where { x < 115792089237316195423570985008687907853269984665640564039457584007913129639935, x' == x }
{
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "huge unsigned literals in signed comparisons must fail closed:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
}

#[test]
fn soundness_build_u256_backends_fail_closed() {
    let path = temp_anvil_source(
        "build_u256_fail_closed",
        r#"
fn build_u256_fail_closed(mut x: u256) -> u256
    where { x == 0, x' == 18446744073709551616 }
{
    x = 18446744073709551616;
    return x;
}
"#,
    );
    let (rust_success, rust_output) = run_anvil_build_path(&path, "rust");
    let (llvm_success, llvm_output) = run_anvil_build_path(&path, "llvm");
    let _ = std::fs::remove_file(&path);

    assert!(
        !rust_success && rust_output.contains("U256"),
        "Rust backend must reject unsupported u256 explicitly: {}",
        rust_output
    );
    assert!(
        !llvm_success && llvm_output.contains("i256"),
        "LLVM backend must reject unsupported i256 explicitly: {}",
        llvm_output
    );
}

#[test]
fn soundness_std_crypto_parses_full_width_literals() {
    let path = Path::new("src/std/crypto.anv");
    let (success, output) = run_anvil_ast_path(path);

    assert!(success, "std crypto should parse: {}", output);
    assert!(
        output.contains("BigLiteral") && output.contains("AddressLit"),
        "std crypto AST should preserve full-width curve constants and address literals: {}",
        output
    );
}

#[test]
fn soundness_u64_overflow_body_rejected() {
    let path = temp_anvil_source(
        "u64_overflow_body",
        r#"
fn u64_overflow_body(mut x: u64) -> u64
    where { x == 18446744073709551615, x' == 0 }
{
    x += 1;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "u64 overflow must not prove postconditions via inconsistent body constraints:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_u128_does_not_wrap_at_u64_width() {
    let path = temp_anvil_source(
        "u128_no_u64_wrap",
        r#"
fn u128_no_u64_wrap(mut x: u128) -> u128
    where { x == 18446744073709551615, x' == 0 }
{
    x += 1;
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "u128 arithmetic must not wrap at 64 bits:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["results"][0]["status"], "REJECTED");
}

#[test]
fn soundness_fncall_congruence_verified() {
    let path = temp_anvil_source(
        "fncall_congruence",
        r#"
fn fncall_congruence(mut a: u64, mut b: u64) -> u64
    where { a == b, a' == b' }
{
    a = f(a);
    b = f(b);
    return a;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "uninterpreted function congruence must prove f(a) == f(b) when a == b:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
    assert_eq!(json["results"][0]["status"], "VERIFIED");
}

#[test]
fn soundness_loop_invariant_preservation_proves_exit() {
    let path = temp_anvil_source(
        "loop_preservation",
        r#"
fn loop_preservation(mut x: u64) -> u64
    where { x == 0, x' == 1 }
{
    while x < 1 where { x <= 1 } {
        x += 1;
    }
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "established and preserved loop invariants should constrain loop exit:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
    assert_eq!(json["results"][0]["status"], "VERIFIED");
}

#[test]
fn soundness_mixed_unverified_function_rejected() {
    let path = temp_anvil_source(
        "mixed_unverified",
        r#"
fn verified_one(mut x: u64) -> u64
    where { x' == x }
{
    return x;
}

fn unverified_one(mut y: u64) -> u64
{
    y += 1;
    return y;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "mixed verified/unverified files must not get a green aggregate:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "REJECTED");
    assert_eq!(json["functions"], 2);
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
    assert_eq!(json["summary"]["functions_failed"], 1);
}

#[test]
fn soundness_block_let_does_not_escape_scope() {
    let path = temp_anvil_source(
        "block_scope_leak",
        r#"
fn block_scope_leak(x: u64) -> u64
    where { x' == x }
{
    if true {
        let y: u64 = 1;
    }
    return y;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "block-local let bindings must not escape their block:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}

#[test]
fn soundness_shadowing_restores_outer_binding() {
    let path = temp_anvil_source(
        "shadowing_restores_outer",
        r#"
fn shadowing_restores_outer(mut x: u64) -> u64
    where { x' == x }
{
    if true {
        let x: bool = false;
    }
    let z: u64 = x;
    return z;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        success,
        "inner shadowing must not overwrite the outer binding:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "VERIFIED");
}

#[test]
fn soundness_nested_blocks_are_typechecked() {
    let path = temp_anvil_source(
        "nested_typecheck",
        r#"
fn nested_typecheck(x: u64) -> u64
{
    if true {
        let y: u64 = false;
    }
    return x;
}
"#,
    );
    let (success, json, stdout, stderr) = run_anvil_check_json_path(&path);
    let _ = std::fs::remove_file(&path);

    assert!(
        !success,
        "nested type errors must fail checking:\nstdout:\n{}\nstderr:\n{}",
        stdout, stderr
    );
    assert_eq!(json["status"], "TYPE_ERROR");
    assert_eq!(json["errors"][0]["source"], "typechecker");
}
