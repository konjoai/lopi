use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use uuid::Uuid;

fn make_msg(tokens: usize) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role: Role::User,
        content: vec![ContentBlock::Text("x".repeat(tokens * 4))],
        tokens,
        pin: PinPolicy::BudgetEvictable,
        phase: Phase::Implementation,
        evict_after: None,
        tool_pair_id: None,
        is_conclusion: false,
    }
}

fn bench_evict_to_budget(c: &mut Criterion) {
    c.bench_function("evict_to_budget_100_turns", |b| {
        b.iter(|| {
            let mut window = ContextWindow::new(10_000);
            for _ in 0..100 {
                window.push(make_msg(150)).ok();
            }
            black_box(window.evict_to_budget(5_000).unwrap());
        });
    });
}

fn bench_to_api_messages(c: &mut Criterion) {
    c.bench_function("to_api_messages_1000_turns", |b| {
        let mut window = ContextWindow::new(1_000_000);
        for _ in 0..1000 {
            window.push(make_msg(100)).ok();
        }
        b.iter(|| {
            black_box(window.to_api_messages());
        });
    });
}

fn bench_push_under_pressure(c: &mut Criterion) {
    c.bench_function("push_at_75pct_pressure", |b| {
        b.iter(|| {
            let mut window = ContextWindow::new(1000);
            // Fill to ~75%.
            for _ in 0..7 {
                window.push(make_msg(100)).ok();
            }
            // This push may trigger auto-eviction.
            black_box(window.push(make_msg(100)));
        });
    });
}

criterion_group!(benches, bench_evict_to_budget, bench_to_api_messages, bench_push_under_pressure);
criterion_main!(benches);
