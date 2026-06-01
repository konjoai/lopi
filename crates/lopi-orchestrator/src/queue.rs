use dashmap::DashMap;
use lopi_core::{Priority, Task, TaskId};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Eq, PartialEq)]
struct PrioEntry {
    priority: Priority,
    seq: u64,
    id: TaskId,
}

impl Ord for PrioEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first; lower seq (older) first as tiebreaker.
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for PrioEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Thread-safe priority task queue — push tasks in, pop highest-priority first.
#[derive(Clone)]
pub struct TaskQueue {
    inner: Arc<Inner>,
}

struct Inner {
    heap: Mutex<BinaryHeap<PrioEntry>>,
    tasks: DashMap<TaskId, Task>,
    seen_goals: DashMap<String, TaskId>,
    counter: Mutex<u64>,
    notify: Notify,
}

impl TaskQueue {
    /// Create an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                heap: Mutex::new(BinaryHeap::new()),
                tasks: DashMap::new(),
                seen_goals: DashMap::new(),
                counter: Mutex::new(0),
                notify: Notify::new(),
            }),
        }
    }

    /// Enqueue `task`. Returns `Some(existing_id)` if an identical goal is already queued (dedup).
    pub async fn push(&self, task: Task) -> Option<TaskId> {
        let goal_key = task.goal.trim().to_lowercase();
        if let Some(existing) = self.inner.seen_goals.get(&goal_key) {
            return Some(*existing);
        }
        let mut c = self.inner.counter.lock().await;
        *c += 1;
        let entry = PrioEntry {
            priority: task.priority,
            seq: *c,
            id: task.id,
        };
        drop(c);
        self.inner.seen_goals.insert(goal_key, task.id);
        self.inner.tasks.insert(task.id, task);
        self.inner.heap.lock().await.push(entry);
        self.inner.notify.notify_one();
        None
    }

    /// Pop the highest-priority task, awaiting if the queue is empty.
    ///
    /// Before dispatching, scans the queue for tasks whose keyword fingerprint overlaps
    /// more than 50% with the dequeued task and merges their constraints in. The merged tasks are
    /// removed from the queue, reducing redundant agent runs for near-duplicate goals.
    pub async fn pop(&self) -> Task {
        loop {
            {
                let mut heap = self.inner.heap.lock().await;
                if let Some(entry) = heap.pop() {
                    if let Some((_, mut task)) = self.inner.tasks.remove(&entry.id) {
                        let goal_key = task.goal.trim().to_lowercase();
                        self.inner.seen_goals.remove(&goal_key);

                        // Keyword fingerprint of the dequeued task.
                        let primary_kws = keyword_set(&task.goal);

                        // Collect IDs of queued tasks with > 50% keyword overlap.
                        let mut to_merge: Vec<TaskId> = vec![];
                        for item in &self.inner.tasks {
                            let overlap = keyword_overlap(&primary_kws, &keyword_set(&item.goal));
                            if overlap > 0.5 {
                                to_merge.push(*item.key());
                            }
                        }

                        // Merge constraints from overlapping tasks, then remove them.
                        for id in to_merge {
                            if let Some((_, merged)) = self.inner.tasks.remove(&id) {
                                let mk = merged.goal.trim().to_lowercase();
                                self.inner.seen_goals.remove(&mk);
                                // Inject the merged task's constraints as additional context.
                                for c in merged.constraints {
                                    if !task.constraints.contains(&c) {
                                        task.constraints.push(c);
                                    }
                                }
                            }
                            // Remove from heap lazily — orphaned entries are skipped on next pop.
                        }

                        return task;
                    }
                }
            }
            self.inner.notify.notified().await;
        }
    }

    /// Number of tasks currently waiting in the queue.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.tasks.len()
    }

    /// True if the queue contains no waiting tasks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.tasks.is_empty()
    }

    /// Non-blocking snapshot of queued tasks, sorted by priority descending.
    ///
    /// Intended for display only — the returned order may not exactly match
    /// dispatch order because the heap's internal sequence numbers are not
    /// exposed.
    #[must_use]
    pub fn peek_queued(&self) -> Vec<(Priority, String)> {
        let mut items: Vec<(Priority, String)> = self
            .inner
            .tasks
            .iter()
            .map(|e| (e.value().priority, e.value().goal.clone()))
            .collect();
        items.sort_by_key(|&(prio, _)| Reverse(prio));
        items
    }
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

fn keyword_set(goal: &str) -> std::collections::HashSet<String> {
    goal.split_whitespace()
        .filter(|w| w.len() > 3)
        .map(str::to_lowercase)
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn keyword_overlap(
    a: &std::collections::HashSet<String>,
    b: &std::collections::HashSet<String>,
) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    intersection as f64 / a.union(b).count() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopi_core::Priority;

    fn make_task(goal: &str, priority: Priority) -> Task {
        let mut t = Task::new(goal);
        t.priority = priority;
        t
    }

    #[tokio::test]
    async fn fifo_same_priority() {
        let q = TaskQueue::new();
        q.push(make_task("first", Priority::Normal)).await;
        q.push(make_task("second", Priority::Normal)).await;
        assert_eq!(q.pop().await.goal, "first");
        assert_eq!(q.pop().await.goal, "second");
    }

    #[tokio::test]
    async fn higher_priority_wins() {
        let q = TaskQueue::new();
        q.push(make_task("low", Priority::Low)).await;
        q.push(make_task("critical", Priority::Critical)).await;
        q.push(make_task("normal", Priority::Normal)).await;
        assert_eq!(q.pop().await.goal, "critical");
        assert_eq!(q.pop().await.goal, "normal");
        assert_eq!(q.pop().await.goal, "low");
    }

    #[tokio::test]
    async fn goal_dedup_returns_existing_id() {
        let q = TaskQueue::new();
        let t = make_task("fix the bug", Priority::Normal);
        let id = t.id;
        assert!(q.push(t).await.is_none());
        // Same goal (case-insensitive, trimmed) should dedup.
        let dup = make_task("Fix the Bug", Priority::High);
        let existing = q.push(dup).await;
        assert_eq!(existing, Some(id));
        assert_eq!(q.len(), 1);
    }

    #[tokio::test]
    async fn len_and_is_empty() {
        let q = TaskQueue::new();
        assert!(q.is_empty());
        q.push(make_task("a", Priority::Normal)).await;
        assert_eq!(q.len(), 1);
        assert!(!q.is_empty());
        q.pop().await;
        assert!(q.is_empty());
    }

    #[tokio::test]
    async fn pop_after_push_clears_dedup() {
        let q = TaskQueue::new();
        q.push(make_task("goal x", Priority::Normal)).await;
        q.pop().await;
        // After pop the goal should be de-registered so we can push again.
        assert!(q
            .push(make_task("goal x", Priority::Normal))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn constraint_merging_combines_overlapping_goals() {
        let q = TaskQueue::new();
        // Jaccard of these two goals (words > 3 chars):
        //   t1 keywords: {refactor, authentication, middleware, logging}
        //   t2 keywords: {refactor, authentication, middleware, database}
        //   overlap = 3/5 = 0.6 > 0.5 → merge triggers
        let mut t1 = make_task(
            "refactor authentication middleware logging",
            Priority::Normal,
        );
        t1.constraints = vec!["keep async".into()];
        let mut t2 = make_task(
            "refactor authentication middleware database",
            Priority::Normal,
        );
        t2.constraints = vec!["preserve tests".into()];
        q.push(t1).await;
        q.push(t2).await;

        let merged = q.pop().await;
        assert!(
            merged.constraints.contains(&"keep async".to_string())
                || merged.constraints.contains(&"preserve tests".to_string()),
            "merged task should carry constraints from both"
        );
        assert!(
            merged.constraints.contains(&"keep async".to_string())
                && merged.constraints.contains(&"preserve tests".to_string()),
            "merged task should contain constraints from both tasks"
        );
    }

    #[tokio::test]
    async fn constraint_merging_leaves_disjoint_goals_alone() {
        let q = TaskQueue::new();
        // Completely disjoint keyword sets → no merge
        let mut t1 = make_task("upgrade database connection pooling", Priority::Normal);
        t1.constraints = vec!["constraint A".into()];
        let mut t2 = make_task("implement telemetry metrics dashboard", Priority::Normal);
        t2.constraints = vec!["constraint B".into()];
        q.push(t1).await;
        q.push(t2).await;

        let first = q.pop().await;
        // First task should have only its own constraint, not the other's
        assert!(first.constraints.contains(&"constraint A".to_string()));
        assert!(!first.constraints.contains(&"constraint B".to_string()));
        // Second task should still be in the queue
        assert_eq!(q.len(), 1);
    }

    #[tokio::test]
    async fn constraint_merge_deduplicates_shared_constraints() {
        let q = TaskQueue::new();
        let mut t1 = make_task(
            "refactor authentication middleware service",
            Priority::Normal,
        );
        t1.constraints = vec!["no new deps".into()];
        let mut t2 = make_task(
            "refactor authentication middleware handler",
            Priority::Normal,
        );
        t2.constraints = vec!["no new deps".into()]; // same constraint
        q.push(t1).await;
        q.push(t2).await;

        let merged = q.pop().await;
        // Duplicate constraint should appear only once
        let count = merged
            .constraints
            .iter()
            .filter(|c| c.as_str() == "no new deps")
            .count();
        assert_eq!(count, 1, "duplicate constraint should be deduplicated");
    }

    #[tokio::test]
    async fn merged_task_reduces_queue_len() {
        let q = TaskQueue::new();
        q.push(make_task(
            "refactor authentication middleware logging",
            Priority::Normal,
        ))
        .await;
        q.push(make_task(
            "refactor authentication middleware database",
            Priority::Normal,
        ))
        .await;
        assert_eq!(q.len(), 2);

        q.pop().await;
        // Both tasks consumed: primary + 1 merged sibling
        assert_eq!(q.len(), 0);
    }
}
