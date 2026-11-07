//! WebAssembly adapter of `workout_syntax` for the web app.
//!
//! The JS API is the contract in `docs/web-app-interfaces.md`, section 1
//! (in the project repository). The library counts columns in characters;
//! JavaScript strings, CodeMirror and LSP count UTF-16 code units, so every
//! column that crosses this boundary is converted here, and nowhere else.
//!
//! Every export is total: positions out of range give an empty result, and
//! no input panics (a panic in WebAssembly would be an uncatchable trap).
//! The functions are plain Rust as well, so `cargo test -p workout_wasm`
//! checks them natively.

use lsp_types::{CompletionTextEdit, Position};
use serde::Serialize;
use wasm_bindgen::prelude::*;
use workout_syntax::lsp::analysis;
use workout_syntax::{Severity, TokenKind, parse_document, tokenize_line};

// ---------- columns ----------

/// UTF-16 offset of every character boundary of `line`: entry `i` is where
/// character `i` starts, the last entry is the line's length.
fn boundaries(line: &str) -> Vec<u32> {
    let mut out = Vec::with_capacity(line.len() + 1);
    let mut units = 0u32;
    out.push(0);
    for c in line.chars() {
        units += c.len_utf16() as u32;
        out.push(units);
    }
    out
}

/// UTF-16 column of character column `chars`, clamped to the line's end.
fn utf16_col(bounds: &[u32], chars: usize) -> u32 {
    bounds.get(chars).or(bounds.last()).copied().unwrap_or(0)
}

/// `col` (UTF-16) clamped to the line and moved back to a character
/// boundary, so it never splits a surrogate pair.
fn snap_utf16(line: &str, col: u32) -> u32 {
    let mut units = 0u32;
    for c in line.chars() {
        let next = units + c.len_utf16() as u32;
        if next > col {
            break;
        }
        units = next;
    }
    units
}

/// The `n`th line (0-based) as the library splits lines.
fn nth_line(text: &str, n: u32) -> Option<&str> {
    text.lines().nth(n as usize)
}

/// A position the analysis functions can take: the line exists and the
/// column is a character boundary inside it.
fn position(text: &str, line: u32, col: u32) -> Option<Position> {
    let l = nth_line(text, line)?;
    Some(Position::new(line, snap_utf16(l, col)))
}

// ---------- exports ----------

/// Runs once when the module is initialised.
#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// Token kind names, in `TokenKind::ALL` order; `tokenize` gives indexes
/// into this list.
#[wasm_bindgen]
pub fn kinds() -> Vec<String> {
    TokenKind::ALL.iter().map(|k| k.name().to_string()).collect()
}

/// Highlighting tokens of one line, flat `[from, to, kindIndex, ...]` with
/// UTF-16 columns (JS string indexes).
#[wasm_bindgen]
pub fn tokenize(line: &str) -> Vec<u32> {
    let bounds = boundaries(line);
    let mut out = Vec::new();
    for t in tokenize_line(line) {
        let Some(kind) = TokenKind::ALL.iter().position(|k| *k == t.kind) else { continue };
        out.extend([utf16_col(&bounds, t.start), utf16_col(&bounds, t.end), kind as u32]);
    }
    out
}

#[derive(Serialize, Debug, PartialEq)]
pub struct JsFix {
    pub title: String,
    pub from: u32,
    pub to: u32,
    pub text: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct JsDiagnostic {
    /// 0-based
    pub line: u32,
    pub from: u32,
    pub to: u32,
    pub severity: &'static str,
    pub message: String,
    pub fix: Option<JsFix>,
}

pub fn diagnostic_list(text: &str) -> Vec<JsDiagnostic> {
    let lines: Vec<&str> = text.lines().collect();
    let doc = parse_document(text);
    doc.diagnostics
        .into_iter()
        .filter(|d| d.line >= 1)
        .map(|d| {
            let bounds = boundaries(lines.get(d.line - 1).copied().unwrap_or(""));
            let col = |c: usize| utf16_col(&bounds, c);
            JsDiagnostic {
                line: (d.line - 1) as u32,
                from: col(d.columns.0),
                to: col(d.columns.1),
                severity: match d.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                },
                message: d.message,
                fix: d.fix.map(|f| JsFix {
                    title: f.title,
                    from: col(f.columns.0),
                    to: col(f.columns.1),
                    text: f.text,
                }),
            }
        })
        .collect()
}

/// Diagnostics of the whole text as a JSON string:
/// `[{line, from, to, severity, message, fix}]`, see the contract.
#[wasm_bindgen]
pub fn diagnostics(text: &str) -> String {
    serde_json::to_string(&diagnostic_list(text)).unwrap_or_else(|_| "[]".into())
}

#[derive(Serialize, Debug, PartialEq)]
pub struct JsCompletion {
    pub label: String,
    /// `null` when the item has no detail
    pub detail: Option<String>,
    /// replace `[from, to)` (UTF-16, on the cursor's line) with `insert`
    pub from: u32,
    pub to: u32,
    pub insert: String,
}

pub fn completion_list(text: &str, line: u32, col: u32) -> Vec<JsCompletion> {
    let Some(pos) = position(text, line, col) else { return Vec::new() };
    let mut items = analysis::completions(text, pos);
    // the LSP orders by sort_text; the editor keeps the list's order
    items.sort_by(|a, b| a.sort_text.cmp(&b.sort_text));
    items
        .into_iter()
        .map(|item| {
            let (from, to, insert) = match item.text_edit {
                Some(CompletionTextEdit::Edit(e)) => (e.range.start.character, e.range.end.character, e.new_text),
                Some(CompletionTextEdit::InsertAndReplace(e)) => {
                    (e.replace.start.character, e.replace.end.character, e.new_text)
                }
                None => (pos.character, pos.character, item.insert_text.unwrap_or_else(|| item.label.clone())),
            };
            JsCompletion { label: item.label, detail: item.detail, from, to, insert }
        })
        .collect()
}

/// Completions at `line` (0-based) and `col` (UTF-16) as a JSON string:
/// `[{label, detail, from, to, insert}]`, best first.
#[wasm_bindgen]
pub fn completions(text: &str, line: u32, col: u32) -> String {
    serde_json::to_string(&completion_list(text, line, col)).unwrap_or_else(|_| "[]".into())
}

/// Hover text (markdown) at `line` (0-based) and `col` (UTF-16), if any.
#[wasm_bindgen]
pub fn hover(text: &str, line: u32, col: u32) -> Option<String> {
    analysis::hover(text, position(text, line, col)?)
}

/// Whether a sync must wait: a date line is half-typed, not ISO, or not a
/// real date.
#[wasm_bindgen]
pub fn unfinished_date(text: &str) -> bool {
    parse_document(text).has_unfinished_date()
}

/// The version of the language crate.
#[wasm_bindgen]
pub fn version() -> String {
    workout_syntax::VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundaries_count_utf16_units() {
        // ⁰ and º are one unit each, the emoji two
        assert_eq!(boundaries("a⁰º💪b"), vec![0, 1, 2, 3, 5, 6]);
        assert_eq!(utf16_col(&boundaries("💪"), 9), 2, "clamped to the end");
        assert_eq!(boundaries(""), vec![0]);
    }

    #[test]
    fn snap_never_splits_a_surrogate_pair() {
        assert_eq!(snap_utf16("💪x", 1), 0);
        assert_eq!(snap_utf16("💪x", 2), 2);
        assert_eq!(snap_utf16("💪x", 99), 3);
    }
}
