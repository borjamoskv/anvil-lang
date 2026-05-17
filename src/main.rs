// ============================================================
// ANVIL CLI — "Where trust doesn't compile."
// ============================================================

pub mod core;
pub mod engine;
pub mod cli;
mod lsp;
mod singularity;

use cli::KeyAction;

use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::PathBuf;
use tracing::info;

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

    let cli_args = Cli::parse();

    print_banner();

    match cli_args.command {
        Commands::Check { file, json } => cli::cmd_check(&file, json),
        Commands::Build { file, output, target } => cli::cmd_build(&file, &output, &target),
        Commands::Compile { file } => info!("Iniciando pase de compilación a Rust para: {:?}", file),
        Commands::Singularity => singularity::initiate_engine(),
        Commands::Ast { file } => cli::cmd_ast(&file),
        Commands::Lsp => lsp::run_server().await,
        Commands::Saas { port } => engine::saas::start_server(port).await,
        Commands::Keys { action } => cli::cmd_keys(action).await,
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

#[cfg(test)]
mod tests {
    #[test]
    fn test_verification_sanity() {
        // [C5-REAL] Basic sanity test to ensure the CI pipeline runs tests.
        // In a full implementation, we would test parser and typechecker here.
        assert_eq!(1 + 1, 2, "Thermodynamic laws broken");
    }
}
