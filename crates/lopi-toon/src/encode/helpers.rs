use super::Delimiter;
use serde_json::{Number, Value};

/// Returns the list of field names if `arr` qualifies for tabular encoding.
/// Conditions: all elements are objects with the same set of keys, all values primitive.
pub(super) fn tabular_fields(arr: &[Value]) -> Option<Vec<String>> {
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

pub(super) fn is_primitive(v: &Value) -> bool {
    !matches!(v, Value::Object(_) | Value::Array(_))
}

/// Shared scalar encoder for both contexts below. `in_cell` controls whether
/// a string must also be quoted when it contains the active delimiter
/// (inline arrays/tabular rows), vs. only when it needs quoting after `key: `.
fn encode_scalar_common(v: &Value, delim: Delimiter, in_cell: bool) -> String {
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
        Value::String(s) => quote_if_needed(s, delim, in_cell),
        _ => unreachable!("encode_scalar_common called on non-scalar"),
    }
}

/// Encode a scalar value for use after `key: `.
pub(super) fn encode_scalar_value(v: &Value, delim: Delimiter) -> String {
    encode_scalar_common(v, delim, false)
}

/// Encode a cell for inline arrays and tabular rows (comma-/tab-/pipe-delimited context).
/// In this context, the value must also be quoted if it contains the active delimiter.
pub(super) fn encode_cell(v: &Value, delim: Delimiter) -> String {
    encode_scalar_common(v, delim, true)
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
pub(super) fn quote_if_needed(s: &str, delim: Delimiter, in_array: bool) -> String {
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
        // A trailing dot with no fractional digits ("1.") isn't valid JSON
        // number syntax, but `decode_primitive`'s fallback (`str::parse::
        // <f64>`) is more lenient than JSON and accepts it anyway — so an
        // unquoted "1." would silently decode back as the float 1.0 instead
        // of the original string. Treat it as numeric-like (i.e. requiring
        // quoting) to match what the decoder actually does, not just the
        // strict grammar.
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
pub(super) fn quote_string(s: &str) -> String {
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
        // `f64`'s `Display` produces the shortest decimal string that parses
        // back to the exact same value, with no exponent notation — unlike a
        // fixed `{:.15}` truncation, it never rounds a subnormal-ish nonzero
        // value (e.g. 1e-16) down to a silently-wrong "0". Reaching this
        // branch at all means `n.as_i64()`/`as_u64()` were `None`, i.e. the
        // source JSON number was itself float-typed (`serde_json` only
        // returns `None` there for a `Number::from_f64` value) — so a whole
        // number like 2.0 must keep a decimal point on the way out, or the
        // decoder (which tries `i64`/`u64` before `f64`) reads it back as an
        // integer and the float/int distinction is lost on round-trip.
        let s = f.to_string();
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    } else {
        n.to_string()
    }
}
