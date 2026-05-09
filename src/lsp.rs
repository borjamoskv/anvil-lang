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
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec!["w".to_string(), "a".to_string(), "g".to_string(), "e".to_string()]),
                    ..Default::default()
                }),
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

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        // Read the document (in a real impl, we'd cache this)
        let path = uri.path();
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let line_text = text.lines().nth(position.line as usize).unwrap_or("");

        // Check if hovering over a function definition
        if line_text.trim_start().starts_with("fn ") {
            // Parse the file to get invariant info
            if let Ok(program) = crate::parser::parse_program(&text) {
                for item in &program.items {
                    if let crate::ast::Item::Function(f) = item {
                        if line_text.contains(&f.name) {
                            let inv_count = f.invariants.len();
                            let assume_count = f.assumes.len();
                            let param_count = f.params.len();
                            let content = format!(
                                "**Anvil Function: `{}`**\n\n\
                                - Parameters: {}\n\
                                - Invariants (where): {}\n\
                                - Assumptions (assumes): {}\n\
                                - Return type: {}\n\n\
                                *All invariants verified by Z3 at compile time.*",
                                f.name, param_count, inv_count, assume_count,
                                f.return_type.as_ref().map(|t| format!("{:?}", t)).unwrap_or_else(|| "()".to_string())
                            );
                            return Ok(Some(Hover {
                                contents: HoverContents::Markup(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: content,
                                }),
                                range: None,
                            }));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn completion(&self, _params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let items = vec![
            CompletionItem {
                label: "where".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Invariant clause — Z3 proves these at compile time".to_string()),
                insert_text: Some("where {\n    $0\n}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "assumes".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Environment axioms — trusted without proof".to_string()),
                insert_text: Some("assumes {\n    $0\n}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "ghost".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Ghost variable — exists in Z3 only, stripped from codegen".to_string()),
                insert_text: Some("ghost ${1:name}: ${2:u256} = ${0:expr};".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "emit".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Emit on-chain event".to_string()),
                insert_text: Some("emit ${1:EventName}(${0:args});".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "contract".to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some("Define a smart contract with state and invariants".to_string()),
                insert_text: Some("contract ${1:Name} {\n    state ${2:var}: ${3:u256} = ${4:0};\n\n    invariant {\n        $0\n    }\n}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
        ];
        Ok(Some(CompletionResponse::Array(items)))
    }
}

pub async fn run_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
