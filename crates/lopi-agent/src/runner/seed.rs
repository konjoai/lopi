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

        // Finding #4 (Symbol Index) — seed the deterministic repo map when
        // `context_mode` opts in. `None` on any failure (index open,
        // reindex, or map build) — the repo map is orientation, never a
        // hard requirement to plan at all, so a failure here degrades to
        // the pre-existing no-repo-map behavior rather than blocking the run.
        if let Some(repo_map) = self.seed_repo_map().await {
            extra_constraints.push(repo_map);
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

    /// Finding #4 (Symbol Index) — build/refresh the repo's symbol index and
    /// render its deterministic repo map. `None` when `context_mode !=
    /// Index`, or on any failure along the way (index open, reindex, map
    /// build) — logged via [`Self::warn`], never fatal to the run.
    ///
    /// Known cost: a repo with no `.lopi/index.db` yet pays a full-repo
    /// parse on this call (seconds, not milliseconds, on a large repo — see
    /// `LEDGER.md`'s Finding #4 entry). A subsequent call on the same repo
    /// only reindexes what changed. Pre-warming the index out of band
    /// (before the loop starts, not on the seeding critical path) is
    /// flagged there as follow-up work, not attempted in this pass.
    pub(super) async fn seed_repo_map(&self) -> Option<String> {
        if self.context_mode != lopi_core::ContextMode::Index {
            return None;
        }
        // Canonicalize before deriving `repo_id` — must match `lopi index`/
        // `lopi mcp-index-serve`'s own `repo.canonicalize()` exactly, or a
        // pre-warmed index (`lopi index --repo .` run out of band) would
        // silently land under a different `repo_id` than this call reads,
        // splitting one physical repo's symbols across two disjoint
        // partitions of the same `.lopi/index.db`.
        let repo_root = match self.repo_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                self.warn(format!("repo map: failed to canonicalize repo path: {e}"));
                return None;
            }
        };
        let db_path = repo_root.join(lopi_index::INDEX_DB_REL_PATH);
        let store = match lopi_index::IndexStore::open(&db_path).await {
            Ok(s) => s,
            Err(e) => {
                self.warn(format!("repo map: failed to open index store: {e}"));
                return None;
            }
        };
        let repo_id = repo_root.to_string_lossy().to_string();
        if let Err(e) = lopi_index::reindex::reindex(&store, &repo_id, &repo_root).await {
            self.warn(format!("repo map: reindex failed: {e}"));
            return None;
        }

        let profile = lopi_core::RepoProfile::load_from_repo(&repo_root);
        let cfg = lopi_index::IndexConfig::default();
        let test_cmd = profile
            .test_command
            .unwrap_or_else(|| "cargo test --workspace".to_string());
        let lint_cmd = profile
            .lint_command
            .unwrap_or_else(|| "cargo clippy -- -D warnings".to_string());
        let cmds = [
            ("build", "cargo build"),
            ("test", test_cmd.as_str()),
            ("lint", lint_cmd.as_str()),
        ];

        match lopi_index::RepoMap::build(
            &store,
            &repo_id,
            &repo_root,
            cfg.skeleton_depth,
            cfg.top_referenced_symbols,
            &cmds,
            cfg.map_token_budget,
        )
        .await
        {
            Ok(map) => {
                self.log(format!(
                    "🗺️ repo map seeded: {} chars{}",
                    map.text.len(),
                    if map.truncated { " (truncated)" } else { "" }
                ));
                Some(repo_map_constraint(&map.text))
            }
            Err(e) => {
                self.warn(format!("repo map: build failed: {e}"));
                None
            }
        }
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

/// Frame the repo map as a planning-prompt constraint. Pure, so the framing
/// is unit-testable without a store or filesystem. Explicitly tells the
/// worker to navigate by pointer (`lopi_find`/`lopi_read`/`lopi_refs`/
/// `lopi_query`) rather than reading whole files — the map orients, it
/// doesn't replace looking things up.
fn repo_map_constraint(map_text: &str) -> String {
    format!(
        "Repo map (context.mode = \"index\"): the shape of this repo, not its \
         full contents. Use the lopi_find/lopi_read/lopi_refs/lopi_query tools \
         to navigate to specific symbols by pointer rather than reading whole \
         files blind.\n{map_text}"
    )
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
#[path = "seed_tests.rs"]
mod tests;
