//! Row types shared across `lopi-index` — the `symbols`/`refs` schema
//! (`schema.rs`) mapped to Rust, plus the `Language`/`SymbolKind` closed
//! enums every parser (`parse/*.rs`) and query (`query.rs`) shares.

use serde::{Deserialize, Serialize};

/// A parseable source language. Adding a grammar means: a new variant here,
/// one arm each in [`Language::from_path`] and `parse::parse_file`, and a
/// new `parse/<lang>.rs` implementing that language's symbol/ref extraction
/// — not a refactor of this module or the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    /// Rust (`.rs`).
    Rust,
    /// TypeScript (`.ts`, `.tsx`).
    TypeScript,
    /// JavaScript (`.js`, `.jsx`, `.mjs`).
    JavaScript,
    /// Python (`.py`).
    Python,
    /// Go (`.go`).
    Go,
}

impl Language {
    /// The wire/storage tag stored in `symbols.lang`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Go => "go",
        }
    }

    /// Parse a wire tag back into a [`Language`]. `None` for anything else.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "rust" => Some(Self::Rust),
            "typescript" => Some(Self::TypeScript),
            "javascript" => Some(Self::JavaScript),
            "python" => Some(Self::Python),
            "go" => Some(Self::Go),
            _ => None,
        }
    }

    /// Detect a language from a file's extension. `None` for anything this
    /// index doesn't parse — the caller skips such files rather than erroring.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit('.').next()?;
        match ext {
            "rs" => Some(Self::Rust),
            "ts" | "tsx" => Some(Self::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "py" | "pyi" => Some(Self::Python),
            "go" => Some(Self::Go),
            _ => None,
        }
    }
}

/// The kind of a named symbol. A closed set (not an open string) so a typo'd
/// kind fails at parse time in each `parse/*.rs`, not silently at query time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    /// A free function.
    Fn,
    /// A function bound to a type (inside an `impl`/`class`/method-with-receiver).
    Method,
    /// A struct / record type.
    Struct,
    /// An enum type.
    Enum,
    /// A trait / interface / protocol.
    Trait,
    /// An `impl` block (Rust-specific container; the type it implements for).
    Impl,
    /// A class (TS/JS/Python container).
    Class,
    /// A constant or immutable module-level binding.
    Const,
    /// A type alias.
    Type,
    /// A module / package / namespace.
    Module,
}

impl SymbolKind {
    /// The wire/storage tag stored in `symbols.kind`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fn => "fn",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Class => "class",
            Self::Const => "const",
            Self::Type => "type",
            Self::Module => "module",
        }
    }

    /// Parse a wire tag back into a [`SymbolKind`]. `None` for anything else.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fn" => Some(Self::Fn),
            "method" => Some(Self::Method),
            "struct" => Some(Self::Struct),
            "enum" => Some(Self::Enum),
            "trait" => Some(Self::Trait),
            "impl" => Some(Self::Impl),
            "class" => Some(Self::Class),
            "const" => Some(Self::Const),
            "type" => Some(Self::Type),
            "module" => Some(Self::Module),
            _ => None,
        }
    }

    /// Whether this kind counts as a "container" — the public-surface API is
    /// interested in it as a namespace even when it has no signature of its
    /// own worth showing (used by `map.rs` to group by top-level module).
    #[must_use]
    pub fn is_container(self) -> bool {
        matches!(self, Self::Impl | Self::Class | Self::Module | Self::Trait)
    }
}

/// One row of the `symbols` table — a named, located, single-line-signature
/// thing a parser found. Never carries a function/type body — `signature` is
/// deliberately one line, normalized whitespace, no bodies ever.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Symbol {
    /// Row id (assigned on insert; `0` for a not-yet-inserted symbol).
    pub id: i64,
    /// Stable per-repo identifier the index was built for.
    pub repo_id: String,
    /// Repo-relative path (forward-slash, no leading `./`).
    pub path: String,
    /// Source language.
    pub lang: Language,
    /// The kind of thing this is.
    pub kind: SymbolKind,
    /// Bare name (e.g. `run`).
    pub name: String,
    /// Fully-qualified name (e.g. `runner::AgentRunner::run`).
    pub qualified_name: String,
    /// One-line, normalized-whitespace declaration signature — never a body.
    pub signature: String,
    /// First line of the symbol's doc comment, if any.
    pub doc_first_line: Option<String>,
    /// Enclosing symbol's id, if any (e.g. a method's `impl`/`class`).
    pub parent_id: Option<i64>,
    /// 1-based inclusive start line.
    pub line_start: u32,
    /// 1-based inclusive end line.
    pub line_end: u32,
    /// 0-based inclusive start byte offset.
    pub byte_start: u32,
    /// 0-based exclusive end byte offset.
    pub byte_end: u32,
    /// BLAKE3 hex hash of the whole file's contents at index time.
    pub file_hash: String,
    /// Whether this symbol is part of the crate/package/module's public surface.
    pub is_public: bool,
}

/// One row of the `refs` table — a best-effort reference edge from one
/// location to a callee name, resolved to a symbol id where possible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ref {
    /// Row id (assigned on insert; `0` for a not-yet-inserted ref).
    pub id: i64,
    /// Stable per-repo identifier the index was built for.
    pub repo_id: String,
    /// The symbol this reference occurs inside, if any (`None` = file scope).
    pub from_symbol_id: Option<i64>,
    /// The callee's textual name as written at the call site — kept even
    /// when resolution fails, per the brief: an unresolved reference stored
    /// as text is more useful than a dropped one.
    pub to_name: String,
    /// The resolved target symbol id, when exactly one repo-wide candidate matched.
    pub to_symbol_id: Option<i64>,
    /// Repo-relative path the reference occurs in.
    pub path: String,
    /// 1-based line the reference occurs on.
    pub line: u32,
}

/// A symbol as a parser emits it, before insertion has assigned a row id.
/// `local_id`/`local_parent` are indices into the same file's `Vec<NewSymbol>`
/// (not database ids) — `store::insert_file` resolves `local_parent` to the
/// real assigned `parent_id` after inserting every symbol in the file, since
/// a child can only learn its parent's database id once the parent row exists.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSymbol {
    /// This symbol's index within the file's symbol list.
    pub local_id: usize,
    /// The enclosing symbol's `local_id` within the same file, if any.
    pub local_parent: Option<usize>,
    /// Source language.
    pub lang: Language,
    /// The kind of thing this is.
    pub kind: SymbolKind,
    /// Bare name.
    pub name: String,
    /// Fully-qualified name.
    pub qualified_name: String,
    /// One-line, normalized-whitespace declaration signature.
    pub signature: String,
    /// First line of the symbol's doc comment, if any.
    pub doc_first_line: Option<String>,
    /// 1-based inclusive start line.
    pub line_start: u32,
    /// 1-based inclusive end line.
    pub line_end: u32,
    /// 0-based inclusive start byte offset.
    pub byte_start: u32,
    /// 0-based exclusive end byte offset.
    pub byte_end: u32,
    /// Whether this symbol is part of the file's public surface.
    pub is_public: bool,
}

/// A reference as a parser emits it, before insertion has assigned a row id.
/// `from_local_id` mirrors [`NewSymbol::local_id`] — resolved to a real
/// `from_symbol_id` at insert time, same as `local_parent` above.
#[derive(Debug, Clone, PartialEq)]
pub struct NewRef {
    /// The enclosing symbol's `local_id` within the same file, if any
    /// (`None` = a file-scope reference, e.g. a top-level macro call).
    pub from_local_id: Option<usize>,
    /// The callee's textual name as written at the call site.
    pub to_name: String,
    /// 1-based line the reference occurs on.
    pub line: u32,
}

/// Aggregate counters from one (re)index pass — surfaced to the caller so
/// `lopi index` (and the LEDGER measurement) can report real deltas rather
/// than "it ran".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDelta {
    /// Files parsed this pass (changed + new).
    pub files_indexed: usize,
    /// Files removed from the index because the path no longer exists.
    pub files_removed: usize,
    /// Symbol rows inserted.
    pub symbols_added: usize,
    /// Symbol rows deleted (superseded by a reparse, or the file was removed).
    pub symbols_removed: usize,
    /// Ref rows inserted.
    pub refs_added: usize,
    /// Ref rows deleted.
    pub refs_removed: usize,
    /// Files that failed to parse and were skipped (never fatal).
    pub parse_failures: usize,
}
