// ─── Loop/verifier/report/override field exposure (web task-create surface) ──

#[tokio::test]
async fn create_task_with_loop_fields_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "verified capped loop",
        "verifier_required": true,
        "verifier_model": "claude-opus-4-7",
        "verifier_effort": "high",
        "report": "telegram",
        "max_iterations": 5,
        "model": "claude-haiku-4-5-20251001",
        "effort": "low",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn create_task_rejects_unreachable_report_channel() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "report to whatsapp",
        "report": "whatsapp",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("inbound-only"),
        "must surface the existing typed ReportChannelError, not a generic message"
    );
}

#[test]
fn apply_loop_fields_leaves_task_unchanged_when_all_fields_are_absent() {
    let mut task = Task::new("plain task");
    let baseline = format!("{task:?}");
    let req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "plain task"})).unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        format!("{task:?}"),
        baseline,
        "no new field may change defaults when omitted"
    );
}

#[test]
fn apply_loop_fields_threads_verifier_overrides_through_exactly() {
    let mut task = Task::new("verified task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "verified task",
        "verifier_required": true,
        "verifier_model": "opus",
        "verifier_effort": "high",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert!(task.verifier_required);
    assert_eq!(task.verifier_model.as_deref(), Some("opus"));
    assert_eq!(task.verifier_effort.as_deref(), Some("high"));
}

#[test]
fn apply_loop_fields_threads_budget_override_through() {
    // A card pinning `standard` must override the repo config and deny the
    // fan-out tools — the card-level lever against sub-agent cost blowup.
    let mut task = Task::new("cheap capped card");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "cheap capped card",
        "budget_override": { "preset": "standard", "usd": 1.0 },
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    let ov = task.budget_override.expect("override must be applied");
    assert_eq!(ov.preset, Some(lopi_core::BudgetPreset::Standard));
    assert_eq!(ov.usd, Some(1.0));
    // `standard` denies the fan-out tools when resolved against any base.
    let resolved = ov.apply(lopi_core::BudgetPreset::Deep.resolved());
    assert_eq!(resolved.deny, vec!["Workflow", "Task", "Agent"]);
}

#[test]
fn apply_loop_fields_threads_deliverable_override_through() {
    let mut task = Task::new("review the auth module");
    // Without an explicit override, this goal infers review-only; assert the
    // API can still pin it explicitly and that it round-trips as snake_case.
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "review the auth module",
        "deliverable": "file_changes",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.deliverable, Some(lopi_core::Deliverable::FileChanges));
    // The explicit override wins over the goal-text inference.
    assert_eq!(task.deliverable_kind(), lopi_core::Deliverable::FileChanges);
}

#[test]
fn apply_loop_fields_accepts_telegram_and_rejects_whatsapp() {
    let mut telegram_task = Task::new("t");
    let telegram_req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "t", "report": "telegram"})).unwrap();
    assert!(apply_loop_fields(&mut telegram_task, &telegram_req).is_ok());
    assert_eq!(telegram_task.report.as_deref(), Some("telegram"));

    let mut whatsapp_task = Task::new("w");
    let whatsapp_req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "w", "report": "whatsapp"})).unwrap();
    let err = apply_loop_fields(&mut whatsapp_task, &whatsapp_req).unwrap_err();
    assert_eq!(err, lopi_core::ReportChannelError::WhatsappUnsupported);
    assert_eq!(
        whatsapp_task.report, None,
        "task must not be mutated on a rejected report channel"
    );
}

#[test]
fn apply_loop_fields_accepts_zero_max_iterations_as_the_infinite_sentinel() {
    let mut task = Task::new("infinite task");
    let req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "infinite task", "max_iterations": 0}))
            .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.max_iterations,
        Some(0),
        "0 must flow through as the infinite sentinel, not be rejected or coerced"
    );
}

#[test]
fn apply_loop_fields_threads_model_and_effort_overrides() {
    let mut task = Task::new("overridden task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "overridden task",
        "model": "claude-opus-4-7",
        "effort": "max",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.model.as_deref(), Some("claude-opus-4-7"));
    assert_eq!(task.effort.as_deref(), Some("max"));
}

#[test]
fn apply_loop_fields_omitting_model_lets_select_models_heuristic_choose() {
    // Mirrors what the UI's `auto` model option does: omit `model` from the
    // wire request entirely (never send the literal string `"auto"`, which
    // `select_model`'s `task.model` override check would pass straight to
    // the CLI as `--model auto` and fail). This proves the whole chain a
    // live task launch actually exercises — request → `apply_loop_fields` →
    // `select_model` — resolves to the size heuristic, not a hardcoded model.
    let mut task = Task::new("fix a typo");
    let req: CreateTaskRequest =
        serde_json::from_value(serde_json::json!({"goal": "fix a typo"})).unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.model, None,
        "an absent `model` key must leave task.model as None, never the string \"auto\""
    );
    // 0 constraints + 2 default allowed_dirs = size 2 → Haiku, same shape
    // `select_model_haiku_for_minimal_task` pins in lopi-agent::claude.
    assert_eq!(
        lopi_agent::select_model(&task, 0),
        lopi_agent::MODEL_HAIKU,
        "a task with no model override resolves through select_model's size heuristic"
    );
}

#[test]
fn apply_loop_fields_threads_gate_until_and_on_fail() {
    let mut task = Task::new("guarded task");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "guarded task",
        "gate": "./preflight.sh",
        "until": "cargo test",
        "on_fail": "continue",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(task.gate.as_deref(), Some("./preflight.sh"));
    assert_eq!(task.until.as_deref(), Some("cargo test"));
    assert_eq!(task.on_fail, Some(lopi_core::loop_config::OnFail::Continue));
}

#[tokio::test]
async fn create_task_with_guardrail_fields_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "gated loop",
        "gate": "true",
        "until": "false",
        "on_fail": "backoff",
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// `acceptance`/`verifier_fail_open`/`budget_tokens` were wired into
// `apply_loop_fields` (Sprint A1/A3) but never exercised by any request body
// — `get_task`'s response deliberately exposes only a small fixed field set
// (id/goal/status/created_at/completed_at/client_ref/cost, see `get_task`
// above), so a `POST` → `GET` round trip can't observe these three even
// where they DO persist; the field-mapping logic itself is what's actually
// verifiable, matching this file's existing `apply_loop_fields_threads_*`
// pattern for gate/until/on_fail above.
#[test]
fn apply_loop_fields_threads_acceptance_verifier_fail_open_and_budget_tokens() {
    let mut task = Task::new("budgeted task with an explicit acceptance gate");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "budgeted task with an explicit acceptance gate",
        "acceptance": { "checks": [] },
        "verifier_fail_open": true,
        "budget_tokens": 50_000,
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.acceptance,
        Some(lopi_core::acceptance::Acceptance::empty()),
        "an explicit (even empty) acceptance must land on the task, not be dropped"
    );
    assert!(
        task.verifier_fail_open,
        "an explicit true must overwrite the false default"
    );
    assert_eq!(
        task.budget_tokens, 50_000,
        "an explicit budget must overwrite the 0 (\"inherits repo/global budget\") default"
    );
}

#[test]
fn apply_loop_fields_omitting_acceptance_verifier_fail_open_and_budget_tokens_keeps_defaults() {
    let mut task = Task::new("no overrides here");
    let req: CreateTaskRequest = serde_json::from_value(serde_json::json!({
        "goal": "no overrides here",
    }))
    .unwrap();
    apply_loop_fields(&mut task, &req).unwrap();
    assert_eq!(
        task.acceptance, None,
        "an omitted acceptance must not be fabricated"
    );
    assert!(
        !task.verifier_fail_open,
        "an omitted verifier_fail_open keeps Task::new's false default"
    );
    assert_eq!(
        task.budget_tokens, 0,
        "an omitted budget_tokens keeps the 0 sentinel (\"inherits repo/global budget\")"
    );
}

#[tokio::test]
async fn create_task_with_acceptance_verifier_fail_open_and_budget_tokens_returns_201() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({
        "goal": "budgeted task with an explicit acceptance gate",
        "acceptance": { "checks": [] },
        "verifier_fail_open": true,
        "budget_tokens": 50_000,
    }))
    .unwrap();
    let resp = send_req(app, "POST", "/api/tasks", Some(body)).await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "the wire format for all three fields must actually deserialize, not just the pure struct-level call above"
    );
}

// `approve_plan`/`reject_plan` (Phase 11 plan-approval gate) had zero test
// coverage — neither the "unknown task" 404 nor the "task isn't currently
// paused awaiting approval" conflict path was exercised. A real approve of a
// *live* paused runner needs a real `claude` subprocess (covered instead at
// the pool level by `AgentPool::decide_plan`'s unit tests), but both HTTP
// error paths are reachable with no live agent at all.
#[tokio::test]
async fn approve_plan_unknown_task_returns_404() {
    let app = test_app().await;
    let resp = send_req(app, "POST", "/api/tasks/not-a-real-task/plan/approve", None).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reject_plan_unknown_task_returns_404() {
    let app = test_app().await;
    let resp = send_req(app, "POST", "/api/tasks/not-a-real-task/plan/reject", None).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn approve_plan_task_not_awaiting_approval_returns_409() {
    let app = test_app().await;
    // A well-formed UUID resolves without a store lookup (see
    // `resolve_task_id`), but no agent is running under it — the pool has no
    // handle, so there's nothing paused to approve.
    let uri = format!("/api/tasks/{}/plan/approve", uuid::Uuid::new_v4());
    let resp = send_req(app, "POST", &uri, None).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn reject_plan_task_not_awaiting_approval_returns_409() {
    let app = test_app().await;
    let uri = format!("/api/tasks/{}/plan/reject", uuid::Uuid::new_v4());
    let resp = send_req(app, "POST", &uri, None).await;
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn checkpoint_agent_persists_row_returns_201() {
    let app = test_app().await;
    let task_uuid = uuid::Uuid::new_v4();
    let body = serde_json::to_string(&serde_json::json!({
        "state": "planning",
        "attempt": 1,
        "last_plan": "step 1\nstep 2",
        "repo_path": "/tmp/repo",
    }))
    .unwrap();
    let resp = send_req(
        app,
        "POST",
        &format!("/api/agents/{task_uuid}/checkpoint"),
        Some(body),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json.get("checkpoint_id").is_some(),
        "response carries new id"
    );
    assert_eq!(json["task_id"], task_uuid.to_string());
}

#[tokio::test]
async fn checkpoint_agent_rejects_non_uuid_returns_400() {
    let app = test_app().await;
    let body = serde_json::to_string(&serde_json::json!({"state": "planning"})).unwrap();
    let resp = send_req(app, "POST", "/api/agents/not-a-uuid/checkpoint", Some(body)).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_task_not_found_returns_404() {
    let app = test_app().await;
    let resp = get_req(app, "/api/tasks/nonexistent-task-id").await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("error").is_some());
}

#[tokio::test]
async fn cancel_task_not_found_returns_404() {
    let app = test_app().await;
    let resp = send_req(app, "DELETE", "/api/tasks/nonexistent-task-id", None).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn models_returns_a_valid_catalog() {
    // No mocking of the outbound Anthropic call — whether this exercises the
    // live path or the fallback depends on whether ANTHROPIC_API_KEY happens
    // to be set in the test environment. Assert on the shape both paths
    // guarantee, not on exact fallback content, so the test is correct
    // either way (see `model_handlers`'s module doc).
    let app = test_app().await;
    let resp = get_req(app, "/api/models").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let models = json["models"].as_array().expect("models is an array");
    assert!(
        !models.is_empty(),
        "the dropdown must never see an empty catalog"
    );
    for m in models {
        assert!(
            m["id"].as_str().is_some_and(|s| !s.is_empty()),
            "every model has a non-empty id"
        );
        assert!(
            m["display_name"].as_str().is_some_and(|s| !s.is_empty()),
            "every model has a non-empty display_name"
        );
        assert!(
            m["effort"].as_array().is_some_and(|a| !a.is_empty()),
            "every model has at least one effort tier"
        );
    }
}

#[tokio::test]
async fn plans_returns_four_tiers() {
    let app = test_app().await;
    let resp = get_req(app, "/api/plans").await;
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let plans = json["plans"].as_array().expect("plans is an array");
    assert_eq!(plans.len(), 4);
    let ids: Vec<&str> = plans.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["free", "starter", "growth", "enterprise"]);
}

#[tokio::test]
async fn plans_response_has_required_fields() {
    let app = test_app().await;
    let resp = get_req(app, "/api/plans").await;
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for plan in json["plans"].as_array().unwrap() {
        assert!(plan.get("id").is_some(), "plan has id");
        assert!(plan.get("name").is_some(), "plan has name");
        assert!(plan.get("price_usd_per_month").is_some(), "plan has price");
        assert!(plan.get("max_agents").is_some(), "plan has max_agents");
        assert!(plan.get("features").is_some(), "plan has features");
        let max = plan["max_agents"].as_u64().unwrap();
        assert!(max >= 1, "max_agents at least 1");
    }
}

