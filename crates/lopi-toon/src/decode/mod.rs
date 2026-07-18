// TOON decoder — §4–§14 of the spec.
// Parses TOON text back into serde_json::Value with full round-trip fidelity.

mod parser;
use parser::Parser;
use serde_json::{Map, Value};
use thiserror::Error;

/// Parse error returned by the TOON decoder.
#[derive(Debug, Error)]
pub enum ToonError {
    /// A key was not followed by the required colon separator.
    #[error("missing colon after key on line {0}")]
    MissingColon(usize),
    /// An unrecognised backslash escape sequence was encountered.
    #[error("invalid escape sequence on line {0}")]
    InvalidEscape(usize),
    /// A quoted string was opened but never closed.
    #[error("unterminated string on line {0}")]
    UnterminatedString(usize),
    /// The closing delimiter did not match the opening one.
    #[error("delimiter mismatch on line {0}")]
    DelimiterMismatch(usize),
    /// Indentation depth was not a multiple of the configured step.
    #[error("indentation not a multiple of {indent} on line {lineno}")]
    BadIndent {
        /// Line number (1-based) where the bad indent occurred.
        lineno: usize,
        /// The configured indent step size.
        indent: usize,
    },
    /// The array header declared a length that didn't match the element count.
    #[error("array count mismatch: declared {declared} but found {found}")]
    CountMismatch {
        /// Count written in the array header.
        declared: usize,
        /// Actual number of elements parsed.
        found: usize,
    },
    /// A tabular row had a different number of cells than the header declared.
    #[error("tabular row width mismatch: expected {expected} cells, got {found} on line {lineno}")]
    WidthMismatch {
        /// Number of columns from the header.
        expected: usize,
        /// Number of cells found in this row.
        found: usize,
        /// Line number (1-based) of the offending row.
        lineno: usize,
    },
    /// A line was encountered that the parser could not classify.
    #[error("unexpected line {0}")]
    Unexpected(usize),
}

/// Options controlling how TOON input is parsed.
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
    let value = p.parse_root()?;
    // A successful parse must consume every line. Trailing unconsumed lines
    // mean the structure didn't line up with what the parser expected (e.g.
    // a depth mismatch) — silently returning a truncated `Value` in that
    // case hides real corruption rather than surfacing it as an error.
    if let Some(line) = p.lines.get(p.pos) {
        return Err(ToonError::Unexpected(line.lineno));
    }
    Ok(value)
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
