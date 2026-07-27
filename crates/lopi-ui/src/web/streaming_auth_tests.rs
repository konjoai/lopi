// Sprint S11, Phase 0 — ticket-based browser auth for `/ws`, `/ws/tasks`,
// `/sse`. `include!`-ed into `tests.rs` (see that file's doc comment on
// `get_req`).

/// `POST /api/ws-ticket` with `Authorization: Bearer <token>`, returning the
/// minted ticket string.
async fn mint_ticket_authed(app: Router, token: &str) -> String {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ws-ticket")
                .header("Authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = json_body(resp).await;
    json["ticket"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn sse_without_token_is_401() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = get_req(app, "/sse").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn metrics_without_token_is_401() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = get_req(app, "/metrics").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_without_token_is_401_before_any_upgrade_is_attempted() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = get_req(app, "/ws").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_tasks_legacy_alias_without_token_is_401() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = get_req(app, "/ws/tasks").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ws_ticket_mint_itself_requires_the_real_bearer_token() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = send_req(app, "POST", "/api/ws-ticket", None).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_minted_ticket_lets_sse_through_exactly_once() {
    let app = test_app_with_auth(Some("secret")).await;
    let ticket = mint_ticket_authed(app.clone(), "secret").await;

    let first = get_req(app.clone(), &format!("/sse?ticket={ticket}")).await;
    assert_eq!(
        first.status(),
        StatusCode::OK,
        "a fresh ticket must let an unauthenticated SSE request through"
    );

    let second = get_req(app, &format!("/sse?ticket={ticket}")).await;
    assert_eq!(
        second.status(),
        StatusCode::UNAUTHORIZED,
        "a ticket must not be replayable after its first successful use"
    );
}

#[tokio::test]
async fn an_unknown_ticket_is_rejected() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = get_req(app, "/sse?ticket=not-a-real-ticket").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tickets_are_not_accepted_on_metrics() {
    // Tickets exist only because a browser can't set a header on a
    // WebSocket/EventSource upgrade — `/metrics` has no such constraint
    // (a Prometheus scraper sets whatever header it likes), so it must not
    // accept the ticket bypass at all.
    let app = test_app_with_auth(Some("secret")).await;
    let ticket = mint_ticket_authed(app.clone(), "secret").await;
    let resp = get_req(app, &format!("/metrics?ticket={ticket}")).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tickets_are_not_accepted_on_plain_api_routes() {
    let app = test_app_with_auth(Some("secret")).await;
    let ticket = mint_ticket_authed(app.clone(), "secret").await;
    let resp = get_req(app, &format!("/api/health?ticket={ticket}")).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn correct_bearer_still_works_on_streaming_routes_no_ticket_needed() {
    let app = test_app_with_auth(Some("secret")).await;
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/sse")
                .header("Authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn streaming_routes_pass_through_untouched_when_no_auth_token_is_configured() {
    // Dev mode / `--insecure-no-auth`: no auth_token means no check at all,
    // same as every other route — must not regress local dev.
    let app = test_app_with_auth(None).await;
    let resp = get_req(app, "/sse").await;
    assert_eq!(resp.status(), StatusCode::OK);
}
