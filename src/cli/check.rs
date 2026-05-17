use colored::Colorize;
use std::path::PathBuf;
use tracing::{info, error, info_span};

use crate::core::{ast, parser, typechecker};
use crate::engine::verifier;

pub fn cmd_check(file: &PathBuf, json_output: bool) {
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
