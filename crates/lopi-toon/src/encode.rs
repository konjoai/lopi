use std::fmt::Write as _;

use serde_json::{Map, Number, Value};

// ── Public API ───────────────────────────────────────────────────────────────

/// Delimiter used in arrays and tabular rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Delimiter {
    #[default]
    Comma,
    Tab,
    Pipe,
}

impl Delimiter {
    pub(crate) fn ch(self) -> char {
        match self {
            Self::Comma => ',',
            Self::Tab => '\t',
            Self::Pipe => '|',
        }
    }
    /// The symbol that appears inside `[N<sym>]` — empty for comma (default).
    pub(crate) fn header_sym(self) -> &'static str {
        match self {
            Self::Comma => "",
            Self::Tab => "\t",
            Self::Pipe => "|",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncoderOptions {
    /// Spaces per indent level (default 2).
    pub indent: usize,
    /// Array and tabular delimiter (default comma).
    pub delimiter: Delimiter,
}

impl Default for EncoderOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            delimiter: Delimiter::Comma,
        }
    }
}

/// Encode a `serde_json::Value` as TOON with default options.
#[must_use]
pub fn encode(value: &Value) -> String {
    encode_with(value, &EncoderOptions::default())
}

/// Encode with explicit options.
#[must_use]
pub fn encode_with(value: &Value, opts: &EncoderOptions) -> String {
    let mut enc = Encoder {
        opts,
        buf: String::new(),
    };
    enc.write_root(value);
    enc.buf
}

/// Convenience: encode lopi task context as a TOON string for Claude prompts.
/// Returns a compact structured representation of the planning context.
#[must_use]
pub fn encode_task_context(
    goal: &str,
    allowed: &[&str],
    forbidden: &[&str],
    constraints: &[&str],
    patterns: &[(String, String)], // (keywords, constraints) from memory
) -> String {
    let allowed_v: Vec<Value> = allowed
        .iter()
        .map(|s| Value::String(s.to_string()))
        .collect();
    let forbidden_v: Vec<Value> = forbidden
        .iter()
        .map(|s| Value::String(s.to_string()))
        .collect();
    let constraints_v: Vec<Value> = constraints
        .iter()
        .map(|s| Value::String(s.to_string()))
        .collect();

    let mut map = serde_json::Map::new();
    map.insert("goal".into(), Value::String(goal.to_string()));
    map.insert("allowed".into(), Value::Array(allowed_v));
    map.insert("forbidden".into(), Value::Array(forbidden_v));
    if !constraints_v.is_empty() {
        map.insert("constraints".into(), Value::Array(constraints_v));
    }
    if !patterns.is_empty() {
        let rows: Vec<Value> = patterns
            .iter()
            .map(|(kw, c)| {
                let mut o = serde_json::Map::new();
                o.insert("keywords".into(), Value::String(kw.clone()));
                o.insert("constraints".into(), Value::String(c.clone()));
                Value::Object(o)
            })
            .collect();
        map.insert("patterns".into(), Value::Array(rows));
    }

    encode(&Value::Object(map))
}

// ── Internal encoder ─────────────────────────────────────────────────────────

struct Encoder<'a> {
    opts: &'a EncoderOptions,
    buf: String,
}

impl Encoder<'_> {
    fn indent(&self, depth: usize) -> String {
        " ".repeat(depth * self.opts.indent)
    }

    fn write_root(&mut self, value: &Value) {
        // §5: root form discovery
        match value {
            Value::Object(map) => self.write_object_fields(map, 0),
            Value::Array(arr) => self.write_array_root(arr),
            other => {
                self.buf
                    .push_str(&encode_scalar_value(other, self.opts.delimiter));
                self.buf.push('\n');
            }
        }
    }

    fn write_object_fields(&mut self, map: &Map<String, Value>, depth: usize) {
        for (key, val) in map {
            self.write_field(key, val, depth);
        }
    }

    fn write_field(&mut self, key: &str, value: &Value, depth: usize) {
        let indent = self.indent(depth);
        let k = encode_key(key);
        match value {
            Value::Object(map) => {
                self.buf.push_str(&indent);
                self.buf.push_str(&k);
                self.buf.push_str(":\n");
                self.write_object_fields(map, depth + 1);
            }
            Value::Array(arr) => {
                self.write_named_array(&indent, &k, arr, depth);
            }
            scalar => {
                let v = encode_scalar_value(scalar, self.opts.delimiter);
                self.buf.push_str(&indent);
                self.buf.push_str(&k);
                self.buf.push_str(": ");
                self.buf.push_str(&v);
                self.buf.push('\n');
            }
        }
    }

    fn write_named_array(&mut self, indent: &str, key: &str, arr: &[Value], depth: usize) {
        let n = arr.len();
        let sym = self.opts.delimiter.header_sym();
        let d = self.opts.delimiter;

        if n == 0 {
            self.buf.push_str(indent);
            self.buf.push_str(key);
            writeln!(self.buf, "[0{sym}]:").ok();
            return;
        }

        // Case 1: all primitives → inline CSV
        if arr.iter().all(is_primitive) {
            let vals: Vec<String> = arr.iter().map(|v| encode_cell(v, d)).collect();
            let inline = vals.join(&d.ch().to_string());
            self.buf.push_str(indent);
            self.buf.push_str(key);
            writeln!(self.buf, "[{n}{sym}]: {inline}").ok();
            return;
        }

        // Case 2: tabular — all objects, same keys, all primitive values
        if let Some(fields) = tabular_fields(arr) {
            let field_str: Vec<String> = fields.iter().map(|f| encode_key(f)).collect();
            let fields_hdr = field_str.join(&d.ch().to_string());
            self.buf.push_str(indent);
            self.buf.push_str(key);
            writeln!(self.buf, "[{n}{sym}]{{{fields_hdr}}}:").ok();
            let row_indent = " ".repeat((depth + 1) * self.opts.indent);
            for item in arr {
                if let Some(obj) = item.as_object() {
                    let cells: Vec<String> = fields
                        .iter()
                        .map(|f| encode_cell(obj.get(f.as_str()).unwrap_or(&Value::Null), d))
                        .collect();
                    self.buf.push_str(&row_indent);
                    self.buf.push_str(&cells.join(&d.ch().to_string()));
                    self.buf.push('\n');
                }
            }
            return;
        }

        // Case 3: expanded (mixed or objects with nested arrays/objects)
        self.buf.push_str(indent);
        self.buf.push_str(key);
        writeln!(self.buf, "[{n}{sym}]:").ok();
        for item in arr {
            self.write_list_item(item, depth + 1);
        }
    }

    fn write_array_root(&mut self, arr: &[Value]) {
        let n = arr.len();
        let sym = self.opts.delimiter.header_sym();
        let d = self.opts.delimiter;

        if n == 0 {
            writeln!(self.buf, "[0{sym}]:").ok();
            return;
        }
        if arr.iter().all(is_primitive) {
            let vals: Vec<String> = arr.iter().map(|v| encode_cell(v, d)).collect();
            writeln!(self.buf, "[{n}{sym}]: {}", vals.join(&d.ch().to_string())).ok();
            return;
        }
        if let Some(fields) = tabular_fields(arr) {
            let field_str: Vec<String> = fields.iter().map(|f| encode_key(f)).collect();
            let fields_hdr = field_str.join(&d.ch().to_string());
            writeln!(self.buf, "[{n}{sym}]{{{fields_hdr}}}:").ok();
            for item in arr {
                if let Some(obj) = item.as_object() {
                    let cells: Vec<String> = fields
                        .iter()
                        .map(|f| encode_cell(obj.get(f.as_str()).unwrap_or(&Value::Null), d))
                        .collect();
                    writeln!(self.buf, "  {}", cells.join(&d.ch().to_string())).ok();
                }
            }
            return;
        }
        writeln!(self.buf, "[{n}{sym}]:").ok();
        for item in arr {
            self.write_list_item(item, 1);
        }
    }

    fn write_list_item(&mut self, item: &Value, depth: usize) {
        let indent = self.indent(depth);
        // "- " prefix for list items (spec §9.4 and §10)
        match item {
            Value::Object(map) if map.is_empty() => {
                self.buf.push_str(&indent);
                self.buf.push_str("-\n");
            }
            Value::Object(map) => {
                // First field on the `- ` line; remainder indented one more.
                let mut entries = map.iter();
                let Some((fk, fv)) = entries.next() else {
                    return;
                };
                let k0 = encode_key(fk);
                // Write first field on the `- ` line.
                self.buf.push_str(&indent);
                self.buf.push_str("- ");
                match fv {
                    Value::Array(arr) => {
                        // Named array starting on the `- ` line
                        let sym = self.opts.delimiter.header_sym();
                        let d = self.opts.delimiter;
                        let n = arr.len();
                        if n == 0 {
                            writeln!(self.buf, "{k0}[0{sym}]:").ok();
                        } else if arr.iter().all(is_primitive) {
                            let vals: Vec<String> = arr.iter().map(|v| encode_cell(v, d)).collect();
                            writeln!(self.buf, "{k0}[{n}{sym}]: {}", vals.join(&d.ch().to_string())).ok();
                        } else {
                            // Complex array as first field: emit header on `- ` line
                            writeln!(self.buf, "{k0}[{n}{sym}]:").ok();
                            // rows / items go at depth+2
                            for sub in arr {
                                self.write_list_item(sub, depth + 2);
                            }
                        }
                    }
                    Value::Object(_) => {
                        writeln!(self.buf, "{k0}:").ok();
                        self.write_field(fk, fv, depth + 2);
                    }
                    scalar => {
                        let v = encode_scalar_value(scalar, self.opts.delimiter);
                        writeln!(self.buf, "{k0}: {v}").ok();
                    }
                }
                // Remaining fields at depth+1.
                let extra_indent = self.indent(depth + 1);
                for (k, v) in entries {
                    let ek = encode_key(k);
                    match v {
                        Value::Array(_) => {
                            // write_named_array (called inside write_field_no_indent) pushes
                            // its own indent from `depth`; don't double-push here.
                            self.write_field_no_indent(k, v, &extra_indent, depth + 1);
                        }
                        Value::Object(_) => {
                            self.buf.push_str(&extra_indent);
                            self.write_field_no_indent(k, v, &extra_indent, depth + 1);
                        }
                        scalar => {
                            let sv = encode_scalar_value(scalar, self.opts.delimiter);
                            writeln!(self.buf, "{extra_indent}{ek}: {sv}").ok();
                        }
                    }
                }
            }
            Value::Array(arr) => {
                // Array as list item: `- [N]: ...`
                let sym = self.opts.delimiter.header_sym();
                let d = self.opts.delimiter;
                let n = arr.len();
                self.buf.push_str(&indent);
                self.buf.push_str("- ");
                if n == 0 {
                    writeln!(self.buf, "[0{sym}]:").ok();
                } else if arr.iter().all(is_primitive) {
                    let vals: Vec<String> = arr.iter().map(|v| encode_cell(v, d)).collect();
                    writeln!(self.buf, "[{n}{sym}]: {}", vals.join(&d.ch().to_string())).ok();
                } else {
                    writeln!(self.buf, "[{n}{sym}]:").ok();
                    for sub in arr {
                        self.write_list_item(sub, depth + 2);
                    }
                }
            }
            scalar => {
                let v = encode_scalar_value(scalar, self.opts.delimiter);
                writeln!(self.buf, "{indent}- {v}").ok();
            }
        }
    }

    // Used when indent is already computed; writes field from an object in a list context.
    fn write_field_no_indent(&mut self, key: &str, value: &Value, _indent: &str, depth: usize) {
        let k = encode_key(key);
        match value {
            Value::Object(map) => {
                self.buf.push_str(&k);
                self.buf.push_str(":\n");
                self.write_object_fields(map, depth + 1);
            }
            Value::Array(arr) => {
                let n_indent = " ".repeat((depth) * self.opts.indent);
                self.write_named_array(&n_indent, &k, arr, depth);
            }
            scalar => {
                let v = encode_scalar_value(scalar, self.opts.delimiter);
                writeln!(self.buf, "{k}: {v}").ok();
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Returns the list of field names if `arr` qualifies for tabular encoding.
/// Conditions: all elements are objects with the same set of keys, all values primitive.
fn tabular_fields(arr: &[Value]) -> Option<Vec<String>> {
    if arr.is_empty() {
        return None;
    }
    let first = arr[0].as_object()?;
    let keys: Vec<String> = first.keys().cloned().collect();
    for item in arr {
        let obj = item.as_object()?;
        // Same keys in same order.
        if obj.keys().cloned().collect::<Vec<_>>() != keys {
            return None;
        }
        // All values are primitives.
        for v in obj.values() {
            if !is_primitive(v) {
                return None;
            }
        }
    }
    Some(keys)
}

fn is_primitive(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

/// Encode a scalar value for use after `key: `.
fn encode_scalar_value(v: &Value, delim: Delimiter) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::Number(n) => normalize_number(n),
        Value::String(s) => quote_if_needed(s, delim, false),
        _ => unreachable!("encode_scalar_value called on non-scalar"),
    }
}

/// Encode a cell for inline arrays and tabular rows (comma-/tab-/pipe-delimited context).
/// In this context, the value must also be quoted if it contains the active delimiter.
fn encode_cell(v: &Value, delim: Delimiter) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => {
            if *b {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::Number(n) => normalize_number(n),
        Value::String(s) => quote_if_needed(s, delim, true),
        _ => unreachable!("encode_cell called on non-scalar"),
    }
}

/// Encode a key per §7.3 — unquoted if it matches `^[A-Za-z_][A-Za-z0-9_.]*$`.
pub(crate) fn encode_key(k: &str) -> String {
    if key_is_safe(k) {
        k.into()
    } else {
        quote_string(k)
    }
}

fn key_is_safe(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => false,
        Some(first) if !first.is_ascii_alphabetic() && first != '_' => false,
        Some(_) => chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'),
    }
}

/// Determine if a string value must be quoted per §7.2.
fn quote_if_needed(s: &str, delim: Delimiter, in_array: bool) -> String {
    if must_quote(s, delim, in_array) {
        quote_string(s)
    } else {
        s.into()
    }
}

fn must_quote(s: &str, delim: Delimiter, in_array: bool) -> bool {
    // §7.2 conditions
    if s.is_empty() {
        return true;
    }
    if s.starts_with(' ') || s.ends_with(' ') {
        return true;
    }
    if matches!(s, "true" | "false" | "null") {
        return true;
    }
    // Numeric-like: matches number pattern or leading zeros
    if is_numeric_like(s) {
        return true;
    }
    // Starts with or equals "-"
    if s.starts_with('-') {
        return true;
    }
    // Contains forbidden chars
    for ch in s.chars() {
        match ch {
            ':' | '"' | '\\' | '[' | ']' | '{' | '}' | '\n' | '\r' | '\t' => return true,
            _ => {}
        }
        // Contains the document delimiter for object field values.
        if ch == delim.ch() && !in_array {
            return true;
        }
    }
    // Contains the active array delimiter.
    if in_array && s.contains(delim.ch()) {
        return true;
    }
    false
}

fn is_numeric_like(s: &str) -> bool {
    // Matches JSON number pattern or leading-zero numbers.
    // Uses a simple state machine rather than regex.
    let b = s.as_bytes();
    if b.is_empty() {
        return false;
    }
    let mut i = 0;
    if b[i] == b'-' {
        i += 1;
    }
    if i >= b.len() {
        return false;
    }
    // Leading zeros: "05" etc.
    if b.len() > i + 1 && b[i] == b'0' && b[i + 1].is_ascii_digit() {
        return true;
    }
    // Digits
    if !b[i].is_ascii_digit() {
        return false;
    }
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        if i >= b.len() || !b[i].is_ascii_digit() {
            return false;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        if i >= b.len() || !b[i].is_ascii_digit() {
            return false;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    i == b.len()
}

/// Wrap `s` in double quotes, escaping `\`, `"`, `\n`, `\r`, `\t` per §7.1.
fn quote_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Normalize a JSON number to canonical decimal form (§2):
/// no exponent notation, no trailing fractional zeros, no leading zeros.
pub(crate) fn normalize_number(n: &Number) -> String {
    // Use the f64 representation and reformat.
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    if let Some(f) = n.as_f64() {
        if f.is_nan() || f.is_infinite() {
            return "null".into();
        }
        if f == 0.0 {
            return "0".into();
        }
        // Format with enough precision, then strip trailing zeros.
        let s = format!("{f:.15}");
        strip_trailing_zeros(&s)
    } else {
        n.to_string()
    }
}

fn strip_trailing_zeros(s: &str) -> String {
    if !s.contains('.') {
        return s.into();
    }
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    s.into()
}
