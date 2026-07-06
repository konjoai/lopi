//! P1.4 — structured-output schema gate. Validates the scorer's JSON
//! projection against a task's optional `output_schema`, incrementing the
//! process-wide `lopi_schema_violations_total{kind=…}` counter and producing
//! a retry-ready summary. Split out of `run_loop.rs` to keep that module
//! under the file-size gate.

use lopi_core::Score;

/// Validate `score`'s projection against `schema`.
///
/// Returns `None` when the score satisfies the schema. On a violation,
/// increments `lopi_schema_violations_total` for each unmet criterion and
/// returns `Some((count, detail))` — `detail` is one `- kind@path: message`
/// line per violation, ready to append to a warn/retry message.
pub(super) fn violation_summary(
    schema: &serde_json::Value,
    score: &Score,
) -> Option<(usize, String)> {
    let score_json = serde_json::json!({
        "test_pass_rate": score.test_pass_rate,
        "lint_errors": score.lint_errors,
        "diff_lines": score.diff_lines,
    });
    let violations = lopi_core::validate_schema(&score_json, schema);
    if violations.is_empty() {
        return None;
    }
    for v in &violations {
        lopi_core::schema_violations_inc(v.kind.clone());
    }
    let detail = violations
        .iter()
        .map(|v| format!("- {}@{}: {}", v.kind.as_str(), v.path, v.message))
        .collect::<Vec<_>>()
        .join("\n");
    Some((violations.len(), detail))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lopi_core::Score;

    fn score_with(pass_rate: f32, lint: u32, diff: u32) -> Score {
        Score::new(pass_rate, lint, diff)
    }

    #[test]
    fn schema_satisfied_returns_none() {
        let schema = serde_json::json!({"type": "object"});
        let score = score_with(1.0, 0, 10);
        assert!(violation_summary(&schema, &score).is_none());
    }

    #[test]
    fn schema_violation_reports_count_and_detail() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["nonexistent_field"],
        });
        let score = score_with(1.0, 0, 10);
        let (count, detail) = violation_summary(&schema, &score).unwrap();
        assert_eq!(count, 1);
        assert!(detail.contains("nonexistent_field"));
    }
}
