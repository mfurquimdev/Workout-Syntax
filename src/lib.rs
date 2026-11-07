//! Parser for the plain-text workout log syntax.
pub mod ast;
pub mod cardio;
pub mod dates;
pub mod document;
pub mod expand;
pub mod merge;
pub mod parser;
pub mod removals;
#[cfg(feature = "native")]
pub mod server;
pub mod session;
#[cfg(feature = "native")]
pub mod sync;
pub mod tokens;

pub use ast::*;
pub use document::{parse_document, Diagnostic, Document, Entry, Fix, Session, SessionTime, Severity, MAX_SESSION_MINUTES};
pub use expand::{ExpandError, MAX_SETS};
pub use parser::{parse_line, parse_line_with_warnings, LineWarning, ParseError, MAX_NUMBER, MAX_REPEATS, MAX_REPS};
pub use tokens::{tokenize_line, Token, TokenKind};
pub mod lsp;

/// The crate version, for the web app's about line and bug reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
