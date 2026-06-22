//! Best-effort extraction of discrete plan steps from free-form plan text.

/// Recognises markdown bullets (`-`, `*`, `•`) and numbered lines (`1.`, `2)`),
/// trimming the marker. Empty when nothing looks step-like — the UI then falls
/// back to the full plan text.
pub(super) fn parse_plan_steps(plan: &str) -> Vec<String> {
    let mut steps = Vec::new();
    for raw in plan.lines() {
        let line = raw.trim();
        let stripped = line
            .strip_prefix("- ")
            .or_else(|| line.strip_prefix("* "))
            .or_else(|| line.strip_prefix("• "))
            .or_else(|| strip_numbered(line));
        if let Some(s) = stripped {
            let s = s.trim();
            if !s.is_empty() {
                steps.push(s.to_string());
            }
        }
        if steps.len() >= 20 {
            break;
        }
    }
    steps
}

/// Strip a leading `N.` / `N)` ordered-list marker, returning the remainder.
fn strip_numbered(line: &str) -> Option<&str> {
    let mut saw_digit = false;
    for (i, c) in line.char_indices() {
        if c.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        if saw_digit && (c == '.' || c == ')') {
            return Some(line[i + 1..].trim_start());
        }
        break;
    }
    None
}

#[cfg(test)]
mod plan_gate_tests {
    use super::parse_plan_steps;

    #[test]
    fn parses_markdown_bullets() {
        let plan = "Here is the plan:\n- Add the cache layer\n* Wire it in\n• Add tests";
        assert_eq!(
            parse_plan_steps(plan),
            vec!["Add the cache layer", "Wire it in", "Add tests"]
        );
    }

    #[test]
    fn parses_numbered_lists() {
        let plan = "1. First step\n2) Second step\n   3. Indented third";
        assert_eq!(
            parse_plan_steps(plan),
            vec!["First step", "Second step", "Indented third"]
        );
    }

    #[test]
    fn leading_punctuation_without_a_digit_is_not_a_step() {
        // `strip_numbered` only fires after seeing a digit — a line that starts
        // with `.` or `)` but no leading number must not be treated as a step.
        assert!(parse_plan_steps("Some prose\n.config = true").is_empty());
        assert!(parse_plan_steps(") closing paren line").is_empty());
    }

    #[test]
    fn ignores_prose_and_blank_markers() {
        // No bullets/numbers → empty, so the UI falls back to the full text.
        assert!(parse_plan_steps("Just a paragraph of prose.\n\nAnother line.").is_empty());
        // A bare marker with no content is skipped.
        assert!(parse_plan_steps("- \n*  ").is_empty());
    }

    #[test]
    fn caps_at_twenty_steps() {
        let plan: String = (0..50).map(|i| format!("- step {i}\n")).collect();
        assert_eq!(parse_plan_steps(&plan).len(), 20);
    }
}
