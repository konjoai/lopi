// Backend-1 Phase 3: per-card event routing proof.
//
// There is no server-side "stack"/"plan" concept (confirmed by the UI-2
// V&V audit) — isolation between concurrently-running loop-stack cards is
// entirely a property of `stream_task`'s `event_task_id` filter over the
// one shared broadcast bus every task's events land on. This proves that
// filter end-to-end: two concurrent SSE subscriptions on the same bus,
// interleaved events for two different task ids, and an explicit
// cross-talk count asserted at zero — not just "looks fine in the logs".

/// Like `test_app_with_store`, but also hands back the `EventBus` handle
/// so a test can `bus.send(..)` directly instead of driving a real task
/// through the pool just to get events onto it.
async fn test_app_with_bus() -> (Router, EventBus<AgentEvent>) {
    let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let queue = TaskQueue::new();
    let pool = Arc::new(AgentPool::new(
        1,
        PathBuf::from("."),
        queue.clone(),
        bus.clone(),
    ));
    let state = AppState::new(store, bus.clone(), queue, pool, None);
    (build_app(state), bus)
}

/// Read `data: ...` SSE lines off `body` until `want` have arrived. Bounded
/// by an overall deadline so a routing regression that drops events fails
/// the test outright instead of hanging the suite.
async fn collect_sse_lines(body: axum::body::Body, want: usize) -> Vec<String> {
    use futures::StreamExt as _;
    let mut stream = body.into_data_stream();
    let mut lines = Vec::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while lines.len() < want {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            !remaining.is_zero(),
            "timed out waiting for {want} SSE lines, only got {}: {lines:?}",
            lines.len()
        );
        let chunk = tokio::time::timeout(remaining, stream.next())
            .await
            .expect("timed out reading the next SSE chunk")
            .expect("SSE body stream ended before enough lines arrived")
            .expect("SSE body stream yielded an error frame");
        for line in String::from_utf8_lossy(&chunk).lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                lines.push(data.to_string());
            }
        }
    }
    lines
}

#[tokio::test]
async fn task_stream_isolates_concurrent_tasks_with_zero_cross_talk() {
    let (app, bus) = test_app_with_bus().await;
    let task_a = lopi_core::TaskId(uuid::Uuid::new_v4());
    let task_b = lopi_core::TaskId(uuid::Uuid::new_v4());

    // `stream_task` calls `bus.subscribe()` synchronously inside the
    // handler body before it ever constructs the `Sse` response, so by the
    // time these two `oneshot` calls resolve, both subscriptions already
    // exist on the bus — no sleep/yield needed to avoid a
    // subscribe-after-send race.
    let resp_a = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/tasks/{}/stream", task_a.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let resp_b = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/tasks/{}/stream", task_b.0))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp_a.status(), StatusCode::OK);
    assert_eq!(resp_b.status(), StatusCode::OK);

    let body_a = resp_a.into_body();
    let body_b = resp_b.into_body();

    const N: usize = 10;
    for i in 0..N {
        // Interleaved on purpose: A's and B's events land on the shared
        // bus back-to-back, so isolation is proven under concurrency, not
        // just "the events happened to arrive in separate batches".
        bus.send(AgentEvent::info(task_a, format!("line-a-{i}")));
        bus.send(AgentEvent::info(task_b, format!("line-b-{i}")));
    }

    let (lines_a, lines_b) = tokio::join!(
        collect_sse_lines(body_a, N),
        collect_sse_lines(body_b, N)
    );

    let cross_talk_a = lines_a.iter().filter(|l| l.contains("line-b-")).count();
    let cross_talk_b = lines_b.iter().filter(|l| l.contains("line-a-")).count();
    assert_eq!(
        cross_talk_a, 0,
        "task A's stream must never surface task B's events"
    );
    assert_eq!(
        cross_talk_b, 0,
        "task B's stream must never surface task A's events"
    );
    assert!(
        lines_a.iter().all(|l| l.contains("line-a-")),
        "every line on A's stream is actually A's: {lines_a:?}"
    );
    assert!(
        lines_b.iter().all(|l| l.contains("line-b-")),
        "every line on B's stream is actually B's: {lines_b:?}"
    );
}
