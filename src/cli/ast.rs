use colored::Colorize;
use std::path::Path;
use tracing::{error, info_span};

use crate::core::parser;

pub fn cmd_ast(file: &Path) {
    let _span = info_span!("cmd_ast", file = %file.display()).entered();
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

    let program = match parser::parse_program(&source) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("  {} {}", "✗".bright_red(), e);
            std::process::exit(1);
        }
    };

    println!("{}", serde_json::to_string_pretty(&program).unwrap());
}
