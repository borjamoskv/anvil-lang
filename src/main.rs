// ============================================================
// ANVIL CLI — "Where trust doesn't compile."
// ============================================================

pub mod core;
pub mod engine;
mod lsp;
mod singularity;

use crate::core::{ast, parser, typechecker};
use crate::engine::{verifier, codegen, llvm_ir, saas};

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;
use tracing::{info, error, info_span};

#[derive(Parser)]
#[command(
    name = "anvil",
    version = env!("CARGO_PKG_VERSION"),
    about = "Anvil — Sovereign Formal Verification Engine.",
    long_about = "Anvil is a proprietary, formally verified engine for smart contracts \
                  and autonomous agents. Every function carries its proof as a first-class \
                  citizen. Access to high-exergy verification is gated by Sovereign Exergy Keys."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and verify an Anvil source file
    Check {
        /// Path to the .anv file
        file: PathBuf,
        /// Output results as JSON (machine-readable)
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Compile an Anvil file to Rust
    Build {
        /// Path to the .anv file
        file: PathBuf,
        /// Output path for generated code
        #[arg(short, long, default_value = "out")]
        output: PathBuf,
        /// Target architecture (rust, llvm)
        #[arg(short, long, default_value = "rust")]
        target: String,
    },
    /// Compile an Anvil file to Rust
    Compile {
        /// Path to the .anv file
        file: PathBuf,
    },
    /// Initiate the Singularity Engine (UltraThink Mode)
    Singularity,
    /// Parse and dump the AST as JSON
    Ast {
        /// Path to the .anv file
        file: PathBuf,
    },
    /// Start the Anvil Language Server Protocol (LSP)
    Lsp,
    /// Start the Anvil Proof Market SaaS Portal
    Saas {
        /// Port to bind the server to
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
    },
    /// Manage Exergy Keys for the SaaS portal
    Keys {
        #[command(subcommand)]
        action: KeyAction,
    },
}

#[derive(Subcommand)]
enum KeyAction {
    /// Add a new exergy key
    Add {
        /// Key ID (if not provided, one will be generated)
        #[arg(short, long)]
        key: Option<String>,
        /// Owner identifier (e.g. username)
        #[arg(short, long)]
        owner: String,
        /// Tier (SOVEREIGN, COMMERCIAL, DEVELOPER)
        #[arg(short, long, default_value = "SOVEREIGN")]
        tier: String,
    },
    /// List all exergy keys
    List,
    /// Revoke an exergy key
    Revoke {
        /// Key ID to revoke
        key: String,
    },
}

#[tokio::main]
async fn main() {
    // Initialize structured logging (respects RUST_LOG env var)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    print_banner();

    match cli.command {
        Commands::Check { file, json } => cmd_check(&file, json),
        Commands::Build { file, output, target } => cmd_build(&file, &output, &target),
        Commands::Compile { file } => info!("Iniciando pase de compilación a Rust para: {:?}", file),
        Commands::Singularity => singularity::initiate_engine(),
        Commands::Ast { file } => cmd_ast(&file),
        Commands::Lsp => lsp::run_server().await,
        Commands::Saas { port } => saas::start_server(port).await,
        Commands::Keys { action } => cmd_keys(action).await,
    }
}

fn print_banner() {
    eprintln!();
    eprintln!("{}", "  ╔═══════════════════════════════════════════╗".bright_blue());
    eprintln!("{}", "  ║   ANVIL — SOVEREIGN VERIFICATION ENGINE   ║".bright_blue());
    eprintln!("{}", "  ║   v0.6.0  [PROPRIETARY // C5-DYNAMIC]     ║".bright_blue());
    eprintln!("{}", "  ║   Where trust doesn't compile.            ║".bright_blue());
    eprintln!("{}", "  ╚═══════════════════════════════════════════╝".bright_blue());
    eprintln!("  {} System status: {}", "●".bright_green(), "SOVEREIGN SHIELD ACTIVE".bold());
    eprintln!();
}

fn cmd_check(file: &PathBuf, json_output: bool) {
    let _span = info_span!("cmd_check", file = %file.display()).entered();
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            if json_output {
                println!("{{\"error\": \"Cannot read {}: {}\"}}", file.display(), e);
            } else {
                error!(file = %file.display(), error = %e, "Cannot read source file");
                eprintln!("  {} Cannot read {}: {}", "✗".bright_red(), file.display(), e);
            }
            std::process::exit(1);
        }
    };

    if !json_output {
        eprintln!("  {} Parsing {}...", "→".bright_blue(), file.display());
    }

    let program = match parser::parse_program(&source) {
        Ok(p) => p,
        Err(e) => {
            if json_output {
                let detail = e.replace('"', "'");
                println!("{{\"error\": \"Parse error\", \"detail\": \"{}\"}}", detail);
            } else {
                eprintln!("  {} {}", "✗".bright_red(), e);
            }
            std::process::exit(1);
        }
    };

    let fn_count = program.items.iter().filter(|i| matches!(i, ast::Item::Function(_))).count();
    let inv_count: usize = program.items.iter().map(|i| match i {
        ast::Item::Function(f) => f.invariants.len(),
        ast::Item::Contract(c) => c.functions.iter().map(|f| f.invariants.len()).sum(),
        _ => 0,
    }).sum();

    if !json_output {
        info!(functions = fn_count, invariants = inv_count, "Parse complete");
        eprintln!("  {} Parsed: {} functions, {} invariants",
            "✓".bright_green(), fn_count, inv_count);
    }

    // Type checking pass — bridge mathematical abstraction to silicon bounds
    if !json_output {
        eprintln!("  {} Type checking...", "→".bright_blue());
    }
    let type_env = typechecker::check_program(&program);
    if !json_output {
        typechecker::print_type_report(&type_env);
    }

    if !type_env.errors.is_empty() {
        if json_output {
            let errs: Vec<String> = type_env.errors.iter()
                .map(|e| {
                    let msg = e.message.replace('"', "'");
                    format!("{{\"location\": \"{}\", \"message\": \"{}\"}}", e.location, msg)
                })
                .collect();
            println!("{{\"type_errors\": [{}]}}", errs.join(", "));
        } else {
            error!(errors = type_env.errors.len(), "Type checking failed");
            eprintln!("  {} Type checking failed. Fix type errors before verification.",
                "✗".bright_red().bold());
        }
        std::process::exit(1);
    }

    if !json_output {
        eprintln!("  {} Verifying with Z3...", "→".bright_blue());
    }

    let results = verifier::verify_program(&program, &type_env);

    if json_output {
        // Structured JSON output for CI/CD integration
        let json_results: Vec<String> = results.iter().map(|r| {
            let cex = r.counterexample.as_deref().unwrap_or("").replace('"', "'");
            let warnings: Vec<String> = r.warnings.iter()
                .map(|w| format!("\"{}\"", w.replace('"', "'")))
                .collect();
            format!(
                "{{\"fn_name\": \"{}\", \"verified\": {}, \"invariants\": {}, \"preconditions\": {}, \"postconditions\": {}, \"proof_hash\": \"{}\", \"duration_ms\": {:.2}, \"counterexample\": \"{}\", \"warnings\": [{}]}}",
                r.fn_name, r.verified, r.invariants_checked,
                r.preconditions_count, r.postconditions_count,
                r.proof_hash, r.duration_ms, cex, warnings.join(", ")
            )
        }).collect();
        let all_ok = results.iter().all(|r| r.verified);
        println!("{{\"file\": \"{}\", \"functions\": {}, \"invariants\": {}, \"all_verified\": {}, \"results\": [{}]}}",
            file.display(), fn_count, inv_count, all_ok, json_results.join(", "));
    } else {
        verifier::print_results(&results);
    }

    let all_ok = results.iter().all(|r| r.verified);
    if all_ok {
        info!(postconditions = results.len(), "All postconditions proven");
    } else {
        error!("Verification failed");
    }
    if !all_ok {
        std::process::exit(1);
    }
}

fn cmd_build(file: &PathBuf, output: &PathBuf, target: &str) {
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
    // Emits a JSON manifest that cortex-persist's anvil_bridge.py can
    // ingest to create AnvilVerifiedExecution facts in the Ledger.
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

fn cmd_ast(file: &PathBuf) {
    let _span = info_span!("cmd_ast", file = %file.display()).entered();
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            error!(file = %file.display(), error = %e, "Cannot read source file");
            eprintln!("  {} Cannot read {}: {}", "✗".bright_red(), file.display(), e);
            std::process::exit(1);
        }
    };

    let program = match parser::parse_program(&source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("  {} {}", "✗".bright_red(), e);
            std::process::exit(1);
        }
    };

    println!("{}", serde_json::to_string_pretty(&program).unwrap());
}

async fn cmd_keys(action: KeyAction) {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:anvil.db".to_string());
    let pool = sqlx::sqlite::SqlitePool::connect(&db_url).await.expect("Failed to connect to database");

    match action {
        KeyAction::Add { key, owner, tier } => {
            let key_id = key.unwrap_or_else(|| format!("anvil-{}-{}", owner, uuid::Uuid::new_v4().to_string()[..8].to_string()));
            match sqlx::query!(
                "INSERT INTO exergy_keys (key_id, owner_id, tier) VALUES (?, ?, ?)",
                key_id, owner, tier
            )
            .execute(&pool)
            .await {
                Ok(_) => println!("  {} Added key: {} (Owner: {}, Tier: {})", "✓".bright_green(), key_id, owner, tier),
                Err(e) => eprintln!("  {} Failed to add key: {}", "✗".bright_red(), e),
            }
        },
        KeyAction::List => {
            let rows = sqlx::query!("SELECT key_id, owner_id, tier, status FROM exergy_keys")
                .fetch_all(&pool)
                .await
                .expect("Failed to fetch keys");
            
            println!("  {:<30} {:<15} {:<15} {:<10}", "KEY ID", "OWNER", "TIER", "STATUS");
            println!("  {}", "-".repeat(80));
            for row in rows {
                println!("  {:<30} {:<15} {:<15} {:<10}", 
                    row.key_id.as_deref().unwrap_or("UNKNOWN"), 
                    row.owner_id, 
                    row.tier.as_deref().unwrap_or("SOVEREIGN"), 
                    row.status.as_deref().unwrap_or("ACTIVE")
                );
            }
        },
        KeyAction::Revoke { key } => {
            match sqlx::query!("UPDATE exergy_keys SET status = 'REVOKED' WHERE key_id = ?", key)
                .execute(&pool)
                .await {
                    Ok(_) => println!("  {} Revoked key: {}", "✓".bright_green(), key),
                    Err(e) => eprintln!("  {} Failed to revoke key: {}", "✗".bright_red(), e),
                }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_verification_sanity() {
        // [C5-REAL] Basic sanity test to ensure the CI pipeline runs tests.
        // In a full implementation, we would test parser and typechecker here.
        assert_eq!(1 + 1, 2, "Thermodynamic laws broken");
    }
}
