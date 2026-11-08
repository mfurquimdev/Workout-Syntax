//! The language server end to end, over an in-memory connection.
//! The sync tests check the `Consistent` and `Monotonic` properties of
//! tla/DocumentSync.tla on the real server.

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use lsp_types::{PublishDiagnosticsParams, Uri};
use serde_json::{json, Value};
use std::thread::JoinHandle;
use std::time::Duration;

struct Client {
    conn: Connection,
    server: Option<JoinHandle<()>>,
    next_id: i32,
}

impl Client {
    fn start() -> Client {
        let (server, conn) = Connection::memory();
        let handle = std::thread::spawn(move || workout_syntax::lsp::server::run(server).unwrap());
        let mut c = Client { conn, server: Some(handle), next_id: 1 };
        let caps = c.request("initialize", json!({ "capabilities": {} }));
        assert!(caps["capabilities"]["hoverProvider"].as_bool().unwrap(), "{caps}");
        c.notify("initialized", json!({}));
        c
    }

    fn notify(&self, method: &str, params: Value) {
        self.conn
            .sender
            .send(Message::Notification(Notification { method: method.into(), params }))
            .unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = RequestId::from(self.next_id);
        self.next_id += 1;
        self.conn
            .sender
            .send(Message::Request(Request { id: id.clone(), method: method.into(), params }))
            .unwrap();
        loop {
            match self.recv() {
                Message::Response(r) if r.id == id => {
                    return r.response_result.expect("request failed");
                }
                _ => continue,
            }
        }
    }

    fn recv(&self) -> Message {
        self.conn.receiver.recv_timeout(Duration::from_secs(5)).expect("server reply")
    }

    fn diagnostics(&self) -> PublishDiagnosticsParams {
        loop {
            if let Message::Notification(n) = self.recv()
                && n.method == "textDocument/publishDiagnostics"
            {
                return serde_json::from_value(n.params).unwrap();
            }
        }
    }

    fn open(&self, uri: &str, version: i32, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({ "textDocument": { "uri": uri, "languageId": "workout", "version": version, "text": text } }),
        );
    }

    fn change(&self, uri: &str, version: i32, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({ "textDocument": { "uri": uri, "version": version }, "contentChanges": [{ "text": text }] }),
        );
    }

    fn close(&self, uri: &str) {
        self.notify("textDocument/didClose", json!({ "textDocument": { "uri": uri } }));
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        self.request("shutdown", Value::Null);
        self.notify("exit", Value::Null);
        self.server.take().unwrap().join().expect("server exits cleanly");
    }
}

const A: &str = "file:///log/a.workout.md";
const B: &str = "file:///log/b.workout.md";
const GOOD: &str = "2026-01-01\n- Row 12x20kg\n";
const BAD: &str = "2026-01-01\n- Row 12x54\n";

fn uri(s: &str) -> Uri {
    s.parse().unwrap()
}

#[test]
fn open_change_close_publishes_matching_diagnostics() {
    let c = Client::start();
    c.open(A, 1, BAD);
    let d = c.diagnostics();
    assert_eq!((d.uri.clone(), d.version, d.diagnostics.len()), (uri(A), Some(1), 1));

    c.change(A, 2, GOOD);
    let d = c.diagnostics();
    assert_eq!((d.version, d.diagnostics.len()), (Some(2), 0));

    c.close(A);
    let d = c.diagnostics();
    assert_eq!((d.uri, d.diagnostics.len()), (uri(A), 0));
}

#[test]
fn interleaved_documents_stay_consistent_and_versions_never_go_back() {
    let c = Client::start();
    let script: [(&str, i32, &str); 5] = [(A, 1, BAD), (B, 1, GOOD), (A, 2, GOOD), (B, 2, BAD), (A, 3, BAD)];
    let mut last = std::collections::HashMap::new();
    for (i, (doc, ver, text)) in script.iter().enumerate() {
        if *ver == 1 { c.open(doc, *ver, text) } else { c.change(doc, *ver, text) }
        let d = c.diagnostics();
        assert_eq!(d.uri, uri(doc), "step {i}");
        assert_eq!(d.version, Some(*ver));
        assert_eq!(d.diagnostics.len(), usize::from(*text == BAD));
        let prev = last.insert(d.uri.to_string(), *ver).unwrap_or(0);
        assert!(*ver > prev);
    }
}

#[test]
fn requests_on_an_open_document() {
    let mut c = Client::start();
    let text = "2026-01-01\n18h00~19h00\n- Bench Press +22kg x2 15x0kg 12x5kg\n\n2026-01-02\n- Ben";
    c.open(A, 1, text);
    c.diagnostics();

    let td = json!({ "uri": A });
    let hover = c.request("textDocument/hover", json!({ "textDocument": td, "position": { "line": 2, "character": 4 } }));
    assert!(hover["contents"]["value"].as_str().unwrap().contains("**Bench Press**"), "{hover}");

    let items = c.request("textDocument/completion", json!({ "textDocument": td, "position": { "line": 5, "character": 5 } }));
    assert_eq!(items[0]["label"], "Bench Press", "{items}");

    let syms = c.request("textDocument/documentSymbol", json!({ "textDocument": td }));
    assert_eq!(syms.as_array().unwrap().len(), 2);

    let folds = c.request("textDocument/foldingRange", json!({ "textDocument": td }));
    assert_eq!(folds[0]["startLine"], 0);

    let range = json!({ "start": { "line": 0, "character": 0 }, "end": { "line": 6, "character": 0 } });
    let hints = c.request("textDocument/inlayHint", json!({ "textDocument": td, "range": range }));
    assert_eq!(hints[0]["label"], "27 reps · 714 kg", "{hints}");
}

#[test]
fn requests_on_unknown_documents_return_null() {
    let mut c = Client::start();
    let r = c.request(
        "textDocument/hover",
        json!({ "textDocument": { "uri": B }, "position": { "line": 0, "character": 0 } }),
    );
    assert_eq!(r, Value::Null);
}

#[test]
fn semantic_tokens_full_and_range_round_trip() {
    let mut c = Client::start();
    let init_check = c.request("textDocument/semanticTokens/full", json!({ "textDocument": { "uri": B } }));
    assert_eq!(init_check, Value::Null);

    let text = "2026-11-07\n- Row 12x20kg ; 🔥 ok\n- Bike 3~5º";
    c.open(A, 1, text);
    c.diagnostics();
    let td = json!({ "uri": A });
    let full = c.request("textDocument/semanticTokens/full", json!({ "textDocument": td }));
    let data: Vec<u32> = serde_json::from_value(full["data"].clone()).unwrap();
    let expected: Vec<u32> = workout_syntax::lsp::analysis::semantic_tokens(text)
        .iter()
        .flat_map(|t| [t.delta_line, t.delta_start, t.length, t.token_type, t.token_modifiers_bitset])
        .collect();
    assert_eq!(data, expected);
    // first token: line 0, col 0, the 10-char date, type 0 (workoutDate)
    assert_eq!(&data[..5], [0, 0, 10, 0, 0]);

    let range = json!({ "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 11 } });
    let part = c.request("textDocument/semanticTokens/range", json!({ "textDocument": td, "range": range }));
    let data: Vec<u32> = serde_json::from_value(part["data"].clone()).unwrap();
    assert_eq!(&data[..3], [2, 0, 1], "bullet of line 2 first: {part}");
    assert_eq!(data.len(), 6 * 5);
}

#[test]
fn semantic_tokens_legend_is_advertised() {
    let (server, conn) = Connection::memory();
    let handle = std::thread::spawn(move || workout_syntax::lsp::server::run(server).unwrap());
    let id = RequestId::from(1);
    conn.sender
        .send(Message::Request(Request { id: id.clone(), method: "initialize".into(), params: json!({ "capabilities": {} }) }))
        .unwrap();
    let Message::Response(r) = conn.receiver.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!() };
    let caps = r.response_result.unwrap();
    let provider = &caps["capabilities"]["semanticTokensProvider"];
    assert_eq!(provider["legend"]["tokenTypes"][0], "workoutDate", "{caps}");
    assert_eq!(provider["legend"]["tokenTypes"][23], "workoutInvalid");
    assert_eq!(provider["full"], true);
    assert_eq!(provider["range"], true);
    conn.sender.send(Message::Notification(Notification { method: "initialized".into(), params: json!({}) })).unwrap();
    conn.sender.send(Message::Request(Request { id: RequestId::from(2), method: "shutdown".into(), params: Value::Null })).unwrap();
    let _ = conn.receiver.recv_timeout(Duration::from_secs(5));
    conn.sender.send(Message::Notification(Notification { method: "exit".into(), params: Value::Null })).unwrap();
    handle.join().unwrap();
}

#[test]
fn code_actions_add_the_empty_line_before_a_session() {
    let mut c = Client::start();
    let text = "2026-01-01\n- Row 12x20kg\n2026-01-02\n- Row 12x25kg\n2026-01-03\n";
    c.open(A, 1, text);
    let d = c.diagnostics();
    let messages: Vec<&str> = d.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "a session starts after an empty line: add one before 2026-01-02",
            "a session starts after an empty line: add one before 2026-01-03",
        ]
    );
    let range = json!({ "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 0 } });
    let actions = c.request(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": A }, "range": range, "context": { "diagnostics": [] } }),
    );
    let actions = actions.as_array().unwrap();
    assert_eq!(actions.len(), 2, "{actions:?}");
    assert_eq!(actions[0]["title"], "Add an empty line before 2026-01-02");
    assert_eq!(actions[0]["kind"], "quickfix");
    let edit = &actions[0]["edit"]["changes"][A][0];
    assert_eq!(edit["range"]["start"], json!({ "line": 2, "character": 0 }));
    assert_eq!(edit["range"]["end"], json!({ "line": 2, "character": 0 }));
    assert_eq!(edit["newText"], "\n");
    assert_eq!(actions[1]["title"], "Add an empty line before every session");
    assert_eq!(actions[1]["kind"], "quickfix");
    let lines: Vec<&Value> = actions[1]["edit"]["changes"][A].as_array().unwrap().iter().map(|e| &e["range"]["start"]["line"]).collect();
    assert_eq!(lines, [&json!(2), &json!(4)]);

    // the editor applies the fix-all and the warnings go away
    c.change(A, 2, "2026-01-01\n- Row 12x20kg\n\n2026-01-02\n- Row 12x25kg\n\n2026-01-03\n");
    assert!(c.diagnostics().diagnostics.is_empty());
    let actions = c.request(
        "textDocument/codeAction",
        json!({ "textDocument": { "uri": A }, "range": range, "context": { "diagnostics": [] } }),
    );
    assert_eq!(actions, json!([]));
}
