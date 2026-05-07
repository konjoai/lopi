mod helpers;
use helpers::{encode_cell, encode_key, encode_scalar_value, is_primitive, tabular_fields};
use std::fmt::Write as _;

use serde_json::{Map, Value};

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
