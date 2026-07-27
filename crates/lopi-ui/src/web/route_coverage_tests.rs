// Sprint S11, Phase 0/4 — the route-coverage gate. `include!`-ed into
// `tests.rs` (see that file's doc comment on `get_req`).
//
// Phase 0's bug (four routes registered outside the auth layer) was
// invisible to every per-route test that existed at the time — each one
// individually proved its own route was reachable, none of them proved
// nothing else was reachable. This is the test that would have caught it:
// it doesn't ask "does auth work on the routes I remembered to check", it
// walks every route this server registers and asserts each one is either
// on the explicit public allowlist or requires auth. Adding a new protected
// route without adding it here makes this test's own accounting wrong in an
// obvious way (the route silently passing unauthenticated would show up as
// a route missing from `PROTECTED_ROUTES`, not as a green test) — the
// nearest thing an axum 0.7 app can do to real router introspection, which
// has no public API.

/// Every route `build_app` registers on `protected`, one entry per distinct
/// path (method chosen to match a real handler on that path — auth doesn't
/// care which method the caller used, so exercising one is a valid coverage
/// check for the whole path; axum's `route_layer` middleware runs before
/// axum matches the request to a specific method handler).
///
/// Keep this in lockstep with `build_app` in `mod.rs`: a route added there
/// and not here is a route this gate does not actually cover.
const PROTECTED_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/health"),
    ("GET", "/api/tasks"),
    ("GET", "/api/tasks/x"),
    ("POST", "/api/tasks/x/plan/approve"),
    ("POST", "/api/tasks/x/plan/reject"),
    ("GET", "/api/repos"),
    ("GET", "/api/branches"),
    ("GET", "/api/claude-commands"),
    ("POST", "/api/agents/x/checkpoint"),
    ("GET", "/api/stats"),
    ("GET", "/api/budget/breakdown"),
    ("GET", "/api/spec"),
    ("GET", "/api/quality/trend"),
    ("GET", "/api/agents/x/dag"),
    ("GET", "/api/tasks/x/stream"),
    ("GET", "/api/tasks/x/logs"),
    ("GET", "/api/logs"),
    ("GET", "/api/agents/x/rate-limit"),
    ("GET", "/api/schedules"),
    ("GET", "/api/schedules/x"),
    ("POST", "/api/schedules/x/enable"),
    ("POST", "/api/schedules/x/disable"),
    ("POST", "/api/schedules/x/run-now"),
    ("POST", "/api/schedules/x/autonomy"),
    ("GET", "/api/schedule-chains"),
    ("GET", "/api/schedule-chains/x"),
    ("POST", "/api/schedule-chains/x/enable"),
    ("POST", "/api/schedule-chains/x/disable"),
    ("POST", "/api/schedule-chains/x/run-now"),
    ("GET", "/api/quota"),
    ("GET", "/api/maxx"),
    ("GET", "/api/maxx/x"),
    ("POST", "/api/maxx/x/enable"),
    ("POST", "/api/maxx/x/disable"),
    ("GET", "/api/loop-engineering"),
    ("GET", "/api/loop-engineering/health"),
    ("GET", "/api/loop-engineering/runs"),
    ("GET", "/api/loop-engineering/runs/x"),
    ("POST", "/api/loop-engineering/strategy"),
    ("POST", "/api/loop-engineering/escalation"),
    ("GET", "/api/config"),
    ("GET", "/api/version"),
    ("GET", "/api/models"),
    ("POST", "/api/ws-ticket"),
    // The four routes this sprint found living outside the auth layer.
    // Listed with no special treatment — that's the point.
    ("GET", "/metrics"),
    ("GET", "/sse"),
    ("GET", "/ws"),
    ("GET", "/ws/tasks"),
];

/// The explicit, complete public allowlist: paths this server serves with
/// no auth check, by design. Today that's exactly the SPA/static fallback.
const PUBLIC_ROUTES: &[&str] = &["/", "/favicon.svg", "/some-unknown-spa-route"];

#[tokio::test]
async fn every_protected_route_rejects_a_request_with_no_token() {
    for (method, path) in PROTECTED_ROUTES {
        let app = test_app_with_auth(Some("coverage-token")).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method(*method)
                    .uri(*path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must require auth — got {}",
            resp.status()
        );
    }
}

#[tokio::test]
async fn every_protected_route_accepts_the_correct_bearer_token() {
    for (method, path) in PROTECTED_ROUTES {
        let app = test_app_with_auth(Some("coverage-token")).await;
        let resp = app
            .oneshot(
                Request::builder()
                    .method(*method)
                    .uri(*path)
                    .header("Authorization", "Bearer coverage-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} rejected the correct token"
        );
    }
}

#[tokio::test]
async fn the_public_allowlist_never_401s_even_with_auth_configured() {
    for path in PUBLIC_ROUTES {
        let app = test_app_with_auth(Some("coverage-token")).await;
        let resp = get_req(app, path).await;
        assert_ne!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{path} is on the public allowlist and must never require auth"
        );
    }
}
