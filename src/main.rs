// ============================================================
// ANVIL CLI — "Where trust doesn't compile."
// ============================================================

mod ast;
mod parser;
mod verifier;
mod codegen;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "anvil",
    version = "0.1.0",
    about = "Anvil — A programming language where trust doesn't compile.",
    long_about = "Anvil is a formally verified programming language for smart contracts \
                  and autonomous agents. Every function carries its proof as a first-class \
                  citizen. If the compiler can't prove your invariants, your code doesn't exist."
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
    },
    /// Compile an Anvil file to Rust
    Build {
        /// Path to the .anv file
        file: PathBuf,
        /// Output path for generated Rust
        #[arg(short, long, default_value = "out.rs")]
        output: PathBuf,
    },
    /// Parse and dump the AST as JSON
    Ast {
        /// Path to the .anv file
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    print_banner();

    match cli.command {
        Commands::Check { file } => cmd_check(&file),
        Commands::Build { file, output } => cmd_build(&file, &output),
        Commands::Ast { file } => cmd_ast(&file),
    }
}

fn print_banner() {
    eprintln!();
    eprintln!("{}", "  ╔═══════════════════════════════════════════╗".bright_blue());
    eprintln!("{}", "  ║   ▄▀█ ███▄ █ █ █ █ █   █                 ║".bright_blue());
    eprintln!("{}", "  ║   █▀█ █  ▀██ ▀▄▀ █ █▄▄ █▄▄    v0.1      ║".bright_blue());
    eprintln!("{}", "  ║   Where trust doesn't compile.            ║".bright_blue());
    eprintln!("{}", "  ╚═══════════════════════════════════════════╝".bright_blue());
    eprintln!();
}

fn cmd_check(file: &PathBuf) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
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

    let fn_count = program.items.iter().filter(|i| matches!(i, ast::Item::Function(_))).count();
    let inv_count: usize = program.items.iter().map(|i| match i {
        ast::Item::Function(f) => f.invariants.len(),
        ast::Item::Contract(c) => c.functions.iter().map(|f| f.invariants.len()).sum(),
        _ => 0,
    }).sum();

    eprintln!("  {} Parsed: {} functions, {} invariants",
        "✓".bright_green(), fn_count, inv_count);
    eprintln!("  {} Verifying with Z3...", "→".bright_blue());

    let results = verifier::verify_program(&program);
    verifier::print_results(&results);

    let all_ok = results.iter().all(|r| r.verified);
    if !all_ok {
        std::process::exit(1);
    }
}

fn cmd_build(file: &PathBuf, output: &PathBuf) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
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

    eprintln!("  {} Verifying with Z3...", "→".bright_blue());
    let results = verifier::verify_program(&program);
    verifier::print_results(&results);

    let all_ok = results.iter().all(|r| r.verified);
    if !all_ok {
        eprintln!("  {} Cannot build: verification failed. Fix your invariants.",
            "✗".bright_red().bold());
        std::process::exit(1);
    }

    eprintln!("  {} Generating Rust...", "→".bright_blue());
    let rust_code = codegen::generate_rust(&program);

    match std::fs::write(output, &rust_code) {
        Ok(_) => {
            eprintln!("  {} Generated {} ({} bytes)",
                "✓".bright_green(), output.display(), rust_code.len());
        },
        Err(e) => {
            eprintln!("  {} Cannot write {}: {}", "✗".bright_red(), output.display(), e);
            std::process::exit(1);
        }
    }
}

fn cmd_ast(file: &PathBuf) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
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
