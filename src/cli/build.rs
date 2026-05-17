use colored::Colorize;
use std::path::PathBuf;
use tracing::{info, error, info_span};

use crate::core::{parser, typechecker};
use crate::engine::{verifier, codegen, llvm_ir};

pub fn cmd_build(file: &PathBuf, output: &PathBuf, target: &str) {
    let _span = info_span!("cmd_build", file = %file.display(), target = target).entered();
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            error!(file = %file.display(), error = %e, "Cannot read source file");
            eprintln!("  {} Cannot read {}: {}", "✗".bright_red(), file.display(), e);
            std::process::exit(1);
        }
    };

    eprintln!("  {} Parsing {}...", "→".bright_blue(), file.display());

    let program = match parser::parse_program(&source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("  {} {}", "✗".bright_red(), e);
            std::process::exit(1);
        }
    };

    // Type checking pass
    eprintln!("  {} Type checking...", "→".bright_blue());
    let type_env = typechecker::check_program(&program);
    typechecker::print_type_report(&type_env);

    if !type_env.errors.is_empty() {
        error!(errors = type_env.errors.len(), "Type checking failed");
        eprintln!("  {} Type checking failed.", "✗".bright_red().bold());
        std::process::exit(1);
    }

    eprintln!("  {} Verifying with Z3...", "→".bright_blue());
    let results = verifier::verify_program(&program, &type_env);
    verifier::print_results(&results);

    let all_ok = results.iter().all(|r| r.verified);
    if !all_ok {
        error!("Build aborted: verification failed");
        eprintln!("  {} Cannot build: verification failed. Fix your invariants.",
            "✗".bright_red().bold());
        std::process::exit(1);
    }

    if target == "llvm" {
        eprintln!("  {} Generating LLVM IR...", "→".bright_blue());
        let llvm_ir = llvm_ir::generate_llvm_ir(&program);
        let out_path = output.with_extension("ll");
        match std::fs::write(&out_path, &llvm_ir) {
            Ok(_) => {
                info!(path = %out_path.display(), bytes = llvm_ir.len(), "LLVM IR generated");
                eprintln!("  {} Generated {} ({} bytes)", "✓".bright_green(), out_path.display(), llvm_ir.len());
            }
            Err(e) => {
                error!(path = %out_path.display(), error = %e, "Cannot write LLVM IR");
                eprintln!("  {} Cannot write {}: {}", "✗".bright_red(), out_path.display(), e);
            }
        }
    } else {
        eprintln!("  {} Generating Rust...", "→".bright_blue());
        let rust_code = codegen::generate_rust(&program);
        let out_path = output.with_extension("rs");
        match std::fs::write(&out_path, &rust_code) {
            Ok(_) => {
                info!(path = %out_path.display(), bytes = rust_code.len(), "Rust code generated");
                eprintln!("  {} Generated {} ({} bytes)", "✓".bright_green(), out_path.display(), rust_code.len());
            }
            Err(e) => {
                error!(path = %out_path.display(), error = %e, "Cannot write Rust code");
                eprintln!("  {} Cannot write {}: {}", "✗".bright_red(), out_path.display(), e);
            }
        }
    }

    // ── CORTEX Provenance Manifest ───────────────────────────────────
    emit_cortex_manifest(file, output, &results);
}

/// Emit a CORTEX provenance manifest alongside the compiled output.
///
/// The manifest contains proof_hash, invariant counts, and verification
/// metadata for every verified function. This is the bridge artifact that
/// `cortex-persist/cortex/engine/anvil_bridge.py` consumes.
fn emit_cortex_manifest(
    source_file: &PathBuf,
    output: &PathBuf,
    results: &[verifier::VerifyResult],
) {
    let _span = info_span!("emit_cortex_manifest").entered();
    let manifest_path = output.with_extension("cortex_manifest.json");

    let functions: Vec<serde_json::Value> = results.iter().map(|r| {
        serde_json::json!({
            "fn_name": r.fn_name,
            "verified": r.verified,
            "proof_hash": r.proof_hash,
            "invariants_checked": r.invariants_checked,
            "preconditions_count": r.preconditions_count,
            "postconditions_count": r.postconditions_count,
            "duration_ms": r.duration_ms,
        })
    }).collect();

    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "anvil_version": env!("CARGO_PKG_VERSION"),
        "source_file": source_file.to_string_lossy(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "verifier": "anvil-z3",
        "fact_type": "anvil_verified_execution",
        "functions": functions,
        "total_proven": results.iter().filter(|r| r.verified).count(),
        "total_failed": results.iter().filter(|r| !r.verified).count(),
    });

    match std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()) {
        Ok(_) => {
            info!(path = %manifest_path.display(), proofs = results.len(), "CORTEX manifest emitted");
            eprintln!("  {} CORTEX manifest: {} ({} proofs)",
                "🔐".to_string().as_str(),
                manifest_path.display(),
                results.len(),
            );
        }
        Err(e) => {
            error!(path = %manifest_path.display(), error = %e, "Cannot write CORTEX manifest");
            eprintln!("  {} Cannot write manifest {}: {}",
                "⚠".bright_yellow(), manifest_path.display(), e);
        }
    }
}
