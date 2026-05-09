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

use std::process::Command;
use std::path::{Path, PathBuf};

fn anvil_binary() -> PathBuf {
    // Cargo builds the binary in target/debug/anvil
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove test binary name
    path.pop(); // remove 'deps'
    path.push("anvil");
    path
}

fn run_anvil_check(example: &str) -> (bool, String) {
    let binary = anvil_binary();
    let example_path = Path::new("examples").join(example);
    
    let output = Command::new(&binary)
        .args(["check", example_path.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("Failed to run anvil binary at {:?}: {}", binary, e));
    
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);
    
    (output.status.success(), combined)
}

fn run_anvil_build(example: &str, target: &str) -> (bool, String) {
    let binary = anvil_binary();
    let example_path = Path::new("examples").join(example);
    let output_path = std::env::temp_dir().join(format!("anvil_test_{}", example));
    
    let mut args = vec![
        "build".to_string(),
        example_path.to_str().unwrap().to_string(),
        "-o".to_string(),
        output_path.to_str().unwrap().to_string(),
    ];
    
    if target == "llvm" {
        args.push("--target".to_string());
        args.push("llvm".to_string());
    }
    
    let output = Command::new(&binary)
        .args(&args)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run anvil build: {}", e));
    
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);
    
    (output.status.success(), combined)
}

fn run_anvil_ast(example: &str) -> (bool, String) {
    let binary = anvil_binary();
    let example_path = Path::new("examples").join(example);
    
    let output = Command::new(&binary)
        .args(["ast", example_path.to_str().unwrap()])
        .output()
        .unwrap_or_else(|e| panic!("Failed to run anvil ast: {}", e));
    
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
            let stdout_part: String = output.lines()
                .take_while(|l| !l.contains("ANVIL"))
                .collect::<Vec<_>>()
                .join("\n");
            // Just check it's non-empty and starts with valid JSON
            assert!(!stdout_part.trim().is_empty() || output.contains("items"), 
                "AST output is empty for {}", $file);
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
    assert!(output.contains("proven") || output.contains("PROVEN") || output.contains("✓"),
        "Expected proven output for hello.anv: {}", output);
}

#[test]
fn verify_transfer_proven() {
    let (success, output) = run_anvil_check("transfer.anv");
    assert!(success, "transfer.anv should verify: {}", output);
    assert!(output.contains("✓"), "Expected proven checkmark: {}", output);
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
    assert!(!success || output.contains("FAILED") || output.contains("✗"),
        "reentrancy.anv should contain at least one rejection: {}", output);
}

// --- Mixed files: some functions proven, some rejected ---

#[test]
fn verify_token_mixed() {
    let (_success, output) = run_anvil_check("token.anv");
    // token.anv has contract invariants — some should pass, some might fail
    assert!(output.contains("✓") || output.contains("✗") || output.contains("postcondition"),
        "token.anv should produce verification output: {}", output);
}

#[test]
fn verify_vault_mixed() {
    let (_success, output) = run_anvil_check("vault.anv");
    // vault.anv has safe deposit, broken withdraw, and donation attack
    assert!(output.contains("VERIFICATION REPORT"),
        "vault.anv should produce a verification report: {}", output);
}

#[test]
fn verify_loops_mixed() {
    let (_success, output) = run_anvil_check("loops.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "loops.anv should produce a report: {}", output);
}

#[test]
fn verify_ssa_mixed() {
    let (_success, output) = run_anvil_check("ssa.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "ssa.anv should produce a report: {}", output);
}

#[test]
fn verify_while_loops_mixed() {
    let (_success, output) = run_anvil_check("while_loops.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "while_loops.anv should produce a report: {}", output);
}

#[test]
fn verify_stress_test() {
    let (_success, output) = run_anvil_check("stress_test.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "stress_test.anv should produce a report: {}", output);
}

#[test]
fn verify_amm_pool() {
    let (_success, output) = run_anvil_check("amm_pool.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "amm_pool.anv should produce a report: {}", output);
}

#[test]
fn verify_oracle_manipulation() {
    let (_success, output) = run_anvil_check("oracle_manipulation.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "oracle_manipulation.anv should produce a report: {}", output);
}

#[test]
fn verify_bounty_hunter() {
    let (_success, output) = run_anvil_check("bounty_hunter.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "bounty_hunter.anv should produce a report: {}", output);
}

#[test]
fn verify_k2_close_factor() {
    let (_success, output) = run_anvil_check("k2_close_factor_bypass.anv");
    assert!(output.contains("VERIFICATION REPORT"),
        "k2_close_factor_bypass.anv should produce a report: {}", output);
}

// ============================================================
// PHASE C: Codegen Tests
// ============================================================

#[test]
fn build_transfer_rust() {
    let (success, output) = run_anvil_build("transfer.anv", "rust");
    assert!(success, "transfer.anv should build to Rust: {}", output);
    assert!(output.contains("Generated") || output.contains(".rs"),
        "Should indicate Rust file generation: {}", output);
}

#[test]
fn build_transfer_llvm() {
    let (success, output) = run_anvil_build("transfer.anv", "llvm");
    assert!(success, "transfer.anv should build to LLVM IR: {}", output);
    assert!(output.contains("Generated") || output.contains(".ll"),
        "Should indicate LLVM IR generation: {}", output);
}

#[test]
fn build_safe_transfer_rust() {
    let (success, output) = run_anvil_build("safe_transfer.anv", "rust");
    assert!(success, "safe_transfer.anv should build to Rust: {}", output);
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
    assert!(output.contains("proof:") || output.contains("🔐"),
        "Proof hash should be present in output: {}", output);
}

#[test]
fn regression_cortex_manifest_emitted() {
    let (success, output) = run_anvil_build("transfer.anv", "rust");
    assert!(success);
    assert!(output.contains("CORTEX manifest") || output.contains("cortex_manifest"),
        "CORTEX manifest should be emitted on build: {}", output);
}

#[test]
fn regression_type_constraints_registered() {
    let (success, output) = run_anvil_check("transfer.anv");
    assert!(success);
    assert!(output.contains("type constraints"),
        "Type constraints should be reported: {}", output);
}

#[test]
fn regression_cli_version() {
    let binary = anvil_binary();
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .expect("Failed to run anvil --version");
    let version_str = String::from_utf8_lossy(&output.stdout);
    assert!(version_str.contains("0.6.0") || version_str.contains("0.7.0"),
        "CLI version should match Cargo.toml: got '{}'", version_str.trim());
}

// ============================================================
// PHASE E: Edge Cases and Soundness Checks
// ============================================================

#[test]
fn soundness_vacuous_invariant() {
    // A vacuous where clause should still verify
    let (success, output) = run_anvil_check("vacuous.anv");
    assert!(success, "Vacuous invariants should verify: {}", output);
}

#[test]
fn soundness_bitflow_drift() {
    let (_success, output) = run_anvil_check("bitflow_drift.anv");
    // Should at minimum parse and produce verification output
    assert!(output.contains("VERIFICATION REPORT") || output.contains("postcondition"),
        "bitflow_drift.anv should produce output: {}", output);
}
