//! Language server loop over an `lsp_server::Connection` (stdio in the
//! binary, in-memory in tests).
//!
//! Document handling follows `tla/DocumentSync.tla`: notifications are
//! processed in arrival order; every open/change stores the full text with
//! its version and publishes diagnostics for exactly that text and
//! version; close forgets the text and publishes an empty list.

use super::analysis;
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::*;
use serde_json::Value;
use std::collections::HashMap;

pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions::default()),
        document_symbol_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        inlay_hint_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: analysis::semantic_tokens_legend(),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: Some(true),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
}

/// Open documents: uri -> (text, version)
#[derive(Default)]
struct Store {
    docs: HashMap<Uri, (String, i32)>,
}

pub fn run(conn: Connection) -> Result<(), Error> {
    conn.initialize(serde_json::to_value(capabilities())?)?;
    let mut store = Store::default();
    for msg in &conn.receiver {
        match msg {
            Message::Request(req) => {
                if conn.handle_shutdown(&req)? {
                    return Ok(());
                }
                conn.sender.send(Message::Response(respond(&store, req)))?;
            }
            Message::Notification(n) => {
                if let Some(publish) = store.apply(n)? {
                    conn.sender.send(Message::Notification(publish))?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

impl Store {
    /// The `Handle` action: update the store, return what to publish.
    fn apply(&mut self, n: Notification) -> Result<Option<Notification>, Error> {
        let (uri, published) = match n.method.as_str() {
            "textDocument/didOpen" => {
                let p: DidOpenTextDocumentParams = serde_json::from_value(n.params)?;
                let d = p.text_document;
                self.docs.insert(d.uri.clone(), (d.text, d.version));
                (d.uri, true)
            }
            "textDocument/didChange" => {
                let p: DidChangeTextDocumentParams = serde_json::from_value(n.params)?;
                // full sync: the last change carries the whole text
                let Some(change) = p.content_changes.into_iter().last() else { return Ok(None) };
                let uri = p.text_document.uri;
                self.docs.insert(uri.clone(), (change.text, p.text_document.version));
                (uri, true)
            }
            "textDocument/didClose" => {
                let p: DidCloseTextDocumentParams = serde_json::from_value(n.params)?;
                self.docs.remove(&p.text_document.uri);
                (p.text_document.uri, false)
            }
            _ => return Ok(None),
        };
        let params = match self.docs.get(&uri).filter(|_| published) {
            Some((text, version)) => PublishDiagnosticsParams {
                uri,
                diagnostics: analysis::diagnostics(text),
                version: Some(*version),
            },
            None => PublishDiagnosticsParams { uri, diagnostics: Vec::new(), version: None },
        };
        Ok(Some(Notification::new("textDocument/publishDiagnostics".into(), params)))
    }

    fn text(&self, uri: &Uri) -> Option<&str> {
        self.docs.get(uri).map(|(t, _)| t.as_str())
    }
}

fn respond(store: &Store, req: Request) -> Response {
    let id = req.id.clone();
    match handle(store, req) {
        Ok(Some(v)) => Response::new_ok(id, v),
        Ok(None) => Response::new_ok(id, Value::Null),
        Err(Handled::Unknown(m)) => {
            Response::new_err(id, ErrorCode::MethodNotFound as i32, format!("unsupported request {m}"))
        }
        Err(Handled::BadParams(e)) => Response::new_err(id, ErrorCode::InvalidParams as i32, e),
    }
}

enum Handled {
    Unknown(String),
    BadParams(String),
}

fn params<P: serde::de::DeserializeOwned>(v: Value) -> Result<P, Handled> {
    serde_json::from_value(v).map_err(|e| Handled::BadParams(e.to_string()))
}

fn json<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).expect("lsp types serialise")
}

fn handle(store: &Store, req: Request) -> Result<Option<Value>, Handled> {
    Ok(match req.method.as_str() {
        "textDocument/hover" => {
            let p: HoverParams = params(req.params)?;
            let tp = p.text_document_position_params;
            store.text(&tp.text_document.uri).and_then(|t| analysis::hover(t, tp.position)).map(|md| {
                json(Hover {
                    contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: md }),
                    range: None,
                })
            })
        }
        "textDocument/completion" => {
            let p: CompletionParams = params(req.params)?;
            let tp = p.text_document_position;
            store
                .text(&tp.text_document.uri)
                .map(|t| json(CompletionResponse::Array(analysis::completions(t, tp.position))))
        }
        "textDocument/documentSymbol" => {
            let p: DocumentSymbolParams = params(req.params)?;
            store
                .text(&p.text_document.uri)
                .map(|t| json(DocumentSymbolResponse::Nested(analysis::symbols(t))))
        }
        "textDocument/foldingRange" => {
            let p: FoldingRangeParams = params(req.params)?;
            store.text(&p.text_document.uri).map(|t| json(analysis::folding(t)))
        }
        "textDocument/inlayHint" => {
            let p: InlayHintParams = params(req.params)?;
            let r = p.range;
            store.text(&p.text_document.uri).map(|t| {
                let hints: Vec<InlayHint> = analysis::inlay_hints(t)
                    .into_iter()
                    .filter(|h| h.position.line >= r.start.line && h.position.line <= r.end.line)
                    .collect();
                json(hints)
            })
        }
        "textDocument/codeAction" => {
            let p: CodeActionParams = params(req.params)?;
            let uri = p.text_document.uri;
            store.text(&uri).map(|t| json(analysis::code_actions(t, &uri, p.range)))
        }
        "textDocument/semanticTokens/full" => {
            let p: SemanticTokensParams = params(req.params)?;
            store.text(&p.text_document.uri).map(|t| {
                json(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: analysis::semantic_tokens(t),
                }))
            })
        }
        "textDocument/semanticTokens/range" => {
            let p: SemanticTokensRangeParams = params(req.params)?;
            let r = p.range;
            store.text(&p.text_document.uri).map(|t| {
                json(SemanticTokensRangeResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: analysis::semantic_tokens_in(t, r.start.line, r.end.line),
                }))
            })
        }
        m => return Err(Handled::Unknown(m.to_string())),
    })
}
