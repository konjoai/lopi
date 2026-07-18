//! TOON — Token-Oriented Object Notation encoder/decoder.
//!
//! Encodes/decodes the JSON data model with ~40% fewer tokens than JSON.
//! Key features: tabular arrays, minimal quoting, indentation over braces.

mod decode;
mod encode;

pub use decode::{decode, decode_with, DecoderOptions, ToonError};
pub use encode::{encode, encode_task_context, encode_with, Delimiter, EncoderOptions};

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::approx_constant,
    clippy::needless_pass_by_value
)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn rt(v: Value) -> Value {
        decode(&encode(&v)).expect("round-trip failed")
    }

    // ── Primitives ──────────────────────────────────────────────────────────

    #[test]
    fn null_roundtrip() {
        assert_eq!(rt(json!(null)), json!(null));
    }

    #[test]
    fn bool_roundtrip() {
        assert_eq!(rt(json!(true)), json!(true));
        assert_eq!(rt(json!(false)), json!(false));
    }

    #[test]
    fn integer_roundtrip() {
        assert_eq!(rt(json!(42)), json!(42));
    }

    #[test]
    fn float_roundtrip() {
        assert_eq!(rt(json!(3.14)), json!(3.14));
    }

    #[test]
    fn string_roundtrip() {
        assert_eq!(rt(json!("hello")), json!("hello"));
        assert_eq!(rt(json!("")), json!(""));
        assert_eq!(rt(json!("has space")), json!("has space"));
    }

    // ── String quoting edge cases ────────────────────────────────────────────

    #[test]
    fn reserved_words_quoted() {
        assert_eq!(rt(json!("true")), json!("true"));
        assert_eq!(rt(json!("false")), json!("false"));
        assert_eq!(rt(json!("null")), json!("null"));
    }

    #[test]
    fn numeric_string_quoted() {
        assert_eq!(rt(json!("42")), json!("42"));
        assert_eq!(rt(json!("3.14")), json!("3.14"));
        assert_eq!(rt(json!("-7")), json!("-7"));
    }

    #[test]
    fn hyphen_string_quoted() {
        assert_eq!(rt(json!("-")), json!("-"));
    }

    #[test]
    fn string_with_colon_quoted() {
        assert_eq!(rt(json!("key:value")), json!("key:value"));
    }

    #[test]
    fn string_with_newline_quoted() {
        assert_eq!(rt(json!("line1\nline2")), json!("line1\nline2"));
    }

    // ── Objects ─────────────────────────────────────────────────────────────

    #[test]
    fn flat_object_roundtrip() {
        let v = json!({"name": "Alice", "age": 30, "active": true});
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn empty_object_roundtrip() {
        assert_eq!(rt(json!({})), json!({}));
    }

    #[test]
    fn nested_object_roundtrip() {
        let v = json!({"outer": {"inner": 42, "flag": false}});
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn deeply_nested_roundtrip() {
        let v = json!({"a": {"b": {"c": {"d": "deep"}}}});
        assert_eq!(rt(v.clone()), v);
    }

    // ── Arrays ──────────────────────────────────────────────────────────────

    #[test]
    fn primitive_array_roundtrip() {
        let v = json!({"tags": ["rust", "async", "wasm"]});
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn empty_array_roundtrip() {
        let v = json!({"items": []});
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn number_array_roundtrip() {
        let v = json!({"values": [1, 2, 3, 4, 5]});
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn bool_array_roundtrip() {
        let v = json!({"flags": [true, false, true]});
        assert_eq!(rt(v.clone()), v);
    }

    // ── Tabular arrays ───────────────────────────────────────────────────────

    #[test]
    fn tabular_array_roundtrip() {
        let v = json!({
            "hikes": [
                {"id": 1, "name": "Blue Lake Trail", "distanceKm": 7.5, "sunny": true},
                {"id": 2, "name": "Ridge Overlook",  "distanceKm": 9.2, "sunny": false},
                {"id": 3, "name": "Wildflower Loop", "distanceKm": 5.1, "sunny": true}
            ]
        });
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn tabular_with_nulls_roundtrip() {
        let v = json!({
            "users": [
                {"id": 1, "name": "Alice", "email": "alice@example.com"},
                {"id": 2, "name": "Bob",   "email": null}
            ]
        });
        assert_eq!(rt(v.clone()), v);
    }

    // ── Mixed arrays (expanded form) ─────────────────────────────────────────

    #[test]
    fn mixed_array_with_nested_objects_roundtrip() {
        let v = json!({
            "items": [
                {"name": "foo", "tags": ["a", "b"]},
                {"name": "bar", "tags": ["c"]}
            ]
        });
        assert_eq!(rt(v.clone()), v);
    }

    #[test]
    fn mixed_type_array_roundtrip() {
        let v = json!({"vals": [1, "two", true, null]});
        assert_eq!(rt(v.clone()), v);
    }

    /// Regression test: a list-item whose object-map iteration happens to
    /// put an object-valued field first used to corrupt the round trip —
    /// the encoder inlined the object onto the `- ` line, doubling its key
    /// and landing its children at the wrong depth, which the decoder then
    /// silently swallowed a sibling field into (or dropped entirely).
    #[test]
    fn list_item_with_object_first_field_roundtrips() {
        let v = json!({
            "items": [
                {"meta": {"a": 1}, "name": "x"}
            ]
        });
        assert_eq!(rt(v.clone()), v);
    }

    /// The item's only field happening to be an object is unambiguous (no
    /// sibling field to collide with) and must keep working.
    #[test]
    fn list_item_with_only_an_object_field_roundtrips() {
        let v = json!({"items": [{"meta": {"a": 1}}]});
        assert_eq!(rt(v.clone()), v);
    }

    /// A trailing line the root parse didn't consume (e.g. a stray `- ` list
    /// item after plain object fields, which doesn't belong to the object)
    /// used to be silently dropped, returning a truncated `Value` with no
    /// error. It must now surface as `ToonError::Unexpected`.
    #[test]
    fn unconsumed_trailing_line_is_an_error_not_silent_truncation() {
        let input = "a: 1\n- b\n";
        let err = decode(input).expect_err("trailing unconsumed line must error");
        assert!(matches!(err, ToonError::Unexpected(_)));
    }

    /// Multiple object-valued fields on one item, with a scalar field
    /// available to take the `- ` line — every object field ends up in the
    /// "remaining fields" path, which already handles nested objects
    /// correctly.
    #[test]
    fn list_item_with_multiple_object_fields_roundtrips() {
        let v = json!({
            "items": [
                {"meta": {"a": 1}, "extra": {"b": 2}, "name": "x"}
            ]
        });
        assert_eq!(rt(v.clone()), v);
    }

    // ── Full spec example ─────────────────────────────────────────────────────

    /// The TOON spec's canonical worked example — shared by the round-trip
    /// test and the exact-output-format test so the fixture has one source.
    fn spec_example() -> serde_json::Value {
        json!({
            "context": {
                "task": "Our favorite hikes together",
                "location": "Boulder",
                "season": "spring_2025"
            },
            "friends": ["ana", "luis", "sam"],
            "hikes": [
                {"id": 1, "name": "Blue Lake Trail", "distanceKm": 7.5,
                 "elevationGain": 320, "companion": "ana", "wasSunny": true},
                {"id": 2, "name": "Ridge Overlook", "distanceKm": 9.2,
                 "elevationGain": 540, "companion": "luis", "wasSunny": false},
                {"id": 3, "name": "Wildflower Loop", "distanceKm": 5.1,
                 "elevationGain": 180, "companion": "sam", "wasSunny": true}
            ]
        })
    }

    #[test]
    fn spec_example_roundtrip() {
        let v = spec_example();
        assert_eq!(rt(v.clone()), v);
    }

    // ── Token efficiency ──────────────────────────────────────────────────────

    #[test]
    fn toon_is_shorter_than_json_for_tabular_data() {
        let v = json!({
            "hikes": [
                {"id": 1, "name": "Blue Lake Trail", "distanceKm": 7.5, "sunny": true},
                {"id": 2, "name": "Ridge Overlook",  "distanceKm": 9.2, "sunny": false},
                {"id": 3, "name": "Wildflower Loop", "distanceKm": 5.1, "sunny": true}
            ]
        });
        let toon_len = encode(&v).len();
        let json_len = serde_json::to_string_pretty(&v).unwrap().len();
        assert!(
            toon_len < json_len,
            "TOON ({toon_len}) should be shorter than JSON ({json_len})"
        );
    }

    // ── Encode output format tests ────────────────────────────────────────────

    #[test]
    fn spec_example_encodes_correctly() {
        let v = spec_example();
        let out = encode(&v);
        // Key structural checks.
        assert!(out.contains("context:"), "should have context block");
        assert!(
            out.contains("  task: Our favorite hikes together"),
            "unquoted multiword value"
        );
        assert!(
            out.contains("friends[3]: ana,luis,sam"),
            "inline primitive array"
        );
        assert!(
            out.contains("hikes[3]{id,name,distanceKm,elevationGain,companion,wasSunny}:"),
            "tabular header"
        );
        assert!(
            out.contains("  1,Blue Lake Trail,7.5,320,ana,true"),
            "tabular row"
        );
    }

    #[test]
    fn empty_string_encodes_quoted() {
        let v = json!({"key": ""});
        let out = encode(&v);
        assert!(
            out.contains("key: \"\""),
            "empty string must be quoted: {out:?}"
        );
    }

    #[test]
    fn reserved_word_value_encodes_quoted() {
        let out = encode(&json!({"flag": "true"}));
        assert!(
            out.contains("\"true\""),
            "string 'true' must be quoted: {out:?}"
        );
    }

    #[test]
    fn number_roundtrip_no_trailing_zeros() {
        let v = json!({"x": 1.50});
        // 1.5, not 1.50
        assert_eq!(rt(v), json!({"x": 1.5}));
    }

    // ── Task encoding helper ──────────────────────────────────────────────────

    #[test]
    fn encode_task_context() {
        use encode::encode_task_context;
        let out = encode_task_context(
            "Fix the failing test",
            &["src/", "tests/"],
            &[".github/"],
            &["do not add deps"],
            &[],
            &[],
        );
        assert!(out.contains("goal: Fix the failing test"));
        assert!(out.contains("allowed[2]: src/,tests/"));
        assert!(out.contains("forbidden[1]: .github/"));
        assert!(out.contains("constraints[1]: do not add deps"));
    }
}
