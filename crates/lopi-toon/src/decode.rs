// TOON decoder — §4–§14 of the spec.
// Parses TOON text back into serde_json::Value with full round-trip fidelity.

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
    WidthMismatch { expected: usize, found: usize, lineno: usize },
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
    fn default() -> Self { Self { indent: 2, strict: true } }
}

/// Decode TOON text into a `serde_json::Value` using default options.
pub fn decode(input: &str) -> Result<Value, ToonError> {
    decode_with(input, &DecoderOptions::default())
}

pub fn decode_with(input: &str, opts: &DecoderOptions) -> Result<Value, ToonError> {
    let lines = preprocess(input, opts.indent)?;
    if lines.is_empty() { return Ok(Value::Object(Map::new())); }
    let mut p = Parser { lines: &lines, pos: 0, opts };
    p.parse_root()
}

// ── Preprocessing ────────────────────────────────────────────────────────────

#[derive(Debug)]
struct Line {
    lineno: usize,   // 1-based original line number
    depth: usize,    // indent level
    content: String, // content after stripping indent
}

fn preprocess(input: &str, indent_size: usize) -> Result<Vec<Line>, ToonError> {
    let mut out = Vec::new();
    for (i, raw) in input.lines().enumerate() {
        let lineno = i + 1;
        // Count leading spaces.
        let leading = raw.len() - raw.trim_start_matches(' ').len();
        // Skip pure blank lines.
        if raw.trim().is_empty() { continue; }
        if leading % indent_size != 0 {
            return Err(ToonError::BadIndent { lineno, indent: indent_size });
        }
        let depth = leading / indent_size;
        let content = raw[leading..].to_string();
        out.push(Line { lineno, depth, content });
    }
    Ok(out)
}

// ── Parsed header ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Header {
    key: Option<String>,     // None for root arrays
    count: usize,
    delim: char,
    fields: Option<Vec<String>>, // Some for tabular
    // true if inline values follow on the same line (after ": ")
    inline_rest: Option<String>,
}

// ── Parser ────────────────────────────────────────────────────────────────────

struct Parser<'a> {
    lines: &'a [Line],
    pos: usize,
    opts: &'a DecoderOptions,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Line> { self.lines.get(self.pos) }
    fn next(&mut self) -> Option<&Line> {
        let l = self.lines.get(self.pos);
        self.pos += 1;
        l
    }

    fn parse_root(&mut self) -> Result<Value, ToonError> {
        // §5: root form discovery.
        // A ROOT array is signalled by a keyless header: `[N]:` or `[N]{fields}:`.
        // A header WITH a key (e.g. `tags[3]: a,b`) is a field of a root object.
        let first = &self.lines[0];
        if let Some(h) = try_parse_header(&first.content) {
            if h.key.is_none() {
                return self.parse_array_body(0, None);
            }
        }
        // Single primitive: one non-empty line that is neither a header nor key:value.
        if self.lines.len() == 1
            && try_parse_header(&first.content).is_none()
            && parse_key_rest(&first.content).is_none()
        {
            let val = decode_primitive(&first.content, first.lineno)?;
            self.pos += 1;
            return Ok(val);
        }
        // Otherwise: root object.
        self.parse_object_at(0)
    }

    // Parse an object whose fields start at `depth`.
    fn parse_object_at(&mut self, depth: usize) -> Result<Value, ToonError> {
        let mut map = Map::new();
        while let Some(line) = self.peek() {
            if line.depth != depth { break; }
            // List items at this depth don't belong to this object.
            if line.content.starts_with("- ") || line.content == "-" { break; }
            let lineno = line.lineno;
            let content = line.content.clone();

            // Is it an array header with a key?
            if let Some(h) = try_parse_header(&content) {
                let key = h.key.clone().ok_or(ToonError::MissingColon(lineno))?;
                self.pos += 1;
                let val = self.parse_array_body(depth + 1, Some(&h.clone()))?;
                map.insert(key, val);
                continue;
            }

            // Must be key: rest
            let (key, rest) = parse_key_rest(&content)
                .ok_or(ToonError::MissingColon(lineno))?;
            self.pos += 1;

            if rest.is_empty() {
                // Value is on subsequent lines.
                let val = if let Some(next) = self.peek() {
                    if next.depth == depth + 1 {
                        // Check if it's a nested array header or object.
                        if try_parse_header(&next.content).is_some() {
                            self.parse_array_body(depth + 1, None)?
                        } else if next.content.starts_with("- ") || next.content == "-" {
                            // list items without an enclosing array header — treat as expanded array
                            self.parse_list_items_at(depth + 1)?
                        } else {
                            self.parse_object_at(depth + 1)?
                        }
                    } else {
                        // Empty object.
                        Value::Object(Map::new())
                    }
                } else {
                    Value::Object(Map::new())
                };
                map.insert(key, val);
            } else {
                // Inline scalar value.
                let val = decode_primitive(rest.trim(), lineno)?;
                map.insert(key, val);
            }
        }
        Ok(Value::Object(map))
    }

    // Parse the body of an array (the header has already been consumed by caller).
    // `outer_header` is the parsed header, or None for root arrays that need re-parsing.
    fn parse_array_body(&mut self, depth: usize, outer: Option<&Header>) -> Result<Value, ToonError> {
        // Parse the header if we haven't yet (root array case).
        let header = if let Some(h) = outer {
            h.clone()
        } else {
            let line = self.next().ok_or(ToonError::Unexpected(0))?;
            let lineno = line.lineno;
            try_parse_header(&line.content).ok_or(ToonError::Unexpected(lineno))?
        };

        let count = header.count;

        // Empty array.
        if count == 0 { return Ok(Value::Array(vec![])); }

        // Inline primitive array: values are already in header.inline_rest
        if let Some(inline) = &header.inline_rest {
            if header.fields.is_none() {
                let vals = split_on_delim(inline, header.delim);
                if self.opts.strict && vals.len() != count {
                    return Err(ToonError::CountMismatch { declared: count, found: vals.len() });
                }
                let arr: Result<Vec<Value>, ToonError> = vals.iter().enumerate()
                    .map(|(i, s)| decode_primitive(s.trim(), i + 1))
                    .collect();
                return Ok(Value::Array(arr?));
            }
        }

        // Tabular array.
        if let Some(fields) = &header.fields {
            let fields = fields.clone();
            let delim = header.delim;
            let mut rows: Vec<Value> = Vec::new();
            while let Some(line) = self.peek() {
                if line.depth != depth { break; }
                let lineno = line.lineno;
                let content = line.content.clone();
                self.pos += 1;
                let cells = split_on_delim(&content, delim);
                if self.opts.strict && cells.len() != fields.len() {
                    return Err(ToonError::WidthMismatch {
                        expected: fields.len(), found: cells.len(), lineno,
                    });
                }
                let mut obj = Map::new();
                for (field, cell) in fields.iter().zip(cells.iter()) {
                    let key = decode_key(field);
                    let val = decode_primitive(cell.trim(), lineno)?;
                    obj.insert(key, val);
                }
                rows.push(Value::Object(obj));
            }
            if self.opts.strict && rows.len() != count {
                return Err(ToonError::CountMismatch { declared: count, found: rows.len() });
            }
            return Ok(Value::Array(rows));
        }

        // Expanded array — items start with "- ".
        let items = self.parse_list_items_at(depth)?;
        if let Value::Array(ref arr) = items {
            if self.opts.strict && arr.len() != count {
                return Err(ToonError::CountMismatch { declared: count, found: arr.len() });
            }
        }
        Ok(items)
    }

    // Parse list items (lines starting with "- ") at the given depth.
    fn parse_list_items_at(&mut self, depth: usize) -> Result<Value, ToonError> {
        let mut items: Vec<Value> = Vec::new();
        while let Some(l) = self.peek() {
            let (line_depth, line_content, lineno) = (l.depth, l.content.clone(), l.lineno);
            if line_depth != depth { break; }
            if !line_content.starts_with("- ") && line_content != "-" { break; }

            let rest_owned: String = if line_content == "-" {
                String::new()
            } else {
                line_content["- ".len()..].to_string()
            };
            let rest: &str = &rest_owned;
            self.pos += 1;

            if rest.is_empty() {
                // Empty object item.
                items.push(Value::Object(Map::new()));
                continue;
            }

            // Is the rest an array header?
            if let Some(h) = try_parse_header(rest) {
                if h.key.is_none() {
                    // Pure array item e.g. `- [3]: a,b,c`
                    let val = self.parse_array_body(depth + 1, Some(&h))?;
                    items.push(val);
                    continue;
                }
                // Keyed array as first field of an object item: e.g. `- tags[3]: a,b,c`
                let mut obj = Map::new();
                let key = h.key.clone().unwrap();
                let first_arr = self.parse_array_body(depth + 2, Some(&h))?;
                obj.insert(key, first_arr);
                // Remaining fields at depth+1
                while let Some(next) = self.peek() {
                    if next.depth != depth + 1 { break; }
                    if next.content.starts_with("- ") || next.content == "-" { break; }
                    let nc = next.content.clone();
                    let nl = next.lineno;
                    if let Some(h2) = try_parse_header(&nc) {
                        let k2 = h2.key.clone().ok_or(ToonError::MissingColon(nl))?;
                        self.pos += 1;
                        let av = self.parse_array_body(depth + 2, Some(&h2))?;
                        obj.insert(k2, av);
                    } else if let Some((k, r)) = parse_key_rest(&nc) {
                        self.pos += 1;
                        if r.is_empty() {
                            let v = self.parse_value_at(depth + 2, nl)?;
                            obj.insert(k, v);
                        } else {
                            obj.insert(k, decode_primitive(r.trim(), nl)?);
                        }
                    } else {
                        break;
                    }
                }
                items.push(Value::Object(obj));
                continue;
            }

            // Is it a key-value pair (object item)?
            if let Some((k, r)) = parse_key_rest(rest) {
                let mut obj = Map::new();
                let first_val = if r.is_empty() {
                    // Value at depth+1
                    self.parse_value_at(depth + 1, lineno)?
                } else {
                    decode_primitive(r.trim(), lineno)?
                };
                obj.insert(k, first_val);
                // More fields of this object at depth+1
                while let Some(next) = self.peek() {
                    if next.depth != depth + 1 { break; }
                    if next.content.starts_with("- ") || next.content == "-" { break; }
                    let nc = next.content.clone();
                    let nl = next.lineno;
                    if let Some(h2) = try_parse_header(&nc) {
                        let k2 = h2.key.clone().ok_or(ToonError::MissingColon(nl))?;
                        self.pos += 1;
                        let av = self.parse_array_body(depth + 2, Some(&h2))?;
                        obj.insert(k2, av);
                    } else if let Some((k2, r2)) = parse_key_rest(&nc) {
                        self.pos += 1;
                        let v = if r2.is_empty() {
                            self.parse_value_at(depth + 2, nl)?
                        } else {
                            decode_primitive(r2.trim(), nl)?
                        };
                        obj.insert(k2, v);
                    } else {
                        break;
                    }
                }
                items.push(Value::Object(obj));
                continue;
            }

            // Pure primitive item.
            items.push(decode_primitive(rest, lineno)?);
        }
        Ok(Value::Array(items))
    }

    // Parse a value at the given depth (used when `key:` has no inline value).
    fn parse_value_at(&mut self, depth: usize, _ctx: usize) -> Result<Value, ToonError> {
        if let Some(line) = self.peek() {
            if line.depth == depth {
                let content = line.content.clone();
                if let Some(h) = try_parse_header(&content) {
                    return self.parse_array_body(depth + 1, Some(&h));
                }
                if content.starts_with("- ") || content == "-" {
                    return self.parse_list_items_at(depth);
                }
                return self.parse_object_at(depth);
            }
        }
        Ok(Value::Object(Map::new()))
    }
}

// ── Header parser ─────────────────────────────────────────────────────────────

fn try_parse_header(s: &str) -> Option<Header> {
    // Grammar: [key] "[" N [sym] "]" ["{" fields "}"] ":" [" " inline]
    // Key: optional identifier or quoted string followed by "["
    let (key, rest) = split_key_bracket(s);

    // rest must start with "["
    let rest = rest.trim_start_matches('[');
    if rest.is_empty() { return None; }

    // Parse N
    let n_end = rest.find(|c: char| !c.is_ascii_digit())?;
    let n: usize = rest[..n_end].parse().ok()?;
    let rest = &rest[n_end..];

    // Optional delimiter symbol
    let (delim, rest) = parse_delim_sym(rest);

    // Must see "]"
    let rest = rest.strip_prefix(']')?;

    // Optional "{fields}"
    let (fields, rest) = if let Some(r) = rest.strip_prefix('{') {
        let end = r.find('}')?;
        let field_str = &r[..end];
        let fs: Vec<String> = split_on_delim(field_str, delim)
            .into_iter()
            .map(|f| decode_key(f.trim()))
            .collect();
        (Some(fs), &r[end + 1..])
    } else {
        (None, rest)
    };

    // Must end with ":"
    let rest = rest.strip_prefix(':')?;

    // Optional inline values after ": "
    let inline = if let Some(r) = rest.strip_prefix(' ') {
        if r.is_empty() { None } else { Some(r.to_string()) }
    } else if rest.is_empty() {
        None
    } else {
        return None; // garbage after ":"
    };

    Some(Header { key, count: n, delim, fields, inline_rest: inline })
}

/// Split optional key from "[...]..." → (Option<key>, rest_starting_with_"[")
fn split_key_bracket(s: &str) -> (Option<String>, &str) {
    if s.starts_with('[') {
        return (None, s);
    }
    // Try quoted key.
    if s.starts_with('"') {
        if let Some((k, rest)) = parse_quoted_key(s) {
            if rest.starts_with('[') {
                return (Some(k), rest);
            }
        }
        return (None, s);
    }
    // Unquoted key: [A-Za-z_][A-Za-z0-9_.]*
    let end = s.find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
        .unwrap_or(s.len());
    if end == 0 { return (None, s); }
    let key = &s[..end];
    let rest = &s[end..];
    if rest.starts_with('[') {
        (Some(key.to_string()), rest)
    } else {
        (None, s)
    }
}

fn parse_quoted_key(s: &str) -> Option<(String, &str)> {
    debug_assert!(s.starts_with('"'));
    let mut chars = s[1..].char_indices();
    let mut key = String::new();
    loop {
        let (i, c) = chars.next()?;
        match c {
            '"' => return Some((key, &s[i + 2..])),
            '\\' => {
                let (_, esc) = chars.next()?;
                key.push(unescape_char(esc)?);
            }
            c => key.push(c),
        }
    }
}

fn unescape_char(c: char) -> Option<char> {
    Some(match c { '"' => '"', '\\' => '\\', 'n' => '\n', 'r' => '\r', 't' => '\t', _ => return None })
}

/// Parse optional delimiter symbol from start of rest (after N digits, before "]").
fn parse_delim_sym(s: &str) -> (char, &str) {
    if let Some(rest) = s.strip_prefix('\t') { ('\t', rest) }
    else if let Some(rest) = s.strip_prefix('|') { ('|', rest) }
    else { (',', s) }
}

// ── Key/value line parsing ────────────────────────────────────────────────────

/// Try to parse "key: rest" → Some((key, rest)). Returns None if no valid key found.
fn parse_key_rest(s: &str) -> Option<(String, &str)> {
    if s.starts_with('"') {
        // Quoted key
        let (k, rest) = parse_quoted_key(s)?;
        let rest = rest.strip_prefix(':')?;
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        Some((k, rest))
    } else {
        // Unquoted key: must match ^[A-Za-z_][A-Za-z0-9_.]* (but we also allow any non-"["
        // identifier-like chars to handle keys produced by our encoder)
        let colon = s.find(':')?;
        let key_part = &s[..colon];
        // Key must not be empty and must not contain spaces.
        if key_part.is_empty() || key_part.contains(' ') { return None; }
        let rest = &s[colon + 1..];
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        Some((decode_key(key_part), rest))
    }
}

/// Decode a key (strip quotes if needed). Keys from our encoder are already safe identifiers.
fn decode_key(s: &str) -> String {
    if s.starts_with('"') {
        parse_quoted_key(s).map(|(k, _)| k).unwrap_or_else(|| s.to_string())
    } else {
        s.trim().to_string()
    }
}

// ── Primitive decoding ────────────────────────────────────────────────────────

/// Decode an unquoted or quoted token to a `Value` per §4.
pub(crate) fn decode_primitive(s: &str, lineno: usize) -> Result<Value, ToonError> {
    let s = s.trim();
    if s.is_empty() { return Ok(Value::String(String::new())); }
    // Quoted string.
    if s.starts_with('"') {
        return decode_quoted_string(s, lineno).map(Value::String);
    }
    // Boolean / null keywords (case-sensitive per spec).
    match s {
        "true"  => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "null"  => return Ok(Value::Null),
        _ => {}
    }
    // Numeric: try integer then float.
    if looks_numeric(s) {
        if let Ok(i) = s.parse::<i64>() { return Ok(Value::Number(i.into())); }
        if let Ok(u) = s.parse::<u64>() { return Ok(Value::Number(u.into())); }
        if let Ok(f) = s.parse::<f64>() {
            if let Some(n) = serde_json::Number::from_f64(f) {
                return Ok(Value::Number(n));
            }
            return Ok(Value::Null); // NaN/Inf → null per spec
        }
    }
    // Plain string.
    Ok(Value::String(s.to_string()))
}

fn looks_numeric(s: &str) -> bool {
    if s.is_empty() { return false; }
    let s2 = s.strip_prefix('-').unwrap_or(s);
    s2.starts_with(|c: char| c.is_ascii_digit())
}

fn decode_quoted_string(s: &str, lineno: usize) -> Result<String, ToonError> {
    debug_assert!(s.starts_with('"'));
    let mut out = String::new();
    let mut chars = s[1..].char_indices();
    loop {
        match chars.next() {
            None => return Err(ToonError::UnterminatedString(lineno)),
            Some((_, '"')) => break,
            Some((_, '\\')) => {
                match chars.next() {
                    Some((_, c)) => out.push(unescape_char(c)
                        .ok_or(ToonError::InvalidEscape(lineno))?),
                    None => return Err(ToonError::InvalidEscape(lineno)),
                }
            }
            Some((_, c)) => out.push(c),
        }
    }
    Ok(out)
}

// ── Delimiter splitting ───────────────────────────────────────────────────────

/// Split `s` on `delim`, respecting quoted strings.
pub(crate) fn split_on_delim(s: &str, delim: char) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut prev_backslash = false;
    for c in s.chars() {
        if prev_backslash {
            cur.push(c);
            prev_backslash = false;
            continue;
        }
        if c == '\\' && in_quote {
            prev_backslash = true;
            cur.push(c);
            continue;
        }
        if c == '"' {
            in_quote = !in_quote;
            cur.push(c);
            continue;
        }
        if !in_quote && c == delim {
            parts.push(cur.clone());
            cur.clear();
            continue;
        }
        cur.push(c);
    }
    parts.push(cur);
    parts
}
