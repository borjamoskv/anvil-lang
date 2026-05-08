use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use crate::parser;
use crate::typechecker;
use crate::verifier;

#[derive(Debug)]
pub struct Backend {
    client: Client,
}

impl Backend {
    async fn verify_document(&self, uri: Url, text: String) {
        let mut diagnostics = Vec::new();

        // 1. Parse Phase
        let program = match parser::parse_program(&text) {
            Ok(p) => p,
            Err(e) => {
                // Parse error bounds (fallback to line 0 if pest spans aren't available)
                diagnostics.push(Diagnostic::new_simple(
                    Range::new(Position::new(0, 0), Position::new(0, 100)),
                    e,
                ));
                self.client.publish_diagnostics(uri, diagnostics, None).await;
                return;
            }
        };

        // 2. Type Checker Phase
        let type_env = typechecker::check_program(&program);
        for err in &type_env.errors {
            let line = find_line(&text, &err.location);
            diagnostics.push(Diagnostic {
                range: Range::new(Position::new(line, 0), Position::new(line, 100)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: err.message.clone(),
                source: Some("anvil::typechecker".to_string()),
                ..Default::default()
            });
        }
        for warn in &type_env.warnings {
            let line = find_line(&text, &warn.location);
            diagnostics.push(Diagnostic {
                range: Range::new(Position::new(line, 0), Position::new(line, 100)),
                severity: Some(DiagnosticSeverity::WARNING),
                message: warn.message.clone(),
                source: Some("anvil::typechecker".to_string()),
                ..Default::default()
            });
        }

        if !type_env.errors.is_empty() {
            // Halt pipeline if typechecking fails (Z3 requires valid silicon bounds)
            self.client.publish_diagnostics(uri, diagnostics, None).await;
            return;
        }

        // 3. Z3 Verification Phase
        let results = verifier::verify_program(&program, &type_env);
        for res in results {
            if !res.verified {
                let line = find_line(&text, &res.fn_name);
                let msg = res.counterexample.unwrap_or_else(|| "Z3 Undecidable".to_string());
                diagnostics.push(Diagnostic {
                    range: Range::new(Position::new(line, 0), Position::new(line, 100)),
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!("Mathematical Proof Failed:\n{}", msg),
                    source: Some("anvil::z3".to_string()),
                    ..Default::default()
                });
            }
        }

        // Emit
        self.client.publish_diagnostics(uri, diagnostics, None).await;
    }
}

fn find_line(text: &str, search: &str) -> u32 {
    for (i, line) in text.lines().enumerate() {
        if line.contains(search) {
            return i as u32;
        }
    }
    0
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Anvil LSP Booted. Hoare Logic Active.")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.verify_document(params.text_document.uri, params.text_document.text).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            self.verify_document(params.text_document.uri, change.text).await;
        }
    }
}

pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
