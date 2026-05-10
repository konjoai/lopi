//! Force-directed 2D layout for the Forge constellation view.
//!
//! Treats every live agent as a point in `[0, 1]² space:
//! - Repulsion between every pair (Coulomb-style, ~ 1 / d²).
//! - Attraction between pairs whose goals share Jaccard similarity ≥ τ
//!   (Hooke-style, proportional to similarity and distance).
//!
//! The simulation is deterministic: agents are seeded onto a unit circle by
//! task_id hash, then `ITERATIONS` Euler steps are run with a small dt.
//! Output is normalized to `[0, 1]²` so the frontend can draw into any
//! viewBox without further math.
//!
//! With at most ~16 live agents (the orchestrator's typical concurrency cap)
//! and an O(n²) pairwise loop, this runs in microseconds — no need for a
//! quadtree or async cooperation.
//!
//! Related agents are returned as `(a_idx, b_idx, similarity)` so the UI can
//! draw fleet lines without recomputing the goal-text similarity.

use std::collections::HashSet;

use lopi_orchestrator::LiveAgent;

const ITERATIONS: usize = 80;
const DT: f32 = 0.04;
const REPULSION: f32 = 0.012;
const ATTRACTION: f32 = 0.6;
const SIM_THRESHOLD: f32 = 0.20;
const MIN_DIST_SQ: f32 = 0.0004;
const DAMPING: f32 = 0.85;

/// One node in the laid-out constellation.
#[derive(Debug, Clone)]
pub(super) struct Node {
    pub task_id: String,
    pub goal: String,
    pub attempt: u32,
    pub elapsed_ms: u64,
    /// X coordinate in `[0.0, 1.0]`.
    pub x: f32,
    /// Y coordinate in `[0.0, 1.0]`.
    pub y: f32,
}

/// A fleet link between two nodes (`a_idx`, `b_idx` index into the node list).
#[derive(Debug, Clone)]
pub(super) struct Link {
    pub a: usize,
    pub b: usize,
    pub similarity: f32,
}

/// Compute the constellation layout for a list of live agents.
///
/// Returns nodes (with positions in `[0, 1]²`) and the set of fleet links.
pub(super) fn layout(agents: &[LiveAgent]) -> (Vec<Node>, Vec<Link>) {
    let n = agents.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }

    // Tokenize each goal once.
    let token_sets: Vec<HashSet<String>> = agents.iter().map(|a| tokenize(&a.goal)).collect();

    // Pre-compute pairwise similarity and the link list.
    let mut sim = vec![0.0_f32; n * n];
    let mut links = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let s = jaccard(&token_sets[i], &token_sets[j]);
            sim[i * n + j] = s;
            sim[j * n + i] = s;
            if s >= SIM_THRESHOLD {
                links.push(Link {
                    a: i,
                    b: j,
                    similarity: s,
                });
            }
        }
    }

    // Seed positions on a unit circle, deterministically by task_id hash.
    let mut pos: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let h = stable_hash(agents[i].task_id.0.as_bytes());
            let theta = (h as f32 / u64::MAX as f32) * std::f32::consts::TAU;
            (0.5 + 0.35 * theta.cos(), 0.5 + 0.35 * theta.sin())
        })
        .collect();
    let mut vel = vec![(0.0_f32, 0.0_f32); n];

    // Force-directed integration.
    for _ in 0..ITERATIONS {
        let mut forces = vec![(0.0_f32, 0.0_f32); n];
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = pos[j].0 - pos[i].0;
                let dy = pos[j].1 - pos[i].1;
                let d2 = (dx * dx + dy * dy).max(MIN_DIST_SQ);
                let inv_d = d2.sqrt().recip();
                let ux = dx * inv_d;
                let uy = dy * inv_d;

                // Repulsion: pushes i and j apart.
                let r = REPULSION / d2;
                forces[i].0 -= ux * r;
                forces[i].1 -= uy * r;
                forces[j].0 += ux * r;
                forces[j].1 += uy * r;

                // Attraction: only for similar pairs, scaled by similarity and distance.
                let s = sim[i * n + j];
                if s >= SIM_THRESHOLD {
                    let a = ATTRACTION * s * d2.sqrt();
                    forces[i].0 += ux * a;
                    forces[i].1 += uy * a;
                    forces[j].0 -= ux * a;
                    forces[j].1 -= uy * a;
                }
            }
        }
        for i in 0..n {
            vel[i].0 = (vel[i].0 + forces[i].0 * DT) * DAMPING;
            vel[i].1 = (vel[i].1 + forces[i].1 * DT) * DAMPING;
            pos[i].0 += vel[i].0 * DT;
            pos[i].1 += vel[i].1 * DT;
        }
    }

    // Normalize back into `[0.05, 0.95]` to keep margin around the viewport.
    normalize(&mut pos);

    let nodes: Vec<Node> = agents
        .iter()
        .zip(pos.iter())
        .map(|(a, (x, y))| Node {
            task_id: a.task_id.0.to_string(),
            goal: a.goal.clone(),
            attempt: a.attempt,
            elapsed_ms: a.elapsed_ms,
            x: *x,
            y: *y,
        })
        .collect();

    (nodes, links)
}

fn tokenize(goal: &str) -> HashSet<String> {
    goal.split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_lowercase())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn stable_hash(bytes: &[u8]) -> u64 {
    // FNV-1a 64-bit. Fast and deterministic across runs/platforms.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn normalize(pos: &mut [(f32, f32)]) {
    if pos.is_empty() {
        return;
    }
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    let (mut min_y, mut max_y) = (f32::MAX, f32::MIN);
    for &(x, y) in pos.iter() {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let range_x = (max_x - min_x).max(0.001);
    let range_y = (max_y - min_y).max(0.001);
    for p in pos.iter_mut() {
        p.0 = 0.05 + ((p.0 - min_x) / range_x) * 0.9;
        p.1 = 0.05 + ((p.1 - min_y) / range_y) * 0.9;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use lopi_core::TaskId;

    fn agent(goal: &str) -> LiveAgent {
        LiveAgent {
            task_id: TaskId::new(),
            goal: goal.to_string(),
            attempt: 1,
            elapsed_ms: 0,
        }
    }

    #[test]
    fn empty_input_returns_empty() {
        let (nodes, links) = layout(&[]);
        assert!(nodes.is_empty());
        assert!(links.is_empty());
    }

    #[test]
    fn single_agent_lands_inside_unit_box() {
        let a = agent("fix flaky test");
        let (nodes, _) = layout(&[a]);
        assert_eq!(nodes.len(), 1);
        assert!(nodes[0].x >= 0.0 && nodes[0].x <= 1.0);
        assert!(nodes[0].y >= 0.0 && nodes[0].y <= 1.0);
    }

    #[test]
    fn similar_goals_produce_a_link() {
        let a = agent("fix flaky integration test for billing module");
        let b = agent("fix flaky integration test for invoicing module");
        let c = agent("rebuild the marketing landing page hero gradient");
        let (_, links) = layout(&[a, b, c]);
        // a–b are clearly similar (share fix/flaky/integration/test/module); a–c and b–c are not.
        assert!(links.iter().any(|l| (l.a == 0 && l.b == 1) || (l.a == 1 && l.b == 0)));
        // No link should involve agent c (idx 2) here.
        assert!(!links.iter().any(|l| l.a == 2 || l.b == 2));
    }

    #[test]
    fn jaccard_handles_empty_sets() {
        let a: HashSet<String> = HashSet::new();
        let b: HashSet<String> = HashSet::new();
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn layout_is_deterministic_for_same_inputs() {
        let id1 = TaskId::new();
        let id2 = TaskId::new();
        let mk = || {
            vec![
                LiveAgent {
                    task_id: id1,
                    goal: "alpha task one shared".into(),
                    attempt: 1,
                    elapsed_ms: 0,
                },
                LiveAgent {
                    task_id: id2,
                    goal: "alpha task one shared".into(),
                    attempt: 1,
                    elapsed_ms: 0,
                },
            ]
        };
        let (n1, _) = layout(&mk());
        let (n2, _) = layout(&mk());
        assert!((n1[0].x - n2[0].x).abs() < 1e-5);
        assert!((n1[1].y - n2[1].y).abs() < 1e-5);
    }
}
