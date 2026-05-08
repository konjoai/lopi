use lopi_memory::store::PatternRow;

/// Enriches task prompts with lessons from historical pattern data.
///
/// Before running a new task the agent can call [`PatternEnricher::enrich`] with
/// the set of [`PatternRow`] records returned by
/// [`lopi_memory::MemoryStore::find_similar_patterns`]. The returned string is
/// prepended to the Claude Code system prompt so the model is aware of
/// constraints that worked well on similar past tasks.
pub struct PatternEnricher {
    /// Only consider patterns whose success rate is at or above this threshold.
    /// Must be in `[0.0, 1.0]`.
    pub min_success_rate: f64,
    /// Maximum number of pattern entries to include in the prefix string.
    pub max_suggestions: usize,
}

impl PatternEnricher {
    /// Create a new enricher.
    ///
    /// # Panics
    /// Panics if `min_success_rate` is outside `[0.0, 1.0]` or `max_suggestions`
    /// is zero — these are programming errors, not runtime conditions.
    pub fn new(min_success_rate: f64, max_suggestions: usize) -> Self {
        assert!(
            (0.0..=1.0).contains(&min_success_rate),
            "min_success_rate must be in [0.0, 1.0], got {min_success_rate}"
        );
        assert!(max_suggestions > 0, "max_suggestions must be > 0");
        Self {
            min_success_rate,
            max_suggestions,
        }
    }

    /// Extract the top constraint suggestions from patterns relevant to `goal`.
    ///
    /// Filtering rules (applied in order):
    /// 1. Drop patterns whose `success_rate` is `None` or below
    ///    [`Self::min_success_rate`].
    /// 2. Require at least one whitespace-delimited keyword from
    ///    `pattern.goal_keywords` to appear as a substring in `goal`
    ///    (case-insensitive).
    /// 3. Sort remaining patterns by `success_rate` descending.
    /// 4. Truncate to [`Self::max_suggestions`].
    ///
    /// Returns an empty string when no patterns match, so callers can append
    /// without special-casing.
    pub fn enrich(&self, goal: &str, patterns: &[PatternRow]) -> String {
        let goal_lower = goal.to_lowercase();

        let mut relevant: Vec<&PatternRow> = patterns
            .iter()
            .filter(|p| {
                p.success_rate
                    .map(|sr| sr >= self.min_success_rate)
                    .unwrap_or(false)
            })
            .filter(|p| {
                let kw = p.goal_keywords.to_lowercase();
                kw.split_whitespace().any(|w| goal_lower.contains(w))
            })
            .collect();

        relevant.sort_by(|a, b| {
            let sr_a = a.success_rate.unwrap_or(0.0);
            let sr_b = b.success_rate.unwrap_or(0.0);
            sr_b.partial_cmp(&sr_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        relevant.truncate(self.max_suggestions);

        if relevant.is_empty() {
            return String::new();
        }

        let mut lines = vec!["Past lessons for similar tasks:".to_string()];
        for p in &relevant {
            let sr = p.success_rate.unwrap_or(0.0);
            let constraint_suffix = p
                .successful_constraints
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| format!(", constraints: {s}"))
                .unwrap_or_default();
            lines.push(format!(
                "  - [{}] success_rate={:.0}%{}",
                p.goal_keywords,
                sr * 100.0,
                constraint_suffix
            ));
        }
        // Trailing blank line makes it easy to concatenate with the real prompt.
        lines.push(String::new());
        lines.join("\n")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn make_row(goal_keywords: &str, success_rate: Option<f64>) -> PatternRow {
        PatternRow {
            id: uuid::Uuid::new_v4().to_string(),
            goal_keywords: goal_keywords.to_string(),
            successful_constraints: None,
            avg_attempts: Some(1.0),
            success_rate,
            last_seen: "2025-01-01T00:00:00Z".to_string(),
            derived_from_postmortem: 0,
        }
    }

    #[test]
    fn test_enricher_empty_patterns() {
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("fix the auth module", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_enricher_filters_by_success_rate() {
        let patterns = vec![
            make_row("auth", Some(0.3)),          // below threshold
            make_row("auth refactor", Some(0.8)), // above threshold
        ];
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("fix the auth module", &patterns);
        assert!(
            result.contains("auth refactor"),
            "high-rate pattern should appear"
        );
        let entry_lines: Vec<&str> = result
            .lines()
            .filter(|l| l.trim_start().starts_with("- ["))
            .collect();
        assert_eq!(
            entry_lines.len(),
            1,
            "only one pattern should survive the filter"
        );
    }

    #[test]
    fn test_enricher_filters_by_keyword_relevance() {
        let patterns = vec![
            make_row("auth", Some(0.9)),
            make_row("database migration", Some(0.9)),
        ];
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("fix auth issue", &patterns);
        assert!(result.contains("auth"));
        assert!(!result.contains("migration"));
    }

    #[test]
    fn test_enricher_max_suggestions() {
        let patterns: Vec<PatternRow> = (0..10)
            .map(|i| make_row("auth", Some(0.9 - i as f64 * 0.01)))
            .collect();
        let e = PatternEnricher::new(0.5, 3);
        let result = e.enrich("fix auth", &patterns);
        let count = result.matches("- [").count();
        assert!(count <= 3, "expected at most 3 suggestions, got {count}");
    }

    #[test]
    fn test_enricher_no_match_returns_empty() {
        let patterns = vec![make_row("database", Some(0.9))];
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("fix the frontend CSS", &patterns);
        assert!(result.is_empty());
    }

    #[test]
    fn test_enricher_sorted_by_success_rate() {
        let patterns = vec![
            make_row("auth refactor", Some(0.6)),
            make_row("auth security", Some(0.9)),
        ];
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("auth issue", &patterns);
        let pos_high = result.find("90%").expect("90% should be in output");
        let pos_low = result.find("60%").expect("60% should be in output");
        assert!(
            pos_high < pos_low,
            "higher success rate should appear first"
        );
    }

    #[test]
    fn test_enricher_prefix_format() {
        let patterns = vec![make_row("auth", Some(0.8))];
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("fix auth bug", &patterns);
        assert!(
            result.starts_with("Past lessons"),
            "output should start with the header line"
        );
    }

    #[test]
    fn test_enricher_none_success_rate_excluded() {
        let patterns = vec![make_row("auth", None)];
        let e = PatternEnricher::new(0.0, 5);
        let result = e.enrich("auth fix", &patterns);
        assert!(
            result.is_empty(),
            "pattern with None success_rate should be excluded"
        );
    }

    #[test]
    fn test_enricher_zero_min_success_rate() {
        let patterns = vec![make_row("auth", Some(0.01))];
        let e = PatternEnricher::new(0.0, 5);
        let result = e.enrich("auth fix", &patterns);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_enricher_trailing_newline() {
        let patterns = vec![make_row("auth", Some(0.8))];
        let e = PatternEnricher::new(0.5, 5);
        let result = e.enrich("fix auth bug", &patterns);
        assert!(result.ends_with('\n'));
    }
}
