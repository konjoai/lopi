//! Planning-seed gathering — pulls similar past patterns, lessons, and the
//! cached spec surface from memory before the first attempt, so the planning
//! prompt starts warm.

use super::AgentRunner;
use lopi_memory::PatternRow;
use lopi_spec::SpecSurface;

/// Constraint-Capture-2 promotion gate — minimum occurrences a mined
/// (non-postmortem) pattern needs before `seed_from_patterns` treats its
/// constraint as a reusable template rather than one-off noise. `2` is the
/// smallest value that means "this has recurred," not "this happened once" —
/// the mission this gate exists for: without it, every completed task's
/// mined pattern is equally weighted with one seen dozens of times.
const MIN_PATTERN_OCCURRENCES: i64 = 2;

/// Constraint-Capture-2 promotion gate — minimum rolling `success_rate` a
/// mined (non-postmortem) pattern needs before its constraint is injected.
/// `0.5` is a bare majority: seeding a constraint whose own history tips
/// more failure than success would inject noise, not guidance.
///
/// Both thresholds are deliberately conservative starting points, not
/// derived from a real usage corpus — this repo has none to tune against
/// yet (Session Prompt 2's own KT-D found the same absence for the
/// similarity threshold). Revisit once real mined-pattern data accumulates;
/// see `LEDGER.md`'s Constraint-Capture-2 entry for why these are a
/// deliberate one-way door, not a number to casually retune.
const MIN_PATTERN_SUCCESS_RATE: f64 = 0.5;

/// Whether a pattern's constraint has earned a place in the planning
/// prompt. Postmortem-derived patterns (`derived_from_postmortem == 1`) are
/// exempt from both thresholds: they are already a single, deliberately
/// curated failure lesson (`insert_postmortem_pattern` has always treated
/// one signal as enough), and a fresh postmortem pattern starts at
/// `success_rate = 0.0` by construction — applying the success-rate floor
/// to it would silently un-seed the exact warning postmortem mining exists
/// to inject. Pure, so it's unit-testable without a store.
fn is_promotable(p: &PatternRow) -> bool {
    if p.derived_from_postmortem != 0 {
        return true;
    }
    p.occurrence_count >= MIN_PATTERN_OCCURRENCES
        && p.success_rate.unwrap_or(0.0) >= MIN_PATTERN_SUCCESS_RATE
}

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
        let (mut extra_constraints, pattern_pairs, lessons_data) = self.seed_from_patterns().await;

        // Pentad M2.2 — inject skills whose triggers match the goal (and record
        // each activation in the audit trail). Appends nothing when none match.
        extra_constraints.extend(self.seed_skills().await);

        // A2 (reflection) — inject relevance-filtered, bounded durable learnings
        // from prior rejected attempts. No-op unless cross-run reflection is
        // enabled; hard-capped so irrelevant/unbounded context (the §2 failure
        // mode) can't bloat the prompt.
        extra_constraints.extend(self.seed_reflection_learnings().await);

        // Sprint I — seed the stability gate's consensus plan (if one was
        // computed by `run_stability_preflight`) as a planning constraint,
        // so the samples generated to gate stability also inform the real
        // plan instead of being discarded once their variance is scored.
        // `take()` — this is attempt 0's seed only, not re-injected on retry.
        if let Some(consensus_plan) = self.consensus_plan_hint.take() {
            extra_constraints.push(consensus_plan_constraint(&consensus_plan));
        }

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
            .filter(|p| is_promotable(p))
            .take(5)
            .filter_map(|p| non_empty_constraint(p.successful_constraints.as_deref()))
            .collect();
        let pairs: Vec<(String, String)> = patterns
            .iter()
            .filter(|p| is_promotable(p))
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

/// Hard cap on how many durable learnings enter the planning prompt. Bounded +
/// relevant is the whole A2 discipline: the §2 kill-test punishes unbounded or
/// irrelevant injection, so prefer the few most relevant over "just in case".
const REFLECTION_INJECTION_CAP: usize = 3;

impl AgentRunner {
    /// A2 (reflection) — pull the most relevant durable learnings for this task's
    /// goal and render them as bounded planning-prompt constraints.
    ///
    /// Empty unless cross-run reflection is enabled and a store is wired.
    /// Retrieval is relevance-filtered + deduped + capped in
    /// [`find_relevant_learnings`](lopi_memory::MemoryStore::find_relevant_learnings);
    /// a non-matching goal returns (near-)nothing rather than dumping recent
    /// history into context.
    pub(super) async fn seed_reflection_learnings(&self) -> Vec<String> {
        if !self.reflect_cross_run {
            return vec![];
        }
        let Some(store) = &self.store else {
            return vec![];
        };
        let learnings = match store
            .find_relevant_learnings(
                self.repo_path.to_string_lossy().as_ref(),
                &self.task.goal,
                REFLECTION_INJECTION_CAP,
            )
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                self.warn(format!("reflection: failed to retrieve learnings: {e}"));
                return vec![];
            }
        };
        if learnings.is_empty() {
            return vec![];
        }
        self.log(format!(
            "🪞 injecting {} relevant past learning(s)",
            learnings.len()
        ));
        learnings
            .iter()
            .map(|l| reflection_constraint(&l.critique))
            .collect()
    }

    /// Match skills relevant to the goal, record each activation in the audit
    /// trail, and return their bodies as planning-prompt constraints.
    ///
    /// Only skills whose triggers fire are returned, so a goal that matches
    /// nothing injects nothing — no context bloat. The audit row (`skill.activated`)
    /// is what satisfies "the skill shows up in the task's trail".
    pub(super) async fn seed_skills(&self) -> Vec<String> {
        let relevant = self.skills.relevant_to(&self.task.goal);
        if relevant.is_empty() {
            return vec![];
        }
        let names: Vec<&str> = relevant.iter().map(|s| s.name.as_str()).collect();
        self.log(format!(
            "📚 injecting {} skill(s): {}",
            relevant.len(),
            names.join(", ")
        ));
        for skill in &relevant {
            self.record_skill_activation(skill).await;
        }
        skill_constraint_blocks(&relevant)
    }

    /// Record a single skill activation in the audit trail (best-effort).
    async fn record_skill_activation(&self, skill: &lopi_skill::Skill) {
        let Some(store) = &self.store else {
            return;
        };
        let payload = format!(
            "{{\"skill\":\"{}\",\"version\":\"{}\"}}",
            skill.name, skill.version
        );
        let input = lopi_memory::AuditInput::new("skill.activated")
            .subject("task", self.task.id.0.to_string())
            .actor("agent")
            .payload_json(payload);
        if let Err(e) = store.record_audit(&input).await {
            tracing::warn!(skill = %skill.name, "skill activation audit failed: {e}");
        }
    }
}

/// Render matched skills as labeled planning-prompt constraint blocks. Pure, so
/// the formatting is unit-testable without a runner or store.
fn skill_constraint_blocks(skills: &[&lopi_skill::Skill]) -> Vec<String> {
    skills
        .iter()
        .map(|s| {
            format!(
                "Skill «{}» (v{}) — {}\n{}",
                s.name, s.version, s.description, s.body
            )
        })
        .collect()
}

/// Frame a retrieved learning's critique as a planning-prompt constraint. Pure,
/// so the wording is unit-testable without a store. Labeled so the worker reads
/// it as a prior failure to avoid, not a fresh instruction.
fn reflection_constraint(critique: &str) -> String {
    format!("Past learning — a prior attempt failed because: {critique}")
}

/// Frame the stability gate's consensus plan as a planning-prompt
/// constraint. Pure, so the wording is unit-testable without a runner.
/// Labeled as a starting point (not a mandate) — the worker still owns the
/// final plan.
fn consensus_plan_constraint(consensus_plan: &str) -> String {
    format!(
        "Stability pre-flight already sampled several plan variants for this \
         goal; the most representative one is below — use it as a starting \
         point rather than planning from scratch:\n{consensus_plan}"
    )
}

/// Return an owned copy of `c` when it is present and non-empty.
fn non_empty_constraint(c: Option<&str>) -> Option<String> {
    c.and_then(|c| (!c.is_empty()).then(|| c.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{
        consensus_plan_constraint, is_promotable, non_empty_constraint, reflection_constraint,
        skill_constraint_blocks, AgentRunner, MIN_PATTERN_OCCURRENCES, MIN_PATTERN_SUCCESS_RATE,
        REFLECTION_INJECTION_CAP,
    };
    use lopi_core::{AgentEvent, Attempt, EventBus, Score, Task};
    use lopi_memory::{MemoryStore, PatternRow};
    use lopi_skill::Skill;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Arc;

    /// A promotable baseline row (postmortem-exempt fields at their
    /// gate-clearing values) that individual tests tweak one field at a
    /// time — keeps each test's intent to a single changed field instead of
    /// repeating the whole literal.
    fn promotable_row() -> PatternRow {
        PatternRow {
            id: "p1".into(),
            goal_keywords: "auth middleware".into(),
            successful_constraints: Some("always mock the token clock".into()),
            avg_attempts: Some(1.0),
            success_rate: Some(0.8),
            last_seen: "2026-01-01T00:00:00Z".into(),
            derived_from_postmortem: 0,
            user_annotation: None,
            occurrence_count: MIN_PATTERN_OCCURRENCES,
        }
    }

    async fn save_high_score_attempt(store: &MemoryStore, task: &Task) {
        store
            .save_attempt(&Attempt {
                id: uuid::Uuid::new_v4(),
                task_id: task.id,
                attempt_num: 1,
                branch: "test/branch".into(),
                score: Some(Score::new(1.0, 0, 10)),
                outcome: "success".into(),
                created_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
    }

    #[test]
    fn reflection_constraint_labels_the_prior_failure() {
        let c = reflection_constraint("the mutex was held across an await");
        assert!(c.starts_with("Past learning"));
        assert!(c.contains("the mutex was held across an await"));
    }

    #[test]
    fn injection_cap_is_small_and_bounded() {
        assert!(
            (1..=5).contains(&REFLECTION_INJECTION_CAP),
            "the cap must stay small — bounded injection is the §2 discipline"
        );
    }

    #[test]
    fn keeps_non_empty_drops_empty_and_none() {
        assert_eq!(non_empty_constraint(Some("x")), Some("x".to_string()));
        assert_eq!(non_empty_constraint(Some("")), None);
        assert_eq!(non_empty_constraint(None), None);
    }

    #[test]
    fn skill_blocks_label_name_version_description_and_body() {
        let skill = Skill {
            name: "refactor".into(),
            description: "safe refactors".into(),
            user_invocable: false,
            version: "1.0.0".into(),
            triggers: vec!["refactor".into()],
            body: "Always run tests after.".into(),
            source: PathBuf::from("x/SKILL.md"),
        };
        let blocks = skill_constraint_blocks(&[&skill]);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0],
            "Skill «refactor» (v1.0.0) — safe refactors\nAlways run tests after."
        );
    }

    #[test]
    fn skill_blocks_empty_for_no_matches() {
        assert!(skill_constraint_blocks(&[]).is_empty());
    }

    #[test]
    fn consensus_plan_constraint_labels_it_as_a_starting_point_and_carries_the_text() {
        let c = consensus_plan_constraint("1. Read the file\n2. Fix the bug");
        assert!(c.contains("starting point"));
        assert!(c.contains("1. Read the file\n2. Fix the bug"));
    }

    #[test]
    fn is_promotable_true_when_both_thresholds_are_met() {
        assert!(is_promotable(&promotable_row()));
    }

    #[test]
    fn is_promotable_false_below_occurrence_threshold() {
        let mut row = promotable_row();
        row.occurrence_count = MIN_PATTERN_OCCURRENCES - 1;
        assert!(!is_promotable(&row));
    }

    #[test]
    fn is_promotable_false_below_success_rate_threshold() {
        let mut row = promotable_row();
        row.success_rate = Some(MIN_PATTERN_SUCCESS_RATE - 0.01);
        assert!(!is_promotable(&row));
    }

    #[test]
    fn is_promotable_false_when_success_rate_is_absent() {
        let mut row = promotable_row();
        row.success_rate = None;
        assert!(!is_promotable(&row));
    }

    #[test]
    fn is_promotable_true_for_postmortem_pattern_regardless_of_stats() {
        // A fresh postmortem pattern starts at success_rate = 0.0 and
        // occurrence_count = 1 by construction (insert_postmortem_pattern) —
        // both would fail the mined-pattern gate. Postmortem patterns are
        // exempt: one curated failure lesson has always been enough.
        let mut row = promotable_row();
        row.derived_from_postmortem = 1;
        row.occurrence_count = 1;
        row.success_rate = Some(0.0);
        assert!(is_promotable(&row));
    }

    /// Exit-gate live check (in miniature): a pattern mined twice with a
    /// consistently high score clears the promotion gate and its constraint
    /// reaches `gather_seed`'s output; a pattern mined only once — the exact
    /// "every one-off task becomes an equally-weighted template" case the
    /// gate exists to stop — does not, even though both are similar enough
    /// to the query goal to be candidates in the first place.
    #[tokio::test]
    async fn gather_seed_only_injects_promotable_pattern_constraints() {
        let store = MemoryStore::open_in_memory().await.unwrap();

        // Promotable: mined twice under the same goal fingerprint, each
        // time from a clean high-score attempt.
        for _ in 0..2 {
            let t = Task::new("update authentication middleware routing");
            store.save_task(&t, "queued").await.unwrap();
            save_high_score_attempt(&store, &t).await;
            store
                .mine_patterns(
                    &t.id,
                    &t.goal,
                    Some("Always mock the token clock in auth middleware tests"),
                )
                .await
                .unwrap();
        }

        // One-off: mined exactly once, occurrence_count stays at 1.
        let one_off = Task::new("update authentication middleware configuration");
        store.save_task(&one_off, "queued").await.unwrap();
        save_high_score_attempt(&store, &one_off).await;
        store
            .mine_patterns(
                &one_off.id,
                &one_off.goal,
                Some("Reload config from disk on every request"),
            )
            .await
            .unwrap();

        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let mut runner = AgentRunner::new(
            Task::new("update authentication middleware handling"),
            PathBuf::from("/repo"),
            bus,
            Some(store),
            rx,
            Arc::new(AtomicUsize::new(0)),
        );

        let seed = runner.gather_seed().await;
        assert!(
            seed.extra_constraints
                .iter()
                .any(|c| c.contains("Always mock the token clock")),
            "the twice-mined, high-success pattern's constraint should be injected: {:?}",
            seed.extra_constraints
        );
        assert!(
            seed.extra_constraints
                .iter()
                .all(|c| !c.contains("Reload config from disk")),
            "the once-mined pattern's constraint must not be injected: {:?}",
            seed.extra_constraints
        );
    }

    /// Session Prompt 2's own exit gate: "a passing unit test alone does not
    /// close this sprint — the whole point is that `seed.rs` was silently
    /// getting nothing before this." This test goes one step further than
    /// `gather_seed_only_injects_promotable_pattern_constraints` above: it
    /// backfills a **file-backed** SQLite store (not `:memory:`) — the same
    /// on-disk shape a real repo's `.lopi/lopi.db` has — by running a
    /// *simulated* prior task through the real production sequence
    /// (`AgentRunner::success_constraint` deriving a constraint from a real
    /// `last_plan`, then the real `mine_patterns` write, exactly as
    /// `pool::run_loop::run_one` does on a clean success), then drives a
    /// **second**, fresh task through the real `gather_seed()` →
    /// `claude_support::build_plan_prompt()` pipeline and asserts the
    /// literal text that would be handed to `claude -p` for planning
    /// contains the prior task's constraint. This sandbox has no live
    /// Anthropic API session to run `claude -p` itself against (the same
    /// standing constraint every prior sprint's LEDGER entry records), so
    /// this is as far into the real pipeline as a session without one can
    /// verify — everything after this point is the CLI subprocess handing
    /// this exact string to Claude unmodified.
    #[tokio::test]
    async fn live_check_backfilled_pattern_constraint_reaches_the_real_planning_prompt() {
        let db_path = std::env::temp_dir().join(format!(
            "lopi-constraint-capture-2-live-check-{}.db",
            uuid::Uuid::new_v4()
        ));
        let store = MemoryStore::open(&db_path).await.unwrap();

        // Simulate a completed prior task in a Rust toolchain repo, run
        // through the exact production sequence: a runner with a real
        // `last_plan`, `success_constraint()` deriving the constraint from
        // it, then `mine_patterns` persisting it on a clean success — twice,
        // so the pattern clears the promotion gate's occurrence threshold.
        for _ in 0..2 {
            let prior_task = Task::new("add retry backoff to the postgres connection pool");
            store.save_task(&prior_task, "queued").await.unwrap();
            save_high_score_attempt(&store, &prior_task).await;

            let bus: EventBus<AgentEvent> = EventBus::new(16);
            let (_tx, rx) = tokio::sync::oneshot::channel();
            let mut prior_runner = AgentRunner::new(
                prior_task.clone(),
                PathBuf::from("/repo"),
                bus,
                None,
                rx,
                Arc::new(AtomicUsize::new(0)),
            );
            prior_runner.last_plan = Some(
                "Wrap the pool acquire call in exponential backoff with jitter\n\
                 then add a connection-health check before returning it to the caller."
                    .to_string(),
            );
            let constraint = prior_runner.success_constraint();
            assert!(
                constraint.is_some(),
                "a real plan always yields a constraint"
            );

            store
                .mine_patterns(&prior_task.id, &prior_task.goal, constraint.as_deref())
                .await
                .unwrap();
        }

        // A fresh task in the same toolchain, similar enough to surface the
        // backfilled pattern (shares "postgres"/"connection"/"pool").
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let (_tx, rx) = tokio::sync::oneshot::channel();
        let mut runner = AgentRunner::new(
            Task::new("fix a postgres connection pool leak under load"),
            PathBuf::from("/repo"),
            bus,
            Some(store),
            rx,
            Arc::new(AtomicUsize::new(0)),
        );

        let seed = runner.gather_seed().await;
        assert!(
            !seed.extra_constraints.is_empty(),
            "gather_seed must inject at least the backfilled pattern's constraint"
        );

        // The real planning-prompt builder — the exact function
        // `ClaudeCode::build_plan_prompt` calls for both the one-shot and
        // streaming plan paths — not a stand-in.
        let prompt = crate::claude_support::build_plan_prompt(
            &runner.task,
            None,
            &seed.extra_constraints,
            &seed.pattern_pairs,
            &seed.lessons_data,
        );

        println!("\n--- live planning prompt (Constraint-Capture-2 verification) ---");
        println!("{prompt}");
        println!("--- end planning prompt ---\n");

        assert!(
            prompt.contains("exponential backoff") || prompt.contains("connection-health check"),
            "the real planning prompt must contain the backfilled pattern's constraint, \
             not just PlanningSeed's in-memory fields — got:\n{prompt}"
        );

        // Best-effort cleanup — a leftover temp db is harmless but pointless
        // to keep, since this test's whole point was exercising a real
        // on-disk store, not accumulating one.
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    }
}
