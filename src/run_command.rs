use anyhow::Result;
use lopi_agent::{AgentRunner, AnthropicClient, AnthropicLimiter, CircuitBreaker, StabilityConfig};
use lopi_core::{
    AgentEvent, BudgetOverride, BudgetPreset, LopiConfig, RepoProfile, Task, TaskId, TaskSource,
    TaskStatus,
};
use lopi_memory::MemoryStore;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::util::{db_path, is_self_modify_attempt, status_label};

/// One-off budget overrides parsed from `lopi run`'s `--budget`/
/// `--budget-preset`/`--budget-tokens` flags — the CLI-argument-parsing
/// counterpart of [`lopi_core::BudgetOverride`], kept separate so this module
/// doesn't need to construct the core type before validating the preset name.
#[derive(Default)]
pub struct BudgetArgs {
    /// `--budget <usd>` — one-off per-session USD cap.
    pub budget: Option<f64>,
    /// `--budget-preset <name>` — one-off named preset (quick/standard/deep/unlimited).
    pub budget_preset: Option<String>,
    /// `--budget-tokens <n>` — one-off per-run token budget.
    pub budget_tokens: Option<u64>,
}

impl BudgetArgs {
    /// Parse into a [`BudgetOverride`]. Errors loudly on an unrecognized
    /// preset name rather than silently falling back to the repo default —
    /// a typo'd `--budget-preset` should never quietly run uncapped-standard.
    fn resolve(self) -> Result<BudgetOverride> {
        let preset = self
            .budget_preset
            .as_deref()
            .map(|s| {
                BudgetPreset::parse(s)
                    .ok_or_else(|| anyhow::anyhow!("unknown --budget-preset '{s}' (expected quick, standard, deep, or unlimited)"))
            })
            .transpose()?;
        Ok(BudgetOverride {
            preset,
            usd: self.budget,
            tokens: self.budget_tokens,
        })
    }
}

/// Run `runner` to completion while bridging its `AgentEvent` bus to stdout
/// (status transitions, log lines, and — when `print_score` is set — score
/// updates), then persist the outcome. Shared by the plain CLI run path and
/// the REPL's bypass path, which differ only in whether score lines print.
pub(crate) async fn run_with_live_print(
    mut runner: AgentRunner,
    store: &MemoryStore,
    task_id: TaskId,
    goal: &str,
    print_score: bool,
) -> Result<TaskStatus> {
    let bus = runner.bus.clone();
    let mut rx = bus.subscribe();
    let print_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(AgentEvent::StatusChanged {
                    status, attempt, ..
                }) => {
                    println!("  [{attempt}] → {}", status_label(&status));
                }
                Ok(AgentEvent::LogLine { line, .. }) => {
                    println!("       {line}");
                }
                Ok(AgentEvent::ScoreUpdated {
                    test_pass_rate,
                    lint_errors,
                    ..
                }) if print_score => {
                    println!(
                        "       score: {:.0}% pass, {} lint errors",
                        test_pass_rate * 100.0,
                        lint_errors
                    );
                }
                Ok(AgentEvent::TaskCompleted { .. }) | Err(_) => break,
                _ => {}
            }
        }
    });

    let outcome = runner.run().await?;
    print_task.abort();
    // Persist the canonical status token — not the human/emoji display label.
    // `status_label` stays for the printed line below; writing it to the DB is
    // what once produced compound values like "failed ❌ Cancelled" that the
    // dashboard could not bucket.
    store
        .mark_completed(&task_id, outcome.db_status())
        .await
        .ok();
    store.mine_patterns(&task_id, goal).await.ok();
    println!();
    println!("⚓ {}", status_label(&outcome));
    // Budget & Guardrail Controls Part 4.3 — surface the session's real
    // billed spend (already flowing into turn_metrics via every streamed
    // call) in the run-complete line, so it's visible without a SQL query.
    match store.task_cost(&task_id.0.to_string()).await {
        Ok(cost_usd) if cost_usd > 0.0 => println!("💵 session cost: ${cost_usd:.4}"),
        Ok(_) => {}
        Err(e) => tracing::warn!(task_id = %task_id, "failed to load session cost: {e}"),
    }
    Ok(outcome)
}

/// `lopi run` — execute a single agent task on the current terminal.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    goal: String,
    repo: PathBuf,
    dry_run: bool,
    speculative: bool,
    adaptive_retry: bool,
    stability_gate: bool,
    cfg: Option<&LopiConfig>,
    budget_args: BudgetArgs,
) -> Result<()> {
    let budget_override = budget_args.resolve()?;
    // Skill Arguments (Sprint 2) — a `:name args` goal resolves to the named
    // skill's rendered body *before* anything else sees `goal`; Claude only
    // ever sees the resolved literal, exactly like a Sprint 1 template.
    let goal = resolve_skill_invocation(goal, &repo)?;

    println!("🚢 lopi run{}", if dry_run { " (dry-run)" } else { "" });
    println!("   goal: {goal}");
    println!("   repo: {}", repo.display());

    let profile = RepoProfile::load_from_repo(&repo);
    let has_profile = !profile.allowed_dirs.is_empty()
        || profile.max_retries.is_some()
        || !profile.default_constraints.is_empty();
    if has_profile {
        println!("   profile: .lopi.toml found — applying overrides");
    }
    println!();

    let store = MemoryStore::open(db_path()).await?;
    let mut task = Task::new(goal);
    profile.apply(&mut task);

    if is_self_modify_attempt(&repo) {
        let allow_self_modify = cfg.is_some_and(|c| c.lopi.allow_self_modify);
        if !allow_self_modify {
            eprintln!("❌ self-modification blocked: lopi cannot modify itself");
            eprintln!(
                "   to enable, set `allow_self_modify = true` in [lopi] section of lopi.toml"
            );
            return Err(anyhow::anyhow!("self-modification not allowed"));
        }
        task.source = TaskSource::SelfModify {
            approved_by: "config".into(),
        };
        task.allowed_dirs = vec!["crates/".into(), "src/".into()];
        task.forbidden_dirs = vec![".github/".into(), "Cargo.lock".into()];
    }

    let task_id = task.id;
    let id_short = &task_id.0.to_string()[..8];
    store.save_task(&task, "queued").await.ok();

    println!("   task id: {id_short}…");
    println!("   use `lopi watch` in another terminal for the TUI");
    println!();

    // Loop-as-code: honor the repo's `.lopi/loop.toml` self-prompting strategy.
    // A malformed file falls back to the conservative default rather than aborting the run.
    let loop_cfg = lopi_core::LoopConfig::load_from_repo(&repo).unwrap_or_default();
    // Budget & Guardrail Controls Part 3 — resolve the repo's `[budget]`
    // preset, then layer any one-off `--budget`/`--budget-preset`/
    // `--budget-tokens` override on top, same resolution order the pool
    // uses for a queued task's `budget_override`.
    let resolved_budget = if budget_override.is_empty() {
        loop_cfg.resolved_budget()
    } else {
        budget_override.apply(loop_cfg.resolved_budget())
    };
    if !budget_override.is_empty() {
        println!(
            "   budget: ${:.2} / {} tokens / deny {:?}",
            resolved_budget.usd, resolved_budget.tokens, resolved_budget.deny
        );
    }
    let mut runner = AgentRunner::standalone(task.clone(), repo)
        .0
        .with_self_prompt(loop_cfg.self_prompt)
        .with_strategy_escalation(loop_cfg.escalate_strategy)
        .with_task_budget(resolved_budget.tokens)
        .with_cli_budget_usd(resolved_budget.usd)
        .with_tool_permissions(resolved_budget.allow, resolved_budget.deny);
    if adaptive_retry {
        runner = runner.with_adaptive_retry();
        let mode = if loop_cfg.escalate_strategy {
            "escalating S1→S4"
        } else {
            "pinned"
        };
        println!(
            "   self-prompt: {} ({}) · {mode}",
            loop_cfg.self_prompt.tag(),
            loop_cfg.self_prompt.label()
        );
    }
    if stability_gate {
        match AnthropicClient::from_env() {
            Ok(client) => {
                let limiter = Arc::new(AnthropicLimiter::default_pro());
                let breaker = Arc::new(CircuitBreaker::new(3, Duration::from_secs(60), 5.0));
                runner = runner.with_stability_gate(
                    Arc::new(client),
                    Some(limiter),
                    Some(breaker),
                    StabilityConfig::default(),
                );
                println!("   stability gate: enabled (n=5, stable≤0.15, block>0.35)");
            }
            Err(e) => {
                eprintln!("⚠️  --stability-gate ignored: {e}");
                eprintln!("   set ANTHROPIC_API_KEY to enable the Layer 5 stability gate");
            }
        }
    }
    runner.store = Some(store.clone());
    runner.dry_run = dry_run;
    runner.speculative = speculative;

    run_with_live_print(runner, &store, task_id, &task.goal, true).await?;
    Ok(())
}

/// If `goal` is a `:<skill-name> <args>` invocation (Skill Arguments, Sprint
/// 2), resolve it to the named skill's rendered body; otherwise return
/// `goal` unchanged. Looks the skill up in `repo`'s conventional
/// `.claude/skills`/`.lopi/skills` directories.
///
/// A skill name with no match is never silently passed through as a literal
/// goal — it fails loudly (a warn log plus an `Err`) so a typo doesn't
/// quietly turn into "fix the bug: kcqf vectro" being sent to Claude.
///
/// # Errors
/// Returns `Err` if the skill directories fail to load, no skill by that
/// name exists, or the body fails to render (an unresolved template hole).
fn resolve_skill_invocation(goal: String, repo: &Path) -> Result<String> {
    let Some((name, args)) = lopi_skill::parse_invocation(&goal) else {
        return Ok(goal);
    };
    let registry = lopi_skill::SkillRegistry::load_from_dirs(&[
        repo.join(".claude/skills"),
        repo.join(".lopi/skills"),
    ])?;
    let Some(skill) = registry.get(name) else {
        tracing::warn!(skill = name, repo = %repo.display(), "skill invocation: no such skill");
        anyhow::bail!("no skill named `{name}` in {}", repo.display());
    };
    Ok(skill.render_body(args)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn budget_args_default_resolves_to_an_empty_override() {
        let ov = BudgetArgs::default().resolve().unwrap();
        assert!(ov.is_empty());
    }

    #[test]
    fn budget_args_bare_usd_resolves_without_touching_preset() {
        let ov = BudgetArgs {
            budget: Some(5.0),
            budget_preset: None,
            budget_tokens: None,
        }
        .resolve()
        .unwrap();
        assert_eq!(ov.usd, Some(5.0));
        assert!(ov.preset.is_none());
    }

    #[test]
    fn budget_args_valid_preset_name_resolves() {
        let ov = BudgetArgs {
            budget: None,
            budget_preset: Some("deep".to_string()),
            budget_tokens: None,
        }
        .resolve()
        .unwrap();
        assert_eq!(ov.preset, Some(BudgetPreset::Deep));
    }

    /// A typo'd `--budget-preset` must fail loudly, never silently fall back
    /// to a different (potentially wider-open) tier.
    #[test]
    fn budget_args_unknown_preset_name_errors() {
        let err = BudgetArgs {
            budget: None,
            budget_preset: Some("deap".to_string()),
            budget_tokens: None,
        }
        .resolve()
        .unwrap_err();
        assert!(err.to_string().contains("unknown --budget-preset"));
    }

    #[test]
    fn budget_args_all_three_flags_combine() {
        let ov = BudgetArgs {
            budget: Some(25.0),
            budget_preset: Some("deep".to_string()),
            budget_tokens: Some(9_000_000),
        }
        .resolve()
        .unwrap();
        assert_eq!(ov.preset, Some(BudgetPreset::Deep));
        assert_eq!(ov.usd, Some(25.0));
        assert_eq!(ov.tokens, Some(9_000_000));
    }
}
