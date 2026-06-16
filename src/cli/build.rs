use colored::Colorize;
use std::path::Path;
use tracing::{error, info, info_span};

use crate::core::ast::{Block, Expr, Item, Program, Stmt, Type};
use crate::core::{parser, typechecker};
use crate::engine::{codegen, llvm_ir, verifier};

pub fn cmd_build(file: &Path, output: &Path, target: &str, timeout_ms: u64) {
    let _span =
        info_span!("cmd_build", file = %file.display(), target = target, timeout_ms).entered();
    if !matches!(target, "rust" | "llvm" | "silicon") {
        error!(target, "Unsupported build target");
        eprintln!(
            "  {} Unsupported build target '{}'. Use 'rust', 'llvm', or 'silicon'.",
            "✗".bright_red(),
            target
        );
        std::process::exit(1);
    }

    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            error!(file = %file.display(), error = %e, "Cannot read source file");
            eprintln!(
                "  {} Cannot read {}: {}",
                "✗".bright_red(),
                file.display(),
                e
            );
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
    let function_count = program_function_count(&program);

    // Type checking pass
    eprintln!("  {} Type checking...", "→".bright_blue());
    let type_env = typechecker::check_program(&program);
    typechecker::print_type_report(&type_env);

    if !type_env.errors.is_empty() {
        error!(errors = type_env.errors.len(), "Type checking failed");
        eprintln!("  {} Type checking failed.", "✗".bright_red().bold());
        std::process::exit(1);
    }

    eprintln!(
        "  {} Verifying with Z3 (timeout {}ms)...",
        "→".bright_blue(),
        timeout_ms
    );
    let results = verifier::verify_program_with_options(
        &program,
        &type_env,
        verifier::VerifyOptions { timeout_ms },
    );
    verifier::print_results(&results);

    let all_ok =
        function_count > 0 && results.len() == function_count && results.iter().all(|r| r.verified);
    if !all_ok {
        if results.is_empty() {
            error!("Build aborted: no verification obligations found");
            eprintln!(
                "  {} Cannot build: no verification obligations found. Add invariants before building verified artifacts.",
                "✗".bright_red().bold()
            );
        } else if results.len() < function_count {
            error!(
                verified_functions = results.len(),
                total_functions = function_count,
                "Build aborted: some functions have no verification obligations"
            );
            eprintln!(
                "  {} Cannot build: some functions have no verification obligations. Add invariants to every function before building verified artifacts.",
                "✗".bright_red().bold()
            );
        } else {
            error!("Build aborted: verification failed");
            eprintln!(
                "  {} Cannot build: verification failed. Fix your invariants.",
                "✗".bright_red().bold()
            );
        }
        std::process::exit(1);
    }

    if let Some(reason) = unsupported_build_feature(&program, target) {
        error!(target, reason, "Build aborted: backend feature unsupported");
        eprintln!(
            "  {} Cannot build {} artifact: {}",
            "✗".bright_red().bold(),
            target,
            reason
        );
        std::process::exit(1);
    }

    if target == "llvm" {
        eprintln!("  {} Generating LLVM IR...", "→".bright_blue());
        let llvm_ir = llvm_ir::generate_llvm_ir(&program);
        let out_path = output.with_extension("ll");
        match std::fs::write(&out_path, &llvm_ir) {
            Ok(_) => {
                info!(path = %out_path.display(), bytes = llvm_ir.len(), "LLVM IR generated");
                eprintln!(
                    "  {} Generated {} ({} bytes)",
                    "✓".bright_green(),
                    out_path.display(),
                    llvm_ir.len()
                );
            }
            Err(e) => {
                error!(path = %out_path.display(), error = %e, "Cannot write LLVM IR");
                eprintln!(
                    "  {} Cannot write {}: {}",
                    "✗".bright_red(),
                    out_path.display(),
                    e
                );
                std::process::exit(1);
            }
        }
    } else if target == "silicon" {
        eprintln!(
            "  {} Generating RTL Verilog & ASIC Bitstream...",
            "→".bright_blue()
        );
        let compiler = crate::singularity::DirectSiliconCompiler::default();
        match compiler.compile_to_bitstream(&program) {
            Ok(bitstream) => {
                let out_path = output.with_extension("bin");
                match std::fs::write(&out_path, &bitstream) {
                    Ok(_) => {
                        info!(path = %out_path.display(), bytes = bitstream.len(), "Direct silicon bitstream generated");
                        eprintln!(
                            "  {} Generated {} ({} bytes)",
                            "✓".bright_green(),
                            out_path.display(),
                            bitstream.len()
                        );
                    }
                    Err(e) => {
                        error!(path = %out_path.display(), error = %e, "Cannot write bitstream");
                        eprintln!(
                            "  {} Cannot write {}: {}",
                            "✗".bright_red(),
                            out_path.display(),
                            e
                        );
                        std::process::exit(1);
                    }
                }

                // Also output the synthesized Verilog code for direct inspection/validation
                let verilog = crate::singularity::synthesize_ast_to_verilog(&program);
                let verilog_path = output.with_extension("v");
                if let Err(e) = std::fs::write(&verilog_path, &verilog) {
                    error!(path = %verilog_path.display(), error = %e, "Cannot write RTL Verilog");
                } else {
                    eprintln!(
                        "  {} Generated RTL Verilog {} ({} bytes)",
                        "✓".bright_green(),
                        verilog_path.display(),
                        verilog.len()
                    );
                }
            }
            Err(e) => {
                error!(error = %e, "ASIC/FPGA fabric synthesis failed");
                eprintln!(
                    "  {} ASIC/FPGA fabric synthesis failed: {}",
                    "✗".bright_red(),
                    e
                );
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("  {} Generating Rust...", "→".bright_blue());
        let rust_code = codegen::generate_rust(&program);
        let out_path = output.with_extension("rs");
        match std::fs::write(&out_path, &rust_code) {
            Ok(_) => {
                info!(path = %out_path.display(), bytes = rust_code.len(), "Rust code generated");
                eprintln!(
                    "  {} Generated {} ({} bytes)",
                    "✓".bright_green(),
                    out_path.display(),
                    rust_code.len()
                );
            }
            Err(e) => {
                error!(path = %out_path.display(), error = %e, "Cannot write Rust code");
                eprintln!(
                    "  {} Cannot write {}: {}",
                    "✗".bright_red(),
                    out_path.display(),
                    e
                );
                std::process::exit(1);
            }
        }
    }

    // ── CORTEX Provenance Manifest ───────────────────────────────────
    emit_cortex_manifest(file, output, timeout_ms, &results);
}

fn unsupported_build_feature(program: &Program, target: &str) -> Option<&'static str> {
    if target == "silicon" {
        return None;
    }
    if program.items.iter().any(item_uses_u256_or_big_literal) {
        return Some(match target {
            "rust" => "the Rust backend does not yet emit a runtime U256 implementation",
            "llvm" => "the LLVM backend does not yet emit coherent i256 operations",
            _ => "unsupported backend target",
        });
    }
    if program.items.iter().any(item_uses_address_literal) {
        return Some(match target {
            "rust" => "the Rust backend does not yet lower address literals",
            "llvm" => "the LLVM backend does not yet lower address literals",
            _ => "unsupported backend target",
        });
    }
    None
}

fn item_uses_u256_or_big_literal(item: &Item) -> bool {
    match item {
        Item::Function(f) => {
            f.params.iter().any(|p| type_uses_u256(&p.ty))
                || f.return_type.as_ref().is_some_and(type_uses_u256)
                || block_uses_u256_or_big_literal(&f.body)
        }
        Item::Struct(s) => s.fields.iter().any(|f| type_uses_u256(&f.ty)),
        Item::Const(c) => type_uses_u256(&c.ty) || expr_uses_big_literal(&c.value),
        Item::Contract(c) => {
            c.state_vars.iter().any(|sv| {
                type_uses_u256(&sv.ty) || sv.default.as_ref().is_some_and(expr_uses_big_literal)
            }) || c
                .functions
                .iter()
                .any(|f| item_uses_u256_or_big_literal(&Item::Function(f.clone())))
        }
        Item::GhostVar(g) => type_uses_u256(&g.ty) || expr_uses_big_literal(&g.value),
    }
}

fn item_uses_address_literal(item: &Item) -> bool {
    match item {
        Item::Function(f) => block_uses_address_literal(&f.body),
        Item::Const(c) => expr_uses_address_literal(&c.value),
        Item::Contract(c) => {
            c.state_vars
                .iter()
                .any(|sv| sv.default.as_ref().is_some_and(expr_uses_address_literal))
                || c.functions
                    .iter()
                    .any(|f| item_uses_address_literal(&Item::Function(f.clone())))
        }
        Item::GhostVar(g) => expr_uses_address_literal(&g.value),
        _ => false,
    }
}

fn type_uses_u256(ty: &Type) -> bool {
    match ty {
        Type::U256 => true,
        Type::Array(inner) | Type::Option(inner) => type_uses_u256(inner),
        Type::Map(key, value) | Type::Result(key, value) => {
            type_uses_u256(key) || type_uses_u256(value)
        }
        _ => false,
    }
}

fn block_uses_u256_or_big_literal(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Let { ty, value, .. } => {
            ty.as_ref().is_some_and(type_uses_u256) || expr_uses_big_literal(value)
        }
        Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => expr_uses_big_literal(value),
        Stmt::Return(Some(expr))
        | Stmt::Assert {
            condition: expr, ..
        } => expr_uses_big_literal(expr),
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_uses_big_literal(condition)
                || block_uses_u256_or_big_literal(then_block)
                || else_block
                    .as_ref()
                    .is_some_and(block_uses_u256_or_big_literal)
        }
        Stmt::While {
            condition, body, ..
        } => expr_uses_big_literal(condition) || block_uses_u256_or_big_literal(body),
        Stmt::Emit { args, .. } => args.iter().any(expr_uses_big_literal),
        Stmt::Expr(expr) => expr_uses_big_literal(expr),
        _ => false,
    })
}

fn block_uses_address_literal(block: &Block) -> bool {
    block.stmts.iter().any(|stmt| match stmt {
        Stmt::Let { value, .. } | Stmt::Assign { value, .. } | Stmt::Ghost { value, .. } => {
            expr_uses_address_literal(value)
        }
        Stmt::Return(Some(expr))
        | Stmt::Assert {
            condition: expr, ..
        } => expr_uses_address_literal(expr),
        Stmt::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_uses_address_literal(condition)
                || block_uses_address_literal(then_block)
                || else_block.as_ref().is_some_and(block_uses_address_literal)
        }
        Stmt::While {
            condition, body, ..
        } => expr_uses_address_literal(condition) || block_uses_address_literal(body),
        Stmt::Emit { args, .. } => args.iter().any(expr_uses_address_literal),
        Stmt::Expr(expr) => expr_uses_address_literal(expr),
        _ => false,
    })
}

fn expr_uses_big_literal(expr: &Expr) -> bool {
    match expr {
        Expr::BigIntLit(_) => true,
        Expr::BinOp { left, right, .. } => {
            expr_uses_big_literal(left) || expr_uses_big_literal(right)
        }
        Expr::UnaryOp { operand, .. } => expr_uses_big_literal(operand),
        Expr::FnCall { args, .. } => args.iter().any(expr_uses_big_literal),
        Expr::MethodCall { object, args, .. } => {
            expr_uses_big_literal(object) || args.iter().any(expr_uses_big_literal)
        }
        Expr::FieldAccess { object, .. } => expr_uses_big_literal(object),
        Expr::Index { object, index } => {
            expr_uses_big_literal(object) || expr_uses_big_literal(index)
        }
        Expr::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_uses_big_literal(condition)
                || block_uses_u256_or_big_literal(then_block)
                || else_block
                    .as_ref()
                    .is_some_and(block_uses_u256_or_big_literal)
        }
        Expr::Block(block) => block_uses_u256_or_big_literal(block),
        _ => false,
    }
}

fn expr_uses_address_literal(expr: &Expr) -> bool {
    match expr {
        Expr::AddressLit(_) => true,
        Expr::BinOp { left, right, .. } => {
            expr_uses_address_literal(left) || expr_uses_address_literal(right)
        }
        Expr::UnaryOp { operand, .. } => expr_uses_address_literal(operand),
        Expr::FnCall { args, .. } => args.iter().any(expr_uses_address_literal),
        Expr::MethodCall { object, args, .. } => {
            expr_uses_address_literal(object) || args.iter().any(expr_uses_address_literal)
        }
        Expr::FieldAccess { object, .. } => expr_uses_address_literal(object),
        Expr::Index { object, index } => {
            expr_uses_address_literal(object) || expr_uses_address_literal(index)
        }
        Expr::If {
            condition,
            then_block,
            else_block,
        } => {
            expr_uses_address_literal(condition)
                || block_uses_address_literal(then_block)
                || else_block.as_ref().is_some_and(block_uses_address_literal)
        }
        Expr::Block(block) => block_uses_address_literal(block),
        _ => false,
    }
}

fn program_function_count(program: &crate::core::ast::Program) -> usize {
    program.items.iter().fold(0, |functions, item| match item {
        crate::core::ast::Item::Function(_) => functions + 1,
        crate::core::ast::Item::Contract(contract) => functions + contract.functions.len(),
        _ => functions,
    })
}

/// Emit a CORTEX provenance manifest alongside the compiled output.
///
/// The manifest contains proof_hash, invariant counts, and verification
/// metadata for every verified function. This is the bridge artifact that
/// `cortex-persist/cortex/engine/anvil_bridge.py` consumes.
fn emit_cortex_manifest(
    source_file: &Path,
    output: &Path,
    timeout_ms: u64,
    results: &[verifier::VerifyResult],
) {
    let _span = info_span!("emit_cortex_manifest").entered();
    let manifest_path = output.with_extension("cortex_manifest.json");

    let functions: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "fn_name": r.fn_name,
                "verified": r.verified,
                "proof_hash": r.proof_hash,
                "invariants_checked": r.invariants_checked,
                "preconditions_count": r.preconditions_count,
                "postconditions_count": r.postconditions_count,
                "duration_ms": r.duration_ms,
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "schema_version": "1.0",
        "anvil_version": env!("CARGO_PKG_VERSION"),
        "source_file": source_file.to_string_lossy(),
        "timeout_ms": timeout_ms,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "verifier": "anvil-z3",
        "fact_type": "anvil_verified_execution",
        "functions": functions,
        "total_proven": results.iter().filter(|r| r.verified).count(),
        "total_failed": results.iter().filter(|r| !r.verified).count(),
    });

    match std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    ) {
        Ok(_) => {
            info!(path = %manifest_path.display(), proofs = results.len(), "CORTEX manifest emitted");
            eprintln!(
                "  {} CORTEX manifest: {} ({} proofs)",
                "🔐".to_string().as_str(),
                manifest_path.display(),
                results.len(),
            );
        }
        Err(e) => {
            error!(path = %manifest_path.display(), error = %e, "Cannot write CORTEX manifest");
            eprintln!(
                "  {} Cannot write manifest {}: {}",
                "✗".bright_red(),
                manifest_path.display(),
                e
            );
            std::process::exit(1);
        }
    }
}
