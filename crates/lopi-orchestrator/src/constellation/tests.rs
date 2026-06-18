#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use chrono::Utc;

fn member(id: &str, weight: f32, tags: &[&str], max_concurrent: u8) -> ConstellationMember {
    ConstellationMember {
        agent_id: id.into(),
        weight,
        tags: tags.iter().map(|t| (*t).to_string()).collect(),
        max_concurrent,
    }
}

fn make(name: &str, strat: RoutingStrategy, members: Vec<ConstellationMember>) -> Constellation {
    Constellation {
        name: name.into(),
        agents: members,
        routing_strategy: strat,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn unknown_constellation_returns_error() {
    let r = ConstellationRouter::new();
    let err = r.dispatch("does-not-exist", &[]).await.unwrap_err();
    assert!(matches!(err, RoutingError::UnknownConstellation(_)));
}

#[tokio::test]
async fn empty_constellation_returns_error() {
    let r = ConstellationRouter::new();
    r.register(make("empty", RoutingStrategy::RoundRobin, vec![]))
        .await;
    let err = r.dispatch("empty", &[]).await.unwrap_err();
    assert!(matches!(err, RoutingError::Empty(_)));
}

#[tokio::test]
async fn round_robin_visits_each_member_in_order() {
    let r = ConstellationRouter::new();
    r.register(make(
        "rr",
        RoutingStrategy::RoundRobin,
        vec![
            member("a", 1.0, &[], 0),
            member("b", 1.0, &[], 0),
            member("c", 1.0, &[], 0),
        ],
    ))
    .await;
    let a = r.dispatch("rr", &[]).await.unwrap().agent_id;
    let b = r.dispatch("rr", &[]).await.unwrap().agent_id;
    let c = r.dispatch("rr", &[]).await.unwrap().agent_id;
    let d = r.dispatch("rr", &[]).await.unwrap().agent_id;
    assert_eq!(a, "a");
    assert_eq!(b, "b");
    assert_eq!(c, "c");
    assert_eq!(d, "a", "round-robin must wrap");
}

#[tokio::test]
async fn least_loaded_picks_lowest_in_flight() {
    let r = ConstellationRouter::new();
    r.register(make(
        "ll",
        RoutingStrategy::LeastLoaded,
        vec![member("a", 1.0, &[], 0), member("b", 1.0, &[], 0)],
    ))
    .await;
    // First dispatch — both tied at 0, picks first.
    assert_eq!(r.dispatch("ll", &[]).await.unwrap().agent_id, "a");
    // Now a=1, b=0 → next pick should be b.
    assert_eq!(r.dispatch("ll", &[]).await.unwrap().agent_id, "b");
    // Now a=1, b=1 → tied, picks first (a).
    assert_eq!(r.dispatch("ll", &[]).await.unwrap().agent_id, "a");
}

#[tokio::test]
async fn release_decrements_in_flight() {
    let r = ConstellationRouter::new();
    r.register(make(
        "rel",
        RoutingStrategy::LeastLoaded,
        vec![member("a", 1.0, &[], 0)],
    ))
    .await;
    r.dispatch("rel", &[]).await.unwrap();
    r.dispatch("rel", &[]).await.unwrap();
    let s = r.stats("rel").await.unwrap();
    assert_eq!(s.members[0].in_flight, 2);
    r.release("rel", "a").await;
    let s2 = r.stats("rel").await.unwrap();
    assert_eq!(s2.members[0].in_flight, 1);
    // Release more than dispatched should not underflow.
    r.release("rel", "a").await;
    r.release("rel", "a").await;
    let s3 = r.stats("rel").await.unwrap();
    assert_eq!(s3.members[0].in_flight, 0);
}

#[tokio::test]
async fn tag_match_filters_to_required_tags() {
    let r = ConstellationRouter::new();
    r.register(make(
        "tm",
        RoutingStrategy::TagMatch {
            required_tags: vec!["rust".into()],
        },
        vec![
            member("a", 1.0, &["python"], 0),
            member("b", 1.0, &["rust", "fast"], 0),
            member("c", 1.0, &["rust"], 0),
        ],
    ))
    .await;
    // Only b and c have the "rust" tag → least-loaded between them
    // picks b first, then c.
    let first = r.dispatch("tm", &[]).await.unwrap().agent_id;
    assert!(first == "b" || first == "c");
    // Either way, picking again should round between b and c.
    let second = r.dispatch("tm", &[]).await.unwrap().agent_id;
    assert!(second == "b" || second == "c");
}

#[tokio::test]
async fn extra_required_tags_intersect_with_eligibility() {
    let r = ConstellationRouter::new();
    r.register(make(
        "extra",
        RoutingStrategy::RoundRobin,
        vec![
            member("a", 1.0, &["fast"], 0),
            member("b", 1.0, &["fast", "secure"], 0),
            member("c", 1.0, &["secure"], 0),
        ],
    ))
    .await;
    let chosen = r
        .dispatch("extra", &["fast".into(), "secure".into()])
        .await
        .unwrap()
        .agent_id;
    assert_eq!(chosen, "b", "only b has both fast and secure tags");
}

#[tokio::test]
async fn max_concurrent_excludes_saturated_member() {
    let r = ConstellationRouter::new();
    r.register(make(
        "cap",
        RoutingStrategy::RoundRobin,
        vec![member("a", 1.0, &[], 1), member("b", 1.0, &[], 1)],
    ))
    .await;
    // Two dispatches use up both members' single slot.
    let _ = r.dispatch("cap", &[]).await.unwrap();
    let _ = r.dispatch("cap", &[]).await.unwrap();
    // Third one should return NoEligibleMember.
    let err = r.dispatch("cap", &[]).await.unwrap_err();
    assert!(matches!(err, RoutingError::NoEligibleMember));
}

#[tokio::test]
async fn stats_lists_every_member_with_dispatched_total() {
    let r = ConstellationRouter::new();
    r.register(make(
        "ss",
        RoutingStrategy::RoundRobin,
        vec![member("a", 1.0, &[], 0), member("b", 1.0, &[], 0)],
    ))
    .await;
    let _ = r.dispatch("ss", &[]).await.unwrap();
    let _ = r.dispatch("ss", &[]).await.unwrap();
    let _ = r.dispatch("ss", &[]).await.unwrap();
    let s = r.stats("ss").await.unwrap();
    assert_eq!(s.members.len(), 2);
    let total: u64 = s.members.iter().map(|m| m.dispatched_total).sum();
    assert_eq!(total, 3);
    assert_eq!(s.recent_decisions.len(), 3);
}

#[tokio::test]
async fn list_returns_registered_specs() {
    let r = ConstellationRouter::new();
    r.register(make("first", RoutingStrategy::RoundRobin, vec![]))
        .await;
    r.register(make("second", RoutingStrategy::RoundRobin, vec![]))
        .await;
    let listed = r.list().await;
    let mut names: Vec<_> = listed.iter().map(|c| c.name.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["first", "second"]);
}

#[tokio::test]
async fn weight_zero_member_is_skipped() {
    let r = ConstellationRouter::new();
    r.register(make(
        "wz",
        RoutingStrategy::RoundRobin,
        vec![member("a", 0.0, &[], 0), member("b", 1.0, &[], 0)],
    ))
    .await;
    // Only b is eligible.
    for _ in 0..3 {
        assert_eq!(r.dispatch("wz", &[]).await.unwrap().agent_id, "b");
    }
}

#[tokio::test]
async fn re_register_replaces_in_place() {
    let r = ConstellationRouter::new();
    r.register(make(
        "dup",
        RoutingStrategy::RoundRobin,
        vec![member("a", 1.0, &[], 0)],
    ))
    .await;
    let replaced = r
        .register(make(
            "dup",
            RoutingStrategy::RoundRobin,
            vec![member("b", 1.0, &[], 0)],
        ))
        .await;
    assert!(
        replaced,
        "register should report it replaced an existing entry"
    );
    let listed = r.list().await;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].agents[0].agent_id, "b");
}

#[tokio::test]
async fn qlearned_dispatch_is_labelled() {
    let r = ConstellationRouter::new();
    r.register(make(
        "lbl",
        RoutingStrategy::QLearned,
        vec![member("a", 1.0, &[], 0)],
    ))
    .await;
    let decision = r.dispatch("lbl", &[]).await.unwrap();
    assert_eq!(decision.strategy, "q_learned");
}

#[tokio::test]
async fn record_outcome_updates_q_table() {
    let r = ConstellationRouter::new();
    r.register(make(
        "rl",
        RoutingStrategy::QLearned,
        vec![member("a", 1.0, &[], 0), member("b", 1.0, &[], 0)],
    ))
    .await;
    r.record_outcome("rl", "b", 1.0);
    let cell = r
        .q_snapshot()
        .into_iter()
        .find(|e| e.state == "rl" && e.action == "b")
        .expect("q cell for (rl, b)");
    assert!(cell.q > 0.0, "reward should move the estimate above zero");
    assert_eq!(cell.updates, 1);
}

#[tokio::test]
async fn qlearned_favours_highest_reward_member() {
    let r = ConstellationRouter::new();
    r.register(make(
        "ql",
        RoutingStrategy::QLearned,
        vec![
            member("a", 1.0, &[], 0),
            member("b", 1.0, &[], 0),
            member("c", 1.0, &[], 0),
        ],
    ))
    .await;
    // Teach the router that "b" is the best agent for this constellation.
    r.record_outcome("ql", "b", 1.0);

    // With ε = 0.1 default exploration, "b" should win the strong majority
    // of dispatches. This is statistical, with a wide safety margin.
    let mut b_count = 0;
    for _ in 0..200 {
        if r.dispatch("ql", &[]).await.unwrap().agent_id == "b" {
            b_count += 1;
        }
    }
    assert!(
        b_count > 120,
        "expected the high-reward member to dominate, got {b_count}/200"
    );
}
