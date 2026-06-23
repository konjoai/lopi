use anyhow::Result;
use lopi_agent::{AgentRunner, AnthropicClient, AnthropicLimiter, CircuitBreaker, StabilityConfig};
use lopi_core::{AgentEvent, LopiConfig, RepoProfile, Task, TaskSource};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::util::{db_path, is_self_modify_attempt, status_label};

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
) -> Result<()> {
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
    let mut runner = AgentRunner::standalone(task.clone(), repo)
        .0
        .with_self_prompt(loop_cfg.self_prompt)
        .with_strategy_escalation(loop_cfg.escalate_strategy)
        .with_task_budget(loop_cfg.budget_tokens);
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
                }) => {
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
    store
        .mark_completed(&task_id, &status_label(&outcome))
        .await
        .ok();
    store.mine_patterns(&task_id, &task.goal).await.ok();

    println!();
    println!("⚓ {}", status_label(&outcome));
    Ok(())
}
