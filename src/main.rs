// ============================================================
// ANVIL CLI — "Where trust doesn't compile."
// ============================================================

mod ast;
mod parser;
mod typechecker;
mod verifier;
mod codegen;
mod lsp;
mod llvm_ir;
mod saas;

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
        /// Output path for generated code
        #[arg(short, long, default_value = "out")]
        output: PathBuf,
        /// Target architecture (rust, llvm)
        #[arg(short, long, default_value = "rust")]
        target: String,
    },
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
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    print_banner();

    match cli.command {
        Commands::Check { file } => cmd_check(&file),
        Commands::Build { file, output, target } => cmd_build(&file, &output, &target),
        Commands::Ast { file } => cmd_ast(&file),
        Commands::Lsp => lsp::run_server().await,
        Commands::Saas { port } => saas::start_server(port).await,
    }
}

fn print_banner() {
    eprintln!();
    eprintln!("{}", "  ╔═══════════════════════════════════════════╗".bright_blue());
    eprintln!("{}", "  ║   ▄▀█ ███▄ █ █ █ █ █   █                 ║".bright_blue());
    eprintln!("{}", "  ║   █▀█ █  ▀██ ▀▄▀ █ █▄▄ █▄▄    v0.5      ║".bright_blue());
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

    // Type checking pass — bridge mathematical abstraction to silicon bounds
    eprintln!("  {} Type checking...", "→".bright_blue());
    let type_env = typechecker::check_program(&program);
    typechecker::print_type_report(&type_env);

    if !type_env.errors.is_empty() {
        eprintln!("  {} Type checking failed. Fix type errors before verification.",
            "✗".bright_red().bold());
        std::process::exit(1);
    }

    eprintln!("  {} Verifying with Z3...", "→".bright_blue());

    let results = verifier::verify_program(&program, &type_env);
    verifier::print_results(&results);

    let all_ok = results.iter().all(|r| r.verified);
    if !all_ok {
        std::process::exit(1);
    }
}

fn cmd_build(file: &PathBuf, output: &PathBuf, target: &str) {
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

    // Type checking pass
    eprintln!("  {} Type checking...", "→".bright_blue());
    let type_env = typechecker::check_program(&program);
    typechecker::print_type_report(&type_env);

    if !type_env.errors.is_empty() {
        eprintln!("  {} Type checking failed.", "✗".bright_red().bold());
        std::process::exit(1);
    }

    eprintln!("  {} Verifying with Z3...", "→".bright_blue());
    let results = verifier::verify_program(&program, &type_env);
    verifier::print_results(&results);

    let all_ok = results.iter().all(|r| r.verified);
    if !all_ok {
        eprintln!("  {} Cannot build: verification failed. Fix your invariants.",
            "✗".bright_red().bold());
        std::process::exit(1);
    }

    if target == "llvm" {
        eprintln!("  {} Generating LLVM IR...", "→".bright_blue());
        let llvm_ir = llvm_ir::generate_llvm_ir(&program);
        let out_path = output.with_extension("ll");
        match std::fs::write(&out_path, &llvm_ir) {
            Ok(_) => eprintln!("  {} Generated {} ({} bytes)", "✓".bright_green(), out_path.display(), llvm_ir.len()),
            Err(e) => eprintln!("  {} Cannot write {}: {}", "✗".bright_red(), out_path.display(), e),
        }
    } else {
        eprintln!("  {} Generating Rust...", "→".bright_blue());
        let rust_code = codegen::generate_rust(&program);
        let out_path = output.with_extension("rs");
        match std::fs::write(&out_path, &rust_code) {
            Ok(_) => eprintln!("  {} Generated {} ({} bytes)", "✓".bright_green(), out_path.display(), rust_code.len()),
            Err(e) => eprintln!("  {} Cannot write {}: {}", "✗".bright_red(), out_path.display(), e),
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
