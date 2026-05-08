// Helper functions for TOON decoding — header/key/primitive/delimiter parsing.

use super::{Header, ToonError};
use serde_json::Value;

// ── Header parser ─────────────────────────────────────────────────────────────

pub(super) fn try_parse_header(s: &str) -> Option<Header> {
    // Grammar: [key] "[" N [sym] "]" ["{" fields "}"] ":" [" " inline]
    // Key: optional identifier or quoted string followed by "["
    let (key, rest) = split_key_bracket(s);

    // rest must start with "["
    let rest = rest.trim_start_matches('[');
    if rest.is_empty() {
        return None;
    }

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
        if r.is_empty() {
            None
        } else {
            Some(r.to_string())
        }
    } else if rest.is_empty() {
        None
    } else {
        return None; // garbage after ":"
    };

    Some(Header {
        key,
        count: n,
        delim,
        fields,
        inline_rest: inline,
    })
}

/// Split optional key from `"[...]..."` → `(Option<key>, rest_starting_with_"[")`
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
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '.')
        .unwrap_or(s.len());
    if end == 0 {
        return (None, s);
    }
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

pub(super) fn unescape_char(c: char) -> Option<char> {
    Some(match c {
        '"' => '"',
        '\\' => '\\',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        _ => return None,
    })
}

/// Parse optional delimiter symbol from start of rest (after N digits, before "]").
fn parse_delim_sym(s: &str) -> (char, &str) {
    if let Some(rest) = s.strip_prefix('\t') {
        ('\t', rest)
    } else if let Some(rest) = s.strip_prefix('|') {
        ('|', rest)
    } else {
        (',', s)
    }
}

// ── Key/value line parsing ────────────────────────────────────────────────────

/// Try to parse "key: rest" → Some((key, rest)). Returns None if no valid key found.
pub(super) fn parse_key_rest(s: &str) -> Option<(String, &str)> {
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
        if key_part.is_empty() || key_part.contains(' ') {
            return None;
        }
        let rest = &s[colon + 1..];
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        Some((decode_key(key_part), rest))
    }
}

/// Decode a key (strip quotes if needed). Keys from our encoder are already safe identifiers.
pub(super) fn decode_key(s: &str) -> String {
    if s.starts_with('"') {
        parse_quoted_key(s).map_or_else(|| s.to_string(), |(k, _)| k)
    } else {
        s.trim().to_string()
    }
}

// ── Primitive decoding ────────────────────────────────────────────────────────

/// Decode an unquoted or quoted token to a `Value` per §4.
///
/// # Errors
///
/// Returns `Err` if the input contains an invalid escape sequence or unterminated string.
pub(crate) fn decode_primitive(s: &str, lineno: usize) -> Result<Value, ToonError> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Value::String(String::new()));
    }
    // Quoted string.
    if s.starts_with('"') {
        return decode_quoted_string(s, lineno).map(Value::String);
    }
    // Boolean / null keywords (case-sensitive per spec).
    match s {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "null" => return Ok(Value::Null),
        _ => {}
    }
    // Numeric: try integer then float.
    if looks_numeric(s) {
        if let Ok(i) = s.parse::<i64>() {
            return Ok(Value::Number(i.into()));
        }
        if let Ok(u) = s.parse::<u64>() {
            return Ok(Value::Number(u.into()));
        }
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
    if s.is_empty() {
        return false;
    }
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
            Some((_, '\\')) => match chars.next() {
                Some((_, c)) => out.push(unescape_char(c).ok_or(ToonError::InvalidEscape(lineno))?),
                None => return Err(ToonError::InvalidEscape(lineno)),
            },
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
