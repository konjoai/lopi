//! P1.4 — Structured output schema validation.
//!
//! When a [`Task`](crate::Task) declares an `output_schema`, the agent's
//! parsed output (diff metadata, score JSON, anything the runner deserialises
//! from Claude) must satisfy that schema before being accepted.
//!
//! Two surfaces:
//!
//! * [`validate`] — pure validator. Pragmatic subset of JSON Schema:
//!   - `"type"`: object / string / number / integer / boolean / array / null
//!   - `"required"`: array of property names (only meaningful on objects)
//!   - `"properties"`: per-property subschemas (recursive)
//!   - `"enum"`: list of allowed literal values (deep-equality)
//!
//!   Anything else in the schema is permissive (ignored, not rejected) so
//!   stricter schemas degrade gracefully on this validator.
//!
//! * [`schema_violations_inc`] / [`schema_violations_snapshot`] — process-
//!   wide atomic counter exposed in Prometheus exposition as
//!   `lopi_schema_violations_total{kind="<label>"}`. The `kind` label
//!   distinguishes violation reasons so `/metrics` can plot trends per
//!   failure mode.
//!
//! This module is dep-free beyond `serde_json` to keep `lopi-core` at tier 1.
//! A heavier external `jsonschema` crate would also pull `regex` and `url`
//! into the dependency graph — not worth it for the four keywords lopi
//! actually consumes.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::sync::RwLock;

/// Reason a validation failed. Carried in [`Violation::kind`] and used as
/// the Prometheus label.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ViolationKind {
    /// `type` keyword mismatch.
    Type,
    /// A `required` property is missing.
    Required,
    /// `enum` keyword — value not in the allowed list.
    EnumMismatch,
    /// Nested property failed its subschema.
    Property,
}

impl ViolationKind {
    /// Stable label used in Prometheus exposition.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Required => "required",
            Self::EnumMismatch => "enum",
            Self::Property => "property",
        }
    }
}

/// One validation failure — carries enough detail for the next planning
/// prompt to learn from.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    pub kind: ViolationKind,
    /// Dotted JSON path: `""` for root, `"foo.bar"` for nested.
    pub path: String,
    pub message: String,
}

impl Violation {
    fn new(kind: ViolationKind, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            path: path.into(),
            message: message.into(),
        }
    }
}

/// Validate `value` against `schema`. Returns all violations, not just
/// the first — the next planning prompt benefits from a complete picture.
///
/// An empty schema (no `type`, `required`, `properties`, `enum`) accepts
/// any value. Unknown keywords are ignored.
#[must_use]
pub fn validate(value: &Value, schema: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    validate_at(value, schema, "", &mut out);
    out
}

fn validate_at(value: &Value, schema: &Value, path: &str, out: &mut Vec<Violation>) {
    let schema_obj = match schema.as_object() {
        Some(o) => o,
        None => return, // a non-object schema is treated as permissive
    };

    if let Some(ty) = schema_obj.get("type").and_then(Value::as_str) {
        if !type_matches(value, ty) {
            out.push(Violation::new(
                ViolationKind::Type,
                path,
                format!("expected type `{ty}`, got `{}`", json_kind(value)),
            ));
            // Return early — further checks against the wrong type just
            // produce noise.
            return;
        }
    }

    if let Some(allowed) = schema_obj.get("enum").and_then(Value::as_array) {
        if !allowed.iter().any(|a| a == value) {
            out.push(Violation::new(
                ViolationKind::EnumMismatch,
                path,
                format!("value not in enum (has {} variants)", allowed.len()),
            ));
        }
    }

    // The rest of the keywords only apply to objects.
    if let Some(obj) = value.as_object() {
        if let Some(required) = schema_obj.get("required").and_then(Value::as_array) {
            for r in required.iter().filter_map(Value::as_str) {
                if !obj.contains_key(r) {
                    out.push(Violation::new(
                        ViolationKind::Required,
                        path,
                        format!("missing required property `{r}`"),
                    ));
                }
            }
        }
        if let Some(props) = schema_obj.get("properties").and_then(Value::as_object) {
            for (k, subschema) in props {
                if let Some(v) = obj.get(k) {
                    let next_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    let before = out.len();
                    validate_at(v, subschema, &next_path, out);
                    // Re-label nested failures as Property so the operator
                    // knows where the cascade started.
                    for v in &mut out[before..] {
                        if v.kind == ViolationKind::Type
                            || v.kind == ViolationKind::Required
                            || v.kind == ViolationKind::EnumMismatch
                            || v.kind == ViolationKind::Property
                        {
                            // Keep the original kind for the leaf failure;
                            // the path already conveys the property.
                        }
                    }
                    if out.len() > before {
                        // Bubble up that this property was the source.
                        out.push(Violation::new(
                            ViolationKind::Property,
                            path,
                            format!("property `{k}` failed validation"),
                        ));
                    }
                }
            }
        }
    }
}

fn type_matches(value: &Value, ty: &str) -> bool {
    match ty {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        // JSON Schema integer: number with no fractional part.
        "integer" => {
            value.as_i64().is_some()
                || value.as_u64().is_some()
                || value
                    .as_f64()
                    .is_some_and(|f| f.is_finite() && f.fract() == 0.0)
        }
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true, // unknown type keyword → permissive
    }
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ──────────────────────────────────────────────────────────────────────
// Prometheus counter — process-wide, label-keyed.
// ──────────────────────────────────────────────────────────────────────

fn counters() -> &'static RwLock<HashMap<String, AtomicU64>> {
    static C: OnceLock<RwLock<HashMap<String, AtomicU64>>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Increment `lopi_schema_violations_total{kind="<kind>"}` by 1. Idempotent
/// across concurrent callers — uses an atomic per label.
pub fn schema_violations_inc(kind: ViolationKind) {
    let map = counters();
    let key = kind.as_str().to_string();

    // Fast path: read lock + existing entry. Recover the inner data on
    // poisoning so a panicked writer doesn't drop the counter permanently.
    {
        let r = map
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(c) = r.get(&key) {
            c.fetch_add(1, Ordering::Relaxed);
            return;
        }
    }
    // Slow path: insert the missing label.
    let mut w = map
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    w.entry(key)
        .or_insert_with(|| AtomicU64::new(0))
        .fetch_add(1, Ordering::Relaxed);
}

/// Snapshot every label → count. Drives Prometheus exposition; cheap.
#[must_use]
pub fn schema_violations_snapshot() -> Vec<(String, u64)> {
    let r = counters()
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    r.iter()
        .map(|(k, v)| (k.clone(), v.load(Ordering::Relaxed)))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_schema_accepts_anything() {
        assert!(validate(&json!({"x": 1}), &json!({})).is_empty());
        assert!(validate(&json!("hello"), &json!({})).is_empty());
        assert!(validate(&json!(null), &json!({})).is_empty());
    }

    #[test]
    fn type_mismatch_is_reported() {
        let v = validate(&json!("not a number"), &json!({"type": "number"}));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, ViolationKind::Type);
    }

    #[test]
    fn integer_accepts_whole_floats() {
        // 5.0 should validate as integer per JSON Schema semantics.
        let v = validate(&json!(5.0), &json!({"type": "integer"}));
        assert!(v.is_empty(), "5.0 should match `integer` type");
    }

    #[test]
    fn integer_rejects_fractional() {
        let v = validate(&json!(5.5), &json!({"type": "integer"}));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, ViolationKind::Type);
    }

    #[test]
    fn required_property_missing_is_reported() {
        let v = validate(
            &json!({"a": 1}),
            &json!({"type": "object", "required": ["b"]}),
        );
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, ViolationKind::Required);
        assert!(v[0].message.contains("`b`"));
    }

    #[test]
    fn enum_mismatch_is_reported() {
        let v = validate(&json!("yellow"), &json!({"enum": ["red", "green", "blue"]}));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].kind, ViolationKind::EnumMismatch);
    }

    #[test]
    fn enum_match_passes() {
        let v = validate(&json!("red"), &json!({"enum": ["red", "green"]}));
        assert!(v.is_empty());
    }

    #[test]
    fn nested_property_failure_cascades_with_path() {
        let schema = json!({
            "type": "object",
            "properties": {
                "score": { "type": "number" }
            }
        });
        let v = validate(&json!({"score": "not a number"}), &schema);
        // One leaf failure (type) plus one Property bubble-up.
        assert!(v
            .iter()
            .any(|x| x.kind == ViolationKind::Type && x.path == "score"));
        assert!(v.iter().any(|x| x.kind == ViolationKind::Property));
    }

    #[test]
    fn schema_violations_counter_increments() {
        // Counter is process-wide; use a fresh kind per test to avoid
        // depending on absolute totals.
        let before: u64 = schema_violations_snapshot()
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| *v)
            .unwrap_or(0);
        schema_violations_inc(ViolationKind::Type);
        let after: u64 = schema_violations_snapshot()
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| *v)
            .unwrap_or(0);
        assert!(after > before);
    }

    #[test]
    fn realistic_score_schema_validates() {
        // The kind of schema a Task might carry: scorer output shape.
        let schema = json!({
            "type": "object",
            "required": ["test_pass_rate", "lint_errors", "diff_lines"],
            "properties": {
                "test_pass_rate": { "type": "number" },
                "lint_errors":    { "type": "integer" },
                "diff_lines":     { "type": "integer" }
            }
        });
        let good = json!({"test_pass_rate": 0.95, "lint_errors": 0, "diff_lines": 42});
        assert!(validate(&good, &schema).is_empty());

        let bad = json!({"test_pass_rate": "n/a"});
        let v = validate(&bad, &schema);
        // Missing two required + one type failure on the bad field.
        assert!(
            v.iter()
                .filter(|x| x.kind == ViolationKind::Required)
                .count()
                >= 2
        );
        assert!(v
            .iter()
            .any(|x| x.kind == ViolationKind::Type && x.path == "test_pass_rate"));
    }
}
