//! `/budget` command — Budget & Guardrail Controls Part 3's remote lever.
//!
//! Split out of `handlers.rs` purely to keep that file under the 500-line
//! CI file-size gate; no behavioral difference from being inline.

use anyhow::Result;
use lopi_core::BudgetOverride;
use lopi_orchestrator::AgentPool;
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::Mutex;

use lopi_core::Task;

/// Shared pending-budget state: maps chat_id → the one-off [`BudgetOverride`]
/// set by `/budget`, consumed (and cleared) by the next `/task`/`/urgent`/
/// `/critical`/`/submit` from that chat — "the next card", not a sticky
/// per-chat default.
pub type PendingBudgetMap = Arc<Mutex<HashMap<i64, BudgetOverride>>>;

/// Consume (and clear) this chat's pending `/budget` override onto `task`,
/// returning a short suffix describing it for the confirmation message —
/// empty when no override was pending. One-shot: "/budget" sets it for the
/// *next* card only, not every subsequent one.
pub(super) async fn take_pending_budget(
    pending_budgets: &PendingBudgetMap,
    chat_id: i64,
    task: &mut Task,
) -> String {
    let Some(ov) = pending_budgets.lock().await.remove(&chat_id) else {
        return String::new();
    };
    let note = format!("\n💰 budget: {}", describe_override(&ov));
    task.budget_override = Some(ov);
    note
}

/// Human-readable one-liner for a [`BudgetOverride`], used in both the
/// queue-confirmation suffix and `/budget status`.
fn describe_override(ov: &BudgetOverride) -> String {
    let mut parts = Vec::new();
    if let Some(p) = ov.preset {
        parts.push(format!("preset={}", p.tag()));
    }
    if let Some(usd) = ov.usd {
        parts.push(format!("${usd:.2}"));
    }
    if let Some(tokens) = ov.tokens {
        parts.push(format!("{tokens} tokens"));
    }
    if parts.is_empty() {
        "(none)".to_string()
    } else {
        parts.join(", ")
    }
}

/// `/budget <preset|usd>` — set a one-off budget override for this chat's
/// next queued card. `/budget status` echoes the resolved budget that would
/// apply right now (pending override applied on top of the pool's repo
/// budget) without consuming it, so an operator can confirm before kicking
/// off a long run.
pub(super) async fn handle_budget(
    bot: &Bot,
    msg: &Message,
    arg: &str,
    pool: &AgentPool,
    pending_budgets: &PendingBudgetMap,
) -> Result<()> {
    let arg = arg.trim();
    if arg.is_empty() {
        bot.send_message(
            msg.chat.id,
            "Usage: /budget <preset|usd> or /budget status\npresets: quick, standard, deep, unlimited",
        )
        .await?;
        return Ok(());
    }
    if arg.eq_ignore_ascii_case("status") {
        return handle_budget_status(bot, msg, pool, pending_budgets).await;
    }
    let ov = match parse_budget_arg(arg) {
        Ok(ov) => ov,
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ {e}")).await?;
            return Ok(());
        }
    };
    let desc = describe_override(&ov);
    pending_budgets.lock().await.insert(msg.chat.id.0, ov);
    bot.send_message(
        msg.chat.id,
        format!("💰 budget set for next card: {desc}\nQueue a task now to apply it."),
    )
    .await?;
    Ok(())
}

/// Parse a `/budget` argument as either a named preset or a bare USD number.
fn parse_budget_arg(arg: &str) -> std::result::Result<BudgetOverride, String> {
    if let Some(preset) = lopi_core::BudgetPreset::parse(arg) {
        return Ok(BudgetOverride {
            preset: Some(preset),
            ..Default::default()
        });
    }
    match arg.parse::<f64>() {
        Ok(usd) if usd >= 0.0 => Ok(BudgetOverride {
            usd: Some(usd),
            ..Default::default()
        }),
        _ => Err(format!(
            "'{arg}' isn't a known preset (quick/standard/deep/unlimited) or a USD amount"
        )),
    }
}

async fn handle_budget_status(
    bot: &Bot,
    msg: &Message,
    pool: &AgentPool,
    pending_budgets: &PendingBudgetMap,
) -> Result<()> {
    let cfg = lopi_core::LoopConfig::load_from_repo(pool.repo_path()).unwrap_or_default();
    let base = cfg.resolved_budget();
    let pending = pending_budgets.lock().await.get(&msg.chat.id.0).cloned();
    let resolved = pending
        .as_ref()
        .map_or_else(|| base.clone(), |ov| ov.apply(base.clone()));
    let pending_line = pending.as_ref().map_or_else(
        || "(none — next card uses the repo default below)".to_string(),
        describe_override,
    );
    bot.send_message(
        msg.chat.id,
        format!(
            "💰 budget status\n\npending /budget override: {pending_line}\n\nresolved for the next card:\n  usd:    ${:.2}\n  tokens: {}\n  allow:  {}\n  deny:   {}",
            resolved.usd,
            resolved.tokens,
            fmt_tool_list(&resolved.allow),
            fmt_tool_list(&resolved.deny),
        ),
    )
    .await?;
    Ok(())
}

/// Render a tool list as `none` (empty) or a comma-joined summary — mirrors
/// `loop_commands.rs::fmt_list` for the CLI-side `lopi loop show`.
fn fmt_tool_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_budget_arg_accepts_a_known_preset() {
        let ov = parse_budget_arg("deep").unwrap();
        assert_eq!(ov.preset, Some(lopi_core::BudgetPreset::Deep));
        assert!(ov.usd.is_none());
    }

    #[test]
    fn parse_budget_arg_is_case_insensitive() {
        let ov = parse_budget_arg("QUICK").unwrap();
        assert_eq!(ov.preset, Some(lopi_core::BudgetPreset::Quick));
    }

    #[test]
    fn parse_budget_arg_accepts_a_bare_usd_amount() {
        let ov = parse_budget_arg("7.5").unwrap();
        assert_eq!(ov.usd, Some(7.5));
        assert!(ov.preset.is_none());
    }

    #[test]
    fn parse_budget_arg_rejects_a_negative_amount() {
        assert!(parse_budget_arg("-3").is_err());
    }

    #[test]
    fn parse_budget_arg_rejects_nonsense() {
        assert!(parse_budget_arg("banana").is_err());
    }

    #[test]
    fn describe_override_lists_every_set_field() {
        let ov = BudgetOverride {
            preset: Some(lopi_core::BudgetPreset::Deep),
            usd: Some(10.0),
            tokens: Some(5_000_000),
        };
        let desc = describe_override(&ov);
        assert!(desc.contains("preset=deep"));
        assert!(desc.contains("$10.00"));
        assert!(desc.contains("5000000 tokens"));
    }

    #[test]
    fn describe_override_empty_is_none() {
        assert_eq!(describe_override(&BudgetOverride::default()), "(none)");
    }

    #[test]
    fn fmt_tool_list_renders_none_or_joined() {
        assert_eq!(fmt_tool_list(&[]), "none");
        assert_eq!(
            fmt_tool_list(&["Workflow".to_string(), "Bash".to_string()]),
            "Workflow, Bash"
        );
    }

    #[tokio::test]
    async fn take_pending_budget_is_one_shot() {
        let map: PendingBudgetMap = Arc::new(Mutex::new(HashMap::new()));
        map.lock().await.insert(
            42,
            BudgetOverride {
                usd: Some(5.0),
                ..Default::default()
            },
        );
        let mut task = Task::new("t");
        let note = take_pending_budget(&map, 42, &mut task).await;
        assert!(note.contains("$5.00"));
        assert_eq!(task.budget_override.as_ref().and_then(|o| o.usd), Some(5.0));

        // Second call for the same chat finds nothing — already consumed.
        let mut task2 = Task::new("t2");
        let note2 = take_pending_budget(&map, 42, &mut task2).await;
        assert!(note2.is_empty());
        assert!(task2.budget_override.is_none());
    }
}
