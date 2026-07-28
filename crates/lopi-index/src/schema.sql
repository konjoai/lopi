-- lopi-index schema. Applied idempotently on every open() — see
-- store.rs::apply_schema, which mirrors lopi-memory's pattern exactly:
-- CREATE TABLE IF NOT EXISTS is naturally re-runnable, and future columns
-- land as an appended ALTER TABLE ... ADD COLUMN, whose duplicate-column
-- error on an already-migrated database is swallowed by apply_schema.
-- NOTE: apply_schema splits this file on a literal semicolon character
-- before stripping '--' comment lines per resulting chunk, so a comment
-- must never itself contain that character — it would split a comment
-- line in half and leak its back half into the next chunk as literal
-- (invalid) SQL.

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- One row per indexed file, independent of how many symbols it produced.
-- A file that parses to zero symbols (a re-export-only module, a data
-- file) still needs its hash tracked somewhere for the dirty-tree sweep to
-- tell "unchanged" from "changed" — recording the hash only on symbol rows
-- would make a zero-symbol file look dirty on every single incremental
-- pass, since it would never have a row to compare against.
CREATE TABLE IF NOT EXISTS files (
    repo_id TEXT NOT NULL,
    path TEXT NOT NULL,
    lang TEXT NOT NULL,
    file_hash TEXT NOT NULL,
    PRIMARY KEY (repo_id, path)
);

-- One row per named thing a parser found. `signature` is one line and never
-- contains a body. `file_hash` mirrors `files.file_hash` for this symbol's
-- own file at index time — kept here too since a caller resolving a single
-- symbol shouldn't need a second table join to see what it was hashed against.
CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    path TEXT NOT NULL,
    lang TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    qualified_name TEXT NOT NULL,
    signature TEXT NOT NULL,
    doc_first_line TEXT,
    parent_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
    line_start INTEGER NOT NULL,
    line_end INTEGER NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_end INTEGER NOT NULL,
    file_hash TEXT NOT NULL,
    is_public INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_symbols_repo_path ON symbols(repo_id, path);
CREATE INDEX IF NOT EXISTS idx_symbols_qname ON symbols(repo_id, qualified_name);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(repo_id, name);
CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(repo_id, kind);
-- FK-column-leading (not repo_id-leading): SQLite's ON DELETE SET NULL
-- cascade for symbols.parent_id issues a bare `WHERE parent_id = ?`, with
-- no repo_id predicate, so only an index whose *first* column is
-- parent_id is usable there. Without this, deleting a reindexed file's old
-- symbol rows degraded from an indexed point lookup to a full table scan
-- per deleted row — a one-file reindex on this repo went from a target of
-- under 150ms to over 1.2s. See LEDGER.md's Finding #4 entry.
CREATE INDEX IF NOT EXISTS idx_symbols_parent_id ON symbols(parent_id);

-- One row per reference edge. Resolution is best-effort: `to_symbol_id` is
-- NULL and `to_name` is kept whenever a callee can't be resolved uniquely —
-- an unresolved reference stored as text is more useful than a dropped one.
CREATE TABLE IF NOT EXISTS refs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id TEXT NOT NULL,
    from_symbol_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
    to_name TEXT NOT NULL,
    to_symbol_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
    path TEXT NOT NULL,
    line INTEGER NOT NULL
);

-- Query-shaped (repo_id-leading): what store/refs.rs's one_hop_callers/
-- callees actually filter by.
CREATE INDEX IF NOT EXISTS idx_refs_to_symbol ON refs(repo_id, to_symbol_id);
CREATE INDEX IF NOT EXISTS idx_refs_from_symbol ON refs(repo_id, from_symbol_id);
CREATE INDEX IF NOT EXISTS idx_refs_to_name ON refs(repo_id, to_name);
-- FK-column-leading: refs.from_symbol_id's ON DELETE CASCADE and
-- refs.to_symbol_id's ON DELETE SET NULL each issue a bare `WHERE
-- from_symbol_id = ?`/`WHERE to_symbol_id = ?` with no repo_id predicate —
-- same reasoning as idx_symbols_parent_id above.
CREATE INDEX IF NOT EXISTS idx_refs_from_symbol_fk ON refs(from_symbol_id);
CREATE INDEX IF NOT EXISTS idx_refs_to_symbol_fk ON refs(to_symbol_id);
