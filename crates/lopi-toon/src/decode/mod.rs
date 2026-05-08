// TOON decoder — §4–§14 of the spec.
// Parses TOON text back into serde_json::Value with full round-trip fidelity.

mod parser;
use parser::Parser;
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToonError {
    #[error("missing colon after key on line {0}")]
    MissingColon(usize),
    #[error("invalid escape sequence on line {0}")]
    InvalidEscape(usize),
    #[error("unterminated string on line {0}")]
    UnterminatedString(usize),
    #[error("delimiter mismatch on line {0}")]
    DelimiterMismatch(usize),
    #[error("indentation not a multiple of {indent} on line {lineno}")]
    BadIndent { lineno: usize, indent: usize },
    #[error("array count mismatch: declared {declared} but found {found}")]
    CountMismatch { declared: usize, found: usize },
    #[error("tabular row width mismatch: expected {expected} cells, got {found} on line {lineno}")]
    WidthMismatch {
        expected: usize,
        found: usize,
        lineno: usize,
    },
    #[error("unexpected line {0}")]
    Unexpected(usize),
}

#[derive(Debug, Clone)]
pub struct DecoderOptions {
    /// Spaces per indent level (default 2).
    pub indent: usize,
    /// Strict mode: enforce count checks, indentation, no blank lines inside arrays.
    pub strict: bool,
}

impl Default for DecoderOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            strict: true,
        }
    }
}

/// Decode TOON text into a `serde_json::Value` using default options.
///
/// # Errors
/// Returns `Err` if the input contains malformed TOON syntax (bad indent, invalid escapes, etc.).
pub fn decode(input: &str) -> Result<Value, ToonError> {
    decode_with(input, &DecoderOptions::default())
}

/// Decode TOON text into a `serde_json::Value` using the provided options.
///
/// # Errors
/// Returns `Err` if the input contains malformed TOON syntax (bad indent, invalid escapes, etc.).
pub fn decode_with(input: &str, opts: &DecoderOptions) -> Result<Value, ToonError> {
    let lines = preprocess(input, opts.indent)?;
    if lines.is_empty() {
        return Ok(Value::Object(Map::new()));
    }
    let mut p = Parser {
        lines: &lines,
        pos: 0,
        opts,
    };
    p.parse_root()
}

// ── Preprocessing ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub(super) struct Line {
    pub(super) lineno: usize,   // 1-based original line number
    pub(super) depth: usize,    // indent level
    pub(super) content: String, // content after stripping indent
}

fn preprocess(input: &str, indent_size: usize) -> Result<Vec<Line>, ToonError> {
    let mut out = Vec::new();
    for (i, raw) in input.lines().enumerate() {
        let lineno = i + 1;
        // Count leading spaces.
        let leading = raw.len() - raw.trim_start_matches(' ').len();
        // Skip pure blank lines.
        if raw.trim().is_empty() {
            continue;
        }
        if leading % indent_size != 0 {
            return Err(ToonError::BadIndent {
                lineno,
                indent: indent_size,
            });
        }
        let depth = leading / indent_size;
        let content = raw[leading..].to_string();
        out.push(Line {
            lineno,
            depth,
            content,
        });
    }
    Ok(out)
}

// ── Parsed header ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub(super) struct Header {
    pub(super) key: Option<String>, // None for root arrays
    pub(super) count: usize,
    pub(super) delim: char,
    pub(super) fields: Option<Vec<String>>, // Some for tabular
    // true if inline values follow on the same line (after ": ")
    pub(super) inline_rest: Option<String>,
}

mod helpers;
use helpers::{decode_key, decode_primitive, parse_key_rest, split_on_delim, try_parse_header};
