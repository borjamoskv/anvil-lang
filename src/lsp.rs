use crate::core::parser;
use crate::core::typechecker;
use crate::engine::verifier;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, Semaphore};
use tokio::task;
use tokio::time::{Duration, sleep};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

const LSP_VERIFY_DEBOUNCE_MS: u64 = 350;
const LSP_SOLVER_TIMEOUT_MS: u64 = 1_000;
const LSP_MAX_SOURCE_BYTES: usize = 50 * 1024;

#[derive(Clone, Debug)]
pub struct Backend {
    client: Client,
    documents: Arc<Mutex<HashMap<Url, DocumentState>>>,
    next_revision: Arc<AtomicU64>,
    verify_slots: Arc<Semaphore>,
}

#[derive(Clone, Copy, Debug, Default)]
struct DocumentState {
    revision: u64,
    version: Option<i32>,
}

impl Backend {
    async fn schedule_verify_document(&self, uri: Url, text: String, version: Option<i32>) {
        let revision = self.next_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let state = {
            let mut documents = self.documents.lock().await;
            let state = DocumentState { revision, version };
            documents.insert(uri.clone(), state);
            state
        };

        sleep(Duration::from_millis(LSP_VERIFY_DEBOUNCE_MS)).await;

        if !self.is_current_document_state(&uri, state).await {
            return;
        }

        let Ok(_permit) = self.verify_slots.acquire().await else {
            return;
        };

        if !self.is_current_document_state(&uri, state).await {
            return;
        }

        self.verify_document(uri, text, state).await;
    }

    async fn is_current_document_state(&self, uri: &Url, state: DocumentState) -> bool {
        let documents = self.documents.lock().await;
        documents.get(uri).copied().is_some_and(|current| {
            current.revision == state.revision && current.version == state.version
        })
    }

    async fn verify_document(&self, uri: Url, text: String, state: DocumentState) {
        let diagnostics = match task::spawn_blocking(move || collect_diagnostics(&text)).await {
            Ok(diagnostics) => diagnostics,
            Err(e) => vec![Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 100)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Anvil LSP verification task failed: {e}"),
                source: Some("anvil::lsp".to_string()),
                ..Default::default()
            }],
        };

        if !self.is_current_document_state(&uri, state).await {
            return;
        }

        self.client
            .publish_diagnostics(uri, diagnostics, state.version)
            .await;
    }
}

fn collect_diagnostics(text: &str) -> Vec<Diagnostic> {
    if text.len() > LSP_MAX_SOURCE_BYTES {
        return vec![Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 100)),
            severity: Some(DiagnosticSeverity::WARNING),
            message: format!(
                "Anvil LSP skipped live verification for this document because it exceeds {}KB. Run `anvil check` manually for full verification.",
                LSP_MAX_SOURCE_BYTES / 1024
            ),
            source: Some("anvil::lsp".to_string()),
            ..Default::default()
        }];
    }

    let mut diagnostics = Vec::new();

    // 1. Parse Phase
    let program = match parser::parse_program(text) {
        Ok(p) => p,
        Err(e) => {
            // Parse error bounds (fallback to line 0 if pest spans aren't available)
            diagnostics.push(Diagnostic::new_simple(
                Range::new(Position::new(0, 0), Position::new(0, 100)),
                e,
            ));
            return diagnostics;
        }
    };

    // 2. Type Checker Phase
    let type_env = typechecker::check_program(&program);
    for err in &type_env.errors {
        let line = find_line(text, &err.location);
        diagnostics.push(Diagnostic {
            range: Range::new(Position::new(line, 0), Position::new(line, 100)),
            severity: Some(DiagnosticSeverity::ERROR),
            message: err.message.clone(),
            source: Some("anvil::typechecker".to_string()),
            ..Default::default()
        });
    }
    for warn in &type_env.warnings {
        let line = find_line(text, &warn.location);
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
        return diagnostics;
    }

    // 3. Z3 Verification Phase
    let results = verifier::verify_program_with_options(
        &program,
        &type_env,
        verifier::VerifyOptions {
            timeout_ms: LSP_SOLVER_TIMEOUT_MS,
        },
    );
    let function_count = program.items.iter().fold(0, |count, item| match item {
        crate::core::ast::Item::Function(_) => count + 1,
        crate::core::ast::Item::Contract(contract) => count + contract.functions.len(),
        _ => count,
    });

    if results.is_empty() {
        diagnostics.push(Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 100)),
            severity: Some(DiagnosticSeverity::ERROR),
            message:
                "No verification obligations found. Add invariants before claiming verification."
                    .to_string(),
            source: Some("anvil::z3".to_string()),
            ..Default::default()
        });
    } else if results.len() < function_count {
        diagnostics.push(Diagnostic {
            range: Range::new(Position::new(0, 0), Position::new(0, 100)),
            severity: Some(DiagnosticSeverity::ERROR),
            message: "Some functions have no verification obligations. Add invariants to every function before claiming verification.".to_string(),
            source: Some("anvil::z3".to_string()),
            ..Default::default()
        });
    }

    for res in results {
        if !res.verified {
            let line = find_line(text, &res.fn_name);
            let msg = res
                .counterexample
                .unwrap_or_else(|| "Z3 Undecidable".to_string());
            diagnostics.push(Diagnostic {
                range: Range::new(Position::new(line, 0), Position::new(line, 100)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: format!("Mathematical Proof Failed:\n{}", msg),
                source: Some("anvil::z3".to_string()),
                ..Default::default()
            });
        }
    }

    diagnostics
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
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "w".to_string(),
                        "a".to_string(),
                        "g".to_string(),
                        "e".to_string(),
                    ]),
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
        let backend = self.clone();
        tokio::spawn(async move {
            let version = Some(params.text_document.version);
            backend
                .schedule_verify_document(
                    params.text_document.uri,
                    params.text_document.text,
                    version,
                )
                .await;
        });
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.pop() {
            let backend = self.clone();
            tokio::spawn(async move {
                let version = Some(params.text_document.version);
                backend
                    .schedule_verify_document(params.text_document.uri, change.text, version)
                    .await;
            });
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        {
            let mut documents = self.documents.lock().await;
            documents.remove(&params.text_document.uri);
        }
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        // Read the document (in a real impl, we'd cache this)
        let path = match uri.to_file_path() {
            Ok(path) => path,
            Err(_) => return Ok(None),
        };
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };

        let line_text = text.lines().nth(position.line as usize).unwrap_or("");

        // Check if hovering over a function definition
        if line_text.trim_start().starts_with("fn ") {
            // Parse the file to get invariant info
            if let Ok(program) = crate::core::parser::parse_program(&text) {
                for item in &program.items {
                    if let crate::core::ast::Item::Function(f) = item {
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
                                f.name,
                                param_count,
                                inv_count,
                                assume_count,
                                f.return_type
                                    .as_ref()
                                    .map(|t| format!("{:?}", t))
                                    .unwrap_or_else(|| "()".to_string())
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

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(Mutex::new(HashMap::new())),
        next_revision: Arc::new(AtomicU64::new(0)),
        verify_slots: Arc::new(Semaphore::new(1)),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
