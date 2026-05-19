use colored::Colorize;
use serde::Serialize;
use sha3::{Digest, Sha3_256};
use std::path::Path;
use std::time::Instant;
use tracing::{error, info, info_span};

use crate::core::{ast, parser, typechecker};
use crate::engine::verifier;

const CHECK_JSON_SCHEMA_VERSION: &str = "anvil.check.v1";
const CHECK_JSON_KIND: &str = "check";
const PROOF_HASH_ALGORITHM: &str = "sha3-256";

#[derive(Serialize)]
struct CheckJsonReport {
    schema_version: &'static str,
    anvil_version: &'static str,
    kind: &'static str,
    status: &'static str,
    ok: bool,
    message: String,
    error: Option<String>,
    detail: Option<String>,
    file: String,
    timeout_ms: u64,
    functions: usize,
    invariants: usize,
    all_verified: bool,
    proof_hash: Option<String>,
    duration_ms: f64,
    durations: CheckJsonDurations,
    summary: CheckJsonSummary,
    proof: CheckJsonProof,
    errors: Vec<CheckJsonDiagnostic>,
    type_errors: Vec<CheckJsonDiagnostic>,
    warnings: Vec<CheckJsonDiagnostic>,
    counterexamples: Vec<CheckJsonCounterexample>,
    results: Vec<CheckJsonResult>,
}

#[derive(Serialize)]
struct CheckJsonDurations {
    parse_ms: f64,
    typecheck_ms: f64,
    verification_ms: f64,
    total_ms: f64,
}

#[derive(Serialize)]
struct CheckJsonSummary {
    functions_total: usize,
    functions_verified: usize,
    functions_failed: usize,
    invariants_total: usize,
    preconditions_total: usize,
    postconditions_total: usize,
    type_constraints_total: usize,
    errors_total: usize,
    warnings_total: usize,
    counterexamples_total: usize,
}

#[derive(Serialize)]
struct CheckJsonProof {
    hash_algorithm: &'static str,
    aggregate_hash: Option<String>,
    function_hashes: Vec<CheckJsonFunctionProof>,
}

#[derive(Serialize)]
struct CheckJsonFunctionProof {
    fn_name: String,
    status: &'static str,
    proof_hash: String,
}

#[derive(Clone, Serialize)]
struct CheckJsonDiagnostic {
    source: &'static str,
    location: Option<String>,
    message: String,
    detail: Option<String>,
}

#[derive(Clone, Serialize)]
struct CheckJsonCounterexample {
    fn_name: String,
    kind: &'static str,
    text: String,
    lines: Vec<String>,
}

#[derive(Serialize)]
struct CheckJsonResult {
    fn_name: String,
    status: &'static str,
    verified: bool,
    invariants: usize,
    invariants_checked: usize,
    preconditions: usize,
    preconditions_count: usize,
    postconditions: usize,
    postconditions_count: usize,
    proof_hash: String,
    duration_ms: f64,
    counterexample: Option<String>,
    counterexamples: Vec<CheckJsonCounterexample>,
    warnings: Vec<String>,
}

pub fn cmd_check(file: &Path, json_output: bool, timeout_ms: u64) {
    let _span = info_span!("cmd_check", file = %file.display(), timeout_ms).entered();
    let total_start = Instant::now();
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            if json_output {
                print_json_report(&error_report(
                    file,
                    timeout_ms,
                    "IO_ERROR",
                    "Cannot read source file",
                    vec![CheckJsonDiagnostic {
                        source: "io",
                        location: Some(file.display().to_string()),
                        message: format!("Cannot read {}", file.display()),
                        detail: Some(e.to_string()),
                    }],
                    total_start.elapsed(),
                ));
            } else {
                error!(file = %file.display(), error = %e, "Cannot read source file");
                eprintln!(
                    "  {} Cannot read {}: {}",
                    "✗".bright_red(),
                    file.display(),
                    e
                );
            }
            std::process::exit(1);
        }
    };

    if !json_output {
        eprintln!("  {} Parsing {}...", "→".bright_blue(), file.display());
    }

    let parse_start = Instant::now();
    let program = match parser::parse_program(&source) {
        Ok(p) => p,
        Err(e) => {
            let parse_ms = elapsed_ms(parse_start);
            if json_output {
                print_json_report(&error_report_with_durations(
                    file,
                    timeout_ms,
                    "PARSE_ERROR",
                    "Parse error",
                    vec![CheckJsonDiagnostic {
                        source: "parser",
                        location: None,
                        message: "Parse error".to_string(),
                        detail: Some(e),
                    }],
                    CheckJsonDurations {
                        parse_ms,
                        typecheck_ms: 0.0,
                        verification_ms: 0.0,
                        total_ms: elapsed_ms(total_start),
                    },
                ));
            } else {
                eprintln!("  {} {}", "✗".bright_red(), e);
            }
            std::process::exit(1);
        }
    };
    let parse_ms = elapsed_ms(parse_start);

    let (fn_count, inv_count) = program_stats(&program);

    if !json_output {
        info!(
            functions = fn_count,
            invariants = inv_count,
            "Parse complete"
        );
        eprintln!(
            "  {} Parsed: {} functions, {} invariants",
            "✓".bright_green(),
            fn_count,
            inv_count
        );
    }

    // Type checking pass — bridge mathematical abstraction to silicon bounds
    if !json_output {
        eprintln!("  {} Type checking...", "→".bright_blue());
    }
    let typecheck_start = Instant::now();
    let type_env = typechecker::check_program(&program);
    let typecheck_ms = elapsed_ms(typecheck_start);
    if !json_output {
        typechecker::print_type_report(&type_env);
    }

    if !type_env.errors.is_empty() {
        if json_output {
            let errors = type_env
                .errors
                .iter()
                .map(|err| CheckJsonDiagnostic {
                    source: "typechecker",
                    location: Some(err.location.clone()),
                    message: err.message.clone(),
                    detail: None,
                })
                .collect();
            let warnings = type_warnings_json(&type_env);
            print_json_report(&build_report(BuildReportInput {
                file,
                timeout_ms,
                status: "TYPE_ERROR",
                ok: false,
                message: "Type checking failed",
                error: Some("Type checking failed".to_string()),
                fn_count,
                inv_count,
                type_constraints_total: type_env.constraints.len(),
                durations: CheckJsonDurations {
                    parse_ms,
                    typecheck_ms,
                    verification_ms: 0.0,
                    total_ms: elapsed_ms(total_start),
                },
                errors,
                warnings,
                results: &[],
            }));
        } else {
            error!(errors = type_env.errors.len(), "Type checking failed");
            eprintln!(
                "  {} Type checking failed. Fix type errors before verification.",
                "✗".bright_red().bold()
            );
        }
        std::process::exit(1);
    }

    if !json_output {
        eprintln!(
            "  {} Verifying with Z3 (timeout {}ms)...",
            "→".bright_blue(),
            timeout_ms
        );
    }

    let verification_start = Instant::now();
    let results = verifier::verify_program_with_options(
        &program,
        &type_env,
        verifier::VerifyOptions { timeout_ms },
    );
    let verification_ms = elapsed_ms(verification_start);

    if json_output {
        let all_ok = verification_succeeded(&results, fn_count);
        let status = aggregate_status(&results, all_ok);
        print_json_report(&build_report(BuildReportInput {
            file,
            timeout_ms,
            status,
            ok: all_ok,
            message: if all_ok {
                "All functions verified"
            } else if results.is_empty() {
                "No verification obligations found"
            } else if results.len() < fn_count {
                "Some functions have no verification obligations"
            } else {
                "Verification failed"
            },
            error: None,
            fn_count,
            inv_count,
            type_constraints_total: type_env.constraints.len(),
            durations: CheckJsonDurations {
                parse_ms,
                typecheck_ms,
                verification_ms,
                total_ms: elapsed_ms(total_start),
            },
            errors: Vec::new(),
            warnings: type_warnings_json(&type_env),
            results: &results,
        }));
    } else {
        verifier::print_results(&results);
    }

    let all_ok = verification_succeeded(&results, fn_count);
    if all_ok {
        info!(postconditions = results.len(), "All postconditions proven");
    } else if results.is_empty() {
        error!("No verification obligations found");
        if !json_output {
            eprintln!(
                "  {} No verification obligations found. Add invariants before claiming verification.",
                "✗".bright_red().bold()
            );
        }
    } else if results.len() < fn_count {
        error!("Some functions have no verification obligations");
        if !json_output {
            eprintln!(
                "  {} Some functions have no verification obligations. Add invariants to every function before claiming verification.",
                "✗".bright_red().bold()
            );
        }
    } else {
        error!("Verification failed");
    }
    if !all_ok {
        std::process::exit(1);
    }
}

fn print_json_report(report: &CheckJsonReport) {
    println!("{}", serde_json::to_string(report).unwrap());
}

fn error_report(
    file: &Path,
    timeout_ms: u64,
    status: &'static str,
    message: &str,
    errors: Vec<CheckJsonDiagnostic>,
    total_duration: std::time::Duration,
) -> CheckJsonReport {
    error_report_with_durations(
        file,
        timeout_ms,
        status,
        message,
        errors,
        CheckJsonDurations {
            parse_ms: 0.0,
            typecheck_ms: 0.0,
            verification_ms: 0.0,
            total_ms: round_ms(total_duration.as_secs_f64() * 1000.0),
        },
    )
}

fn error_report_with_durations(
    file: &Path,
    timeout_ms: u64,
    status: &'static str,
    message: &str,
    errors: Vec<CheckJsonDiagnostic>,
    durations: CheckJsonDurations,
) -> CheckJsonReport {
    build_report(BuildReportInput {
        file,
        timeout_ms,
        status,
        ok: false,
        message,
        error: Some(message.to_string()),
        fn_count: 0,
        inv_count: 0,
        type_constraints_total: 0,
        durations,
        errors,
        warnings: Vec::new(),
        results: &[],
    })
}

struct BuildReportInput<'a> {
    file: &'a Path,
    timeout_ms: u64,
    status: &'static str,
    ok: bool,
    message: &'a str,
    error: Option<String>,
    fn_count: usize,
    inv_count: usize,
    type_constraints_total: usize,
    durations: CheckJsonDurations,
    errors: Vec<CheckJsonDiagnostic>,
    warnings: Vec<CheckJsonDiagnostic>,
    results: &'a [verifier::VerifyResult],
}

fn build_report(input: BuildReportInput<'_>) -> CheckJsonReport {
    let BuildReportInput {
        file,
        timeout_ms,
        status,
        ok,
        message,
        error,
        fn_count,
        inv_count,
        type_constraints_total,
        durations,
        errors,
        mut warnings,
        results,
    } = input;
    let mut counterexamples = Vec::new();
    let mut json_results = Vec::new();

    for result in results {
        let result_status = verify_result_status(result);
        let result_counterexamples = result
            .counterexample
            .as_deref()
            .map(|text| {
                vec![CheckJsonCounterexample {
                    fn_name: result.fn_name.clone(),
                    kind: counterexample_kind(text),
                    text: text.to_string(),
                    lines: text.lines().map(str::to_string).collect(),
                }]
            })
            .unwrap_or_default();
        counterexamples.extend(result_counterexamples.iter().cloned());

        for warning in &result.warnings {
            warnings.push(CheckJsonDiagnostic {
                source: "verifier",
                location: Some(result.fn_name.clone()),
                message: warning.clone(),
                detail: None,
            });
        }

        json_results.push(CheckJsonResult {
            fn_name: result.fn_name.clone(),
            status: result_status,
            verified: result.verified,
            invariants: result.invariants_checked,
            invariants_checked: result.invariants_checked,
            preconditions: result.preconditions_count,
            preconditions_count: result.preconditions_count,
            postconditions: result.postconditions_count,
            postconditions_count: result.postconditions_count,
            proof_hash: result.proof_hash.clone(),
            duration_ms: round_ms(result.duration_ms),
            counterexample: result.counterexample.clone(),
            counterexamples: result_counterexamples,
            warnings: result.warnings.clone(),
        });
    }

    let function_hashes: Vec<CheckJsonFunctionProof> = results
        .iter()
        .map(|result| CheckJsonFunctionProof {
            fn_name: result.fn_name.clone(),
            status: verify_result_status(result),
            proof_hash: result.proof_hash.clone(),
        })
        .collect();
    let aggregate_hash = aggregate_proof_hash(results);
    let functions_verified = results.iter().filter(|result| result.verified).count();
    let functions_failed = if !ok {
        fn_count.saturating_sub(functions_verified)
    } else {
        results.len().saturating_sub(functions_verified)
    };
    let preconditions_total = results
        .iter()
        .map(|result| result.preconditions_count)
        .sum();
    let postconditions_total = results
        .iter()
        .map(|result| result.postconditions_count)
        .sum();
    let detail = errors
        .iter()
        .find_map(|diagnostic| diagnostic.detail.clone())
        .or_else(|| error.clone());
    let type_errors = if status == "TYPE_ERROR" {
        errors.clone()
    } else {
        Vec::new()
    };

    CheckJsonReport {
        schema_version: CHECK_JSON_SCHEMA_VERSION,
        anvil_version: env!("CARGO_PKG_VERSION"),
        kind: CHECK_JSON_KIND,
        status,
        ok,
        message: message.to_string(),
        error,
        detail,
        file: file.display().to_string(),
        timeout_ms,
        functions: fn_count,
        invariants: inv_count,
        all_verified: ok,
        proof_hash: aggregate_hash.clone(),
        duration_ms: durations.total_ms,
        durations,
        summary: CheckJsonSummary {
            functions_total: fn_count,
            functions_verified,
            functions_failed,
            invariants_total: inv_count,
            preconditions_total,
            postconditions_total,
            type_constraints_total,
            errors_total: errors.len(),
            warnings_total: warnings.len(),
            counterexamples_total: counterexamples.len(),
        },
        proof: CheckJsonProof {
            hash_algorithm: PROOF_HASH_ALGORITHM,
            aggregate_hash,
            function_hashes,
        },
        errors,
        type_errors,
        warnings,
        counterexamples,
        results: json_results,
    }
}

fn type_warnings_json(type_env: &typechecker::TypeEnv) -> Vec<CheckJsonDiagnostic> {
    type_env
        .warnings
        .iter()
        .map(|warning| CheckJsonDiagnostic {
            source: "typechecker",
            location: Some(warning.location.clone()),
            message: warning.message.clone(),
            detail: None,
        })
        .collect()
}

fn aggregate_status(results: &[verifier::VerifyResult], all_ok: bool) -> &'static str {
    if all_ok {
        "VERIFIED"
    } else if results.iter().any(|result| {
        !result.verified
            && !result
                .counterexample
                .as_deref()
                .is_some_and(is_solver_resource_exhaustion)
    }) {
        "REJECTED"
    } else if results.iter().any(|result| {
        result
            .counterexample
            .as_deref()
            .is_some_and(is_solver_resource_exhaustion)
    }) {
        "Z3_RESOURCE_EXHAUSTED"
    } else {
        "REJECTED"
    }
}

fn verification_succeeded(results: &[verifier::VerifyResult], fn_count: usize) -> bool {
    fn_count > 0 && results.len() == fn_count && results.iter().all(|result| result.verified)
}

fn verify_result_status(result: &verifier::VerifyResult) -> &'static str {
    if result.verified {
        "VERIFIED"
    } else if result
        .counterexample
        .as_deref()
        .is_some_and(is_solver_resource_exhaustion)
    {
        "Z3_RESOURCE_EXHAUSTED"
    } else {
        "REJECTED"
    }
}

fn counterexample_kind(text: &str) -> &'static str {
    if is_solver_resource_exhaustion(text) {
        "solver_unknown"
    } else if text.contains("GLOBAL INVARIANT VIOLATED") {
        "global_invariant"
    } else if text.contains("Contract invariant") {
        "contract_invariant"
    } else if text.contains("Postcondition") {
        "postcondition"
    } else {
        "verification"
    }
}

fn is_solver_resource_exhaustion(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("z3 undecidable")
        || lower.contains("z3 unknown")
        || lower.contains("solver unknown")
        || lower.contains("is undecidable")
        || lower.contains("out of memory")
        || lower.contains("cannot allocate memory")
        || lower.contains("memory allocation")
}

fn program_stats(program: &ast::Program) -> (usize, usize) {
    program
        .items
        .iter()
        .fold((0, 0), |(functions, invariants), item| match item {
            ast::Item::Function(function) => {
                (functions + 1, invariants + function.invariants.len())
            }
            ast::Item::Contract(contract) => {
                let function_invariants: usize = contract
                    .functions
                    .iter()
                    .map(|function| function.invariants.len())
                    .sum();
                (
                    functions + contract.functions.len(),
                    invariants + contract.invariants.len() + function_invariants,
                )
            }
            _ => (functions, invariants),
        })
}

fn aggregate_proof_hash(results: &[verifier::VerifyResult]) -> Option<String> {
    if results.is_empty() {
        return None;
    }

    let mut hasher = Sha3_256::new();
    for result in results {
        hasher.update(result.fn_name.as_bytes());
        hasher.update([0]);
        hasher.update(verify_result_status(result).as_bytes());
        hasher.update([0]);
        hasher.update(result.proof_hash.as_bytes());
        hasher.update([0xff]);
    }
    Some(hex::encode(hasher.finalize()))
}

fn elapsed_ms(start: Instant) -> f64 {
    round_ms(start.elapsed().as_secs_f64() * 1000.0)
}

fn round_ms(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solver_unknown_result() -> verifier::VerifyResult {
        verifier::VerifyResult {
            fn_name: "slow_proof".to_string(),
            invariants_checked: 1,
            preconditions_count: 0,
            postconditions_count: 1,
            verified: false,
            counterexample: Some("Postcondition #1: Z3 undecidable".to_string()),
            duration_ms: 1.0,
            proof_hash: "timeout-proof-hash".to_string(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn solver_unknown_maps_to_resource_exhausted() {
        let result = solver_unknown_result();
        assert_eq!(verify_result_status(&result), "Z3_RESOURCE_EXHAUSTED");
        assert_eq!(aggregate_status(&[result], false), "Z3_RESOURCE_EXHAUSTED");
    }

    #[test]
    fn mixed_concrete_failure_beats_solver_unknown() {
        let timeout = solver_unknown_result();
        let concrete = verifier::VerifyResult {
            fn_name: "bad_proof".to_string(),
            invariants_checked: 1,
            preconditions_count: 0,
            postconditions_count: 1,
            verified: false,
            counterexample: Some("Postcondition #1 failed".to_string()),
            duration_ms: 1.0,
            proof_hash: "bad-proof-hash".to_string(),
            warnings: Vec::new(),
        };

        assert_eq!(aggregate_status(&[timeout, concrete], false), "REJECTED");
    }

    #[test]
    fn undecidable_without_z3_prefix_is_resource_exhausted() {
        let mut result = solver_unknown_result();
        result.counterexample = Some("Loop invariant #1 establishment is undecidable".to_string());

        assert_eq!(verify_result_status(&result), "Z3_RESOURCE_EXHAUSTED");
        assert_eq!(
            counterexample_kind(result.counterexample.as_ref().unwrap()),
            "solver_unknown"
        );
    }

    #[test]
    fn json_report_keeps_legacy_root_error_fields() {
        let report = build_report(BuildReportInput {
            file: std::path::Path::new("bad.anv"),
            timeout_ms: 1_000,
            status: "TYPE_ERROR",
            ok: false,
            message: "Type checking failed",
            error: Some("Type checking failed".to_string()),
            fn_count: 1,
            inv_count: 0,
            type_constraints_total: 0,
            durations: CheckJsonDurations {
                parse_ms: 1.0,
                typecheck_ms: 2.0,
                verification_ms: 0.0,
                total_ms: 3.0,
            },
            errors: vec![CheckJsonDiagnostic {
                source: "typechecker",
                location: Some("bad.anv:1".to_string()),
                message: "expected bool".to_string(),
                detail: Some("found u64".to_string()),
            }],
            warnings: Vec::new(),
            results: &[],
        });

        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["schema_version"], CHECK_JSON_SCHEMA_VERSION);
        assert_eq!(value["detail"], "found u64");
        assert_eq!(value["errors"].as_array().unwrap().len(), 1);
        assert_eq!(value["type_errors"].as_array().unwrap().len(), 1);
        assert_eq!(value["type_errors"][0]["message"], "expected bool");
    }
}
