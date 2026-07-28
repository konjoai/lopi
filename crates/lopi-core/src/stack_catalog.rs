//! Sprint T0 — the static eval/preset catalogs, split out of `stack.rs` to
//! keep that file under the 500-line CI file-size gate. Config, not logic;
//! every value here must match the shipped web/Swift catalogs exactly,
//! since T1's command-palette autocomplete depends on it byte-for-byte.

use crate::EvalTier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::EvalRef;

/// The eval every card carries and can never turn off — code that builds
/// and runs clean. `EVAL_CATALOG`'s first entry, always.
#[must_use]
pub fn baseline_eval() -> EvalRef {
    EvalRef::new("execution ok", EvalTier::ExecutionOk)
}

/// The full eval catalog a card's checklist can select from
/// (`StackTypes.swift:310-321`).
#[must_use]
pub fn eval_catalog() -> Vec<EvalRef> {
    vec![
        baseline_eval(),
        EvalRef::new("tests pass", EvalTier::ShellTest),
        EvalRef::new("unit", EvalTier::ShellTest),
        EvalRef::new("integration", EvalTier::ShellTest),
        EvalRef::new("benchmark gate", EvalTier::ShellTest),
        EvalRef::new("30-run gate", EvalTier::ShellTest),
        EvalRef::new("code review", EvalTier::Judge),
        EvalRef::new("beats-best", EvalTier::Judge),
        EvalRef::new("vuln scan", EvalTier::Suite),
        EvalRef::new("adversarial", EvalTier::Suite),
    ]
}

/// Named eval bundles a preset or suite picker can expand to
/// (`StackTypes.swift:324-328`).
#[must_use]
pub fn eval_suites() -> HashMap<&'static str, Vec<&'static str>> {
    HashMap::from([
        (
            "kcqf",
            vec!["tests pass", "code review", "vuln scan", "adversarial"],
        ),
        ("security", vec!["vuln scan", "adversarial"]),
        ("research", vec!["code review"]),
    ])
}

/// The eight quick-insert composer presets (`:research`, `:implement`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKey {
    /// Explore & investigate.
    Research,
    /// Build a feature.
    Implement,
    /// Improve speed.
    Optimize,
    /// Self-improve — ratchet on beats-best.
    Gain,
    /// Measure variance.
    Benchmark,
    /// Verify it works.
    Test,
    /// Try to break it.
    Killtest,
    /// Write up findings.
    Report,
}

impl PresetKey {
    /// All preset keys, in the catalog's canonical display order.
    #[must_use]
    pub const fn all() -> [Self; 8] {
        [
            Self::Research,
            Self::Implement,
            Self::Optimize,
            Self::Gain,
            Self::Benchmark,
            Self::Test,
            Self::Killtest,
            Self::Report,
        ]
    }
}

/// A single preset's label, alias, keyword-suggestion triggers, and the
/// eval set it attaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetDef {
    /// The preset's key.
    pub key: PresetKey,
    /// Display label.
    pub label: &'static str,
    /// `:`-prefixed alias the composer recognizes.
    pub alias: &'static str,
    /// Keywords that suggest this preset from free-text input.
    pub keywords: &'static [&'static str],
    /// Evals this preset attaches to a fresh card.
    pub evals: Vec<EvalRef>,
}

/// The full preset catalog (`StackTypes.swift:331-402`) — config, not
/// logic; every field here must match the shipped web/Swift catalogs
/// exactly, since T1's command-palette autocomplete depends on it.
#[must_use]
pub fn preset_catalog() -> HashMap<PresetKey, PresetDef> {
    use EvalTier::{Judge, ShellTest, Suite};
    let eval = EvalRef::new;
    let base = baseline_eval;
    HashMap::from([
        (
            PresetKey::Research,
            PresetDef {
                key: PresetKey::Research,
                label: "research",
                alias: ":research",
                keywords: &[
                    "research",
                    "investigate",
                    "explore",
                    "learn",
                    "study",
                    "survey",
                ],
                evals: vec![base(), eval("code review", Judge)],
            },
        ),
        (
            PresetKey::Implement,
            PresetDef {
                key: PresetKey::Implement,
                label: "implement",
                alias: ":implement",
                keywords: &[
                    "add",
                    "build",
                    "implement",
                    "feature",
                    "create",
                    "gate",
                    "wire",
                ],
                evals: vec![
                    base(),
                    eval("unit", ShellTest),
                    eval("integration", ShellTest),
                    eval("code review", Judge),
                    eval("vuln scan", Suite),
                    eval("adversarial", Suite),
                ],
            },
        ),
        (
            PresetKey::Optimize,
            PresetDef {
                key: PresetKey::Optimize,
                label: "optimize",
                alias: ":optimize",
                keywords: &[
                    "optimize",
                    "improve",
                    "speed",
                    "performance",
                    "faster",
                    "latency",
                ],
                evals: vec![
                    base(),
                    eval("beats-best", Judge),
                    eval("30-run gate", ShellTest),
                    eval("adversarial", Suite),
                ],
            },
        ),
        (
            PresetKey::Gain,
            PresetDef {
                key: PresetKey::Gain,
                label: "gain",
                alias: ":gain",
                keywords: &[
                    "gain",
                    "ratchet",
                    "self-improve",
                    "self improve",
                    "beats-best",
                ],
                evals: vec![
                    base(),
                    eval("beats-best", Judge),
                    eval("adversarial", Suite),
                ],
            },
        ),
        (
            PresetKey::Benchmark,
            PresetDef {
                key: PresetKey::Benchmark,
                label: "benchmark",
                alias: ":benchmark",
                keywords: &["benchmark", "measure", "variance", "throughput"],
                evals: vec![
                    base(),
                    eval("benchmark gate", ShellTest),
                    eval("30-run gate", ShellTest),
                ],
            },
        ),
        (
            PresetKey::Test,
            PresetDef {
                key: PresetKey::Test,
                label: "test",
                alias: ":test",
                keywords: &["test", "verify", "validate", "confirm", "prove", "check"],
                evals: vec![
                    base(),
                    eval("tests pass", ShellTest),
                    eval("integration", ShellTest),
                    eval("code review", Judge),
                ],
            },
        ),
        (
            PresetKey::Killtest,
            PresetDef {
                key: PresetKey::Killtest,
                label: "killtest",
                alias: ":killtest",
                keywords: &[
                    "killtest",
                    "kill test",
                    "break",
                    "destroy",
                    "adversarial",
                    "stress",
                    "fuzz",
                    "attack",
                ],
                evals: vec![
                    base(),
                    eval("adversarial", Suite),
                    eval("vuln scan", Suite),
                    eval("30-run gate", ShellTest),
                ],
            },
        ),
        (
            PresetKey::Report,
            PresetDef {
                key: PresetKey::Report,
                label: "report",
                alias: ":report",
                keywords: &[
                    "report",
                    "summarize",
                    "summary",
                    "findings",
                    "writeup",
                    "write up",
                    "docs",
                ],
                evals: vec![base(), eval("code review", Judge)],
            },
        ),
    ])
}

/// `PresetKey::all()`, in catalog order — a thin convenience over
/// `PresetKey::all()` matching the Swift catalog's `PRESET_KEYS` name.
#[must_use]
pub fn preset_keys() -> Vec<PresetKey> {
    PresetKey::all().to_vec()
}

/// One-line description per preset, for palette hints
/// (`StackTypes.swift:410-419`).
#[must_use]
pub fn preset_descriptions() -> HashMap<PresetKey, &'static str> {
    HashMap::from([
        (
            PresetKey::Research,
            "explore & investigate — judge-reviewed",
        ),
        (
            PresetKey::Implement,
            "build a feature — full test + review suite",
        ),
        (
            PresetKey::Optimize,
            "improve speed — beats-best + 30-run gate",
        ),
        (PresetKey::Gain, "self-improve — ratchet on beats-best"),
        (
            PresetKey::Benchmark,
            "measure variance — benchmark + 30-run gate",
        ),
        (
            PresetKey::Test,
            "verify it works — full test suite + review",
        ),
        (
            PresetKey::Killtest,
            "try to break it — adversarial + vuln scan + 30-run gate",
        ),
        (
            PresetKey::Report,
            "write up findings — .md summary, judge-reviewed",
        ),
    ])
}

/// Legacy alias names that still resolve to a current preset key
/// (`StackTypes.swift:422`).
#[must_use]
pub fn legacy_aliases() -> HashMap<&'static str, PresetKey> {
    HashMap::from([("ratchet", PresetKey::Gain)])
}
