//! Wire-payload builders — the Rust port of `web/src/lib/stores/
//! stack.ts`'s `cardToTaskPayload`/`cardToTaskPayloadForRunOnce`/
//! `paneSubmitPayload`.
//!
//! These target [`CreateTaskRequest`] directly, not a new intermediate DTO
//! (Sprint T0, Phase 1.4). They live in `lopi-ui` rather than
//! `lopi_core::stack` (which owns the pure domain types and catalogs) for
//! one reason: `CreateTaskRequest` is defined in `lopi-ui`
//! (`crate::web::types`), and `lopi-core` cannot depend on `lopi-ui` — that
//! dependency already runs the other way. Building the byte-identical
//! output this sprint's kill test (KT-T0.3) requires means the conversion
//! has to live wherever both `StackCard` and `CreateTaskRequest` are
//! reachable without a cycle, which is here. See `LEDGER.md` for this
//! sprint's placement decision.

use crate::web::types::CreateTaskRequest;
use lopi_core::stack::{
    autonomy_to_wire, budget_to_tokens, evals_to_acceptance, PaneDefaults, PaneLaunch, StackCard,
};

/// `"auto"` — the model-picker sentinel meaning "no override," matching
/// `web/src/lib/stores/options.ts::AUTO_MODEL`.
const AUTO_MODEL: &str = "auto";

/// `"bypassPermissions"` — the wire default an absent `permission_mode`
/// resolves to server-side, matching
/// `web/src/lib/stores/stackDefaults.ts::DEFAULT_PERMISSION_MODE`.
const DEFAULT_PERMISSION_MODE: &str = "bypassPermissions";

/// Convert a card's config + guardrails into the `CreateTaskRequest` a
/// full-run submission sends (`stack.ts::cardToTaskPayload`).
#[must_use]
pub fn card_to_task_payload(card: &StackCard, defaults: &PaneDefaults) -> CreateTaskRequest {
    let mut req = CreateTaskRequest {
        goal: card.goal.clone(),
        repo: Some(
            card.config
                .repo
                .clone()
                .unwrap_or_else(|| defaults.repo.clone()),
        ),
        priority: Some("normal".to_string()),
        constraints: None,
        allowed_dirs: None,
        forbidden_dirs: None,
        max_retries: None,
        require_plan_approval: None,
        verifier_required: None,
        verifier_model: None,
        verifier_effort: None,
        report: None,
        // The card pill's `0` means "off" — a single pass on the wire,
        // never the backend's `0` = infinite sentinel.
        max_iterations: Some(if card.max_iterations == 0 {
            1
        } else {
            card.max_iterations.min(u32::from(u8::MAX)) as u8
        }),
        autonomy_level: None,
        no_progress_limit: card.guardrails.no_progress_limit,
        isolation: card.guardrails.isolation.to_isolation_mode(),
        model: None,
        effort: card
            .config
            .effort
            .clone()
            .or_else(|| Some(defaults.effort.clone())),
        permission_mode: None,
        deliverable: None,
        gate: None,
        until: None,
        on_fail: Some(card.guardrails.on_fail),
        // Backend-1 — lets the response's `duplicate_of ?? id` be traced
        // straight back to this card regardless of any server-side dedup.
        client_ref: Some(card.id.clone()),
        acceptance: evals_to_acceptance(&card.evals),
        verifier_fail_open: None,
        budget_tokens: budget_to_tokens(card.guardrails.budget),
        budget_override: None,
    };

    // `auto` means "no override" — omit `model` so the backend's
    // `select_model` size heuristic runs, instead of sending the literal
    // string `"auto"` through to `task.model`'s override check.
    let resolved_model = card
        .config
        .model
        .clone()
        .unwrap_or_else(|| defaults.model.clone());
    if !resolved_model.is_empty() && resolved_model != AUTO_MODEL {
        req.model = Some(resolved_model);
    }

    // Never send the literal default (`bypassPermissions`) on the wire when
    // the field wasn't touched.
    let resolved_permission_mode = card
        .config
        .permission_mode
        .clone()
        .or_else(|| defaults.permission_mode.clone())
        .unwrap_or_else(|| DEFAULT_PERMISSION_MODE.to_string());
    if resolved_permission_mode != DEFAULT_PERMISSION_MODE {
        req.permission_mode = Some(resolved_permission_mode);
    }

    // Send only a live L1..L4 choice; an unresolvable/unset value is
    // omitted so the server keeps the repo's `.lopi/loop.toml`
    // `autonomy_level` as the sole source.
    let autonomy = card
        .config
        .autonomy
        .as_deref()
        .or(defaults.autonomy.as_deref());
    req.autonomy_level = autonomy_to_wire(autonomy);

    if card.guardrails.gate {
        req.gate = Some(card.guardrails.gate_cmd.clone());
    }
    if card.guardrails.until {
        req.until = Some(card.guardrails.until_cmd.clone());
    }

    // The real budget-preset override — orthogonal to the legacy token-cap
    // `budget` above. `Inherit` (untouched) omits the whole
    // `budget_override` object rather than sending an empty one.
    let preset = card.guardrails.budget_preset.to_budget_preset();
    if preset.is_some() || card.guardrails.budget_usd.is_some() {
        req.budget_override = Some(lopi_core::BudgetOverride {
            preset,
            usd: card.guardrails.budget_usd,
            tokens: None,
        });
    }

    // `branch` has no `CreateTaskRequest` field of its own — encode it into
    // `constraints`, same as `paneSubmitPayload`.
    let branch = card.config.branch.as_deref().or(defaults.branch.as_deref());
    if let Some(branch) = branch.map(str::trim).filter(|b| !b.is_empty()) {
        req.constraints = Some(vec![format!("Target branch: {branch}")]);
    }

    req
}

/// [`card_to_task_payload`], forced to a single pass — the run-once launch
/// path (`stack.ts::cardToTaskPayloadForRunOnce`).
#[must_use]
pub fn card_to_task_payload_for_run_once(
    card: &StackCard,
    defaults: &PaneDefaults,
) -> CreateTaskRequest {
    let mut req = card_to_task_payload(card, defaults);
    req.max_iterations = Some(1);
    req
}

/// Convert a bare-prompt input-bar submission into a `CreateTaskRequest`,
/// with no card/guardrails involved (`stack.ts::paneSubmitPayload`).
#[must_use]
pub fn pane_submit_payload(launch: &PaneLaunch) -> CreateTaskRequest {
    let mut req = CreateTaskRequest {
        goal: launch.goal.clone(),
        repo: Some(launch.repo.clone()),
        priority: Some(
            launch
                .priority
                .clone()
                .filter(|p| !p.is_empty())
                .unwrap_or_else(|| "normal".to_string()),
        ),
        constraints: None,
        allowed_dirs: None,
        forbidden_dirs: None,
        max_retries: None,
        require_plan_approval: None,
        verifier_required: None,
        verifier_model: None,
        verifier_effort: None,
        report: None,
        max_iterations: None,
        autonomy_level: None,
        no_progress_limit: None,
        isolation: None,
        model: None,
        effort: None,
        permission_mode: None,
        deliverable: None,
        gate: None,
        until: None,
        on_fail: None,
        client_ref: None,
        acceptance: None,
        verifier_fail_open: None,
        budget_tokens: None,
        budget_override: None,
    };

    if let Some(model) = launch
        .model
        .as_deref()
        .filter(|m| !m.is_empty() && *m != AUTO_MODEL)
    {
        req.model = Some(model.to_string());
    }
    if let Some(effort) = launch.effort.as_deref().filter(|e| !e.is_empty()) {
        req.effort = Some(effort.to_string());
    }
    if let Some(mode) = launch
        .permission_mode
        .as_deref()
        .filter(|m| !m.is_empty() && *m != DEFAULT_PERMISSION_MODE)
    {
        req.permission_mode = Some(mode.to_string());
    }
    if let Some(branch) = launch
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
    {
        req.constraints = Some(vec![format!("Target branch: {branch}")]);
    }

    req
}

#[cfg(test)]
#[path = "stack_payload_tests.rs"]
mod tests;
