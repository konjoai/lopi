//! Planning-seed gathering — pulls similar past patterns, lessons, and the
//! cached spec surface from memory before the first attempt, so the planning
//! prompt starts warm.

use super::AgentRunner;
use lopi_spec::SpecSurface;

/// Constraints, patterns, and lessons injected into the planning prompt.
pub(super) struct PlanningSeed {
    /// Flat constraint strings (legacy list) merged into the planning prompt.
    pub extra_constraints: Vec<String>,
    /// `(keywords, constraints)` pairs rendered as a TOON tabular block.
    pub pattern_pairs: Vec<(String, String)>,
    /// `(category, content)` lesson rows from the lessons table.
    pub lessons_data: Vec<(String, String)>,
    /// Top spec-surface item descriptions, injected as constraints.
    pub spec_constraints: Vec<String>,
}

impl AgentRunner {
    /// Gather planning seed material from memory + the cached spec surface.
    ///
    /// Side effect: populates `self.task_lessons` from the loaded lessons so
    /// the direct-API planning path can reuse them.
    pub(super) async fn gather_seed(&mut self) -> PlanningSeed {
        // Site 2 (TOON biggest win): PatternRow[] is a uniform tabular array.
        // encode_task_context() in claude.rs renders it as TOON §9.3 tabular,
        // saving ~158 tokens per attempt vs JSON (grows with pattern count).
        let (extra_constraints, pattern_pairs, lessons_data) = self.seed_from_patterns().await;

        // Store lessons for use in the API planning path.
        self.task_lessons = lessons_data
            .iter()
            .map(|(_, content)| content.clone())
            .collect();

        // Load spec surface if cached — inject top 10 items as planning constraints.
        let spec_constraints: Vec<String> = match SpecSurface::load(&self.repo_path) {
            Ok(Some(surface)) if !surface.is_empty() => {
                self.log(format!("📋 spec surface: {} items loaded", surface.len()));
                surface.top_descriptions(10)
            }
            _ => vec![],
        };

        PlanningSeed {
            extra_constraints,
            pattern_pairs,
            lessons_data,
            spec_constraints,
        }
    }

    /// Pull similar past patterns + lessons from the store. Returns
    /// `(constraints, (keywords, constraints) pairs, (category, content) lessons)`.
    async fn seed_from_patterns(
        &self,
    ) -> (Vec<String>, Vec<(String, String)>, Vec<(String, String)>) {
        let Some(store) = &self.store else {
            return (vec![], vec![], vec![]);
        };
        let patterns = match store.find_similar_patterns(&self.task.goal).await {
            Ok(patterns) if !patterns.is_empty() => patterns,
            _ => return (vec![], vec![], vec![]),
        };
        self.log(format!(
            "🧠 seeding from {} similar past patterns",
            patterns.len()
        ));

        let constraints: Vec<String> = patterns
            .iter()
            .take(5)
            .filter_map(|p| non_empty_constraint(p.successful_constraints.as_deref()))
            .collect();
        let pairs: Vec<(String, String)> = patterns
            .iter()
            .take(5)
            .filter_map(|p| {
                non_empty_constraint(p.successful_constraints.as_deref())
                    .map(|c| (p.goal_keywords.clone(), c))
            })
            .collect();

        let lessons = match store
            .load_lessons(self.repo_path.to_string_lossy().as_ref(), 10)
            .await
        {
            Ok(rows) => rows
                .into_iter()
                .map(|row| (row.category, row.content))
                .collect(),
            Err(e) => {
                self.warn(format!("failed to load lessons: {e}"));
                vec![]
            }
        };

        (constraints, pairs, lessons)
    }
}

/// Return an owned copy of `c` when it is present and non-empty.
fn non_empty_constraint(c: Option<&str>) -> Option<String> {
    c.and_then(|c| (!c.is_empty()).then(|| c.to_string()))
}

#[cfg(test)]
mod tests {
    use super::non_empty_constraint;

    #[test]
    fn keeps_non_empty_drops_empty_and_none() {
        assert_eq!(non_empty_constraint(Some("x")), Some("x".to_string()));
        assert_eq!(non_empty_constraint(Some("")), None);
        assert_eq!(non_empty_constraint(None), None);
    }
}
