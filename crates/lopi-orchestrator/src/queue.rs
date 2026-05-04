use dashmap::DashMap;
use lopi_core::{Task, TaskId, Priority};
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
        let entry = PrioEntry { priority: task.priority, seq: *c, id: task.id };
        drop(c);
        self.inner.seen_goals.insert(goal_key, task.id);
        self.inner.tasks.insert(task.id, task);
        self.inner.heap.lock().await.push(entry);
        self.inner.notify.notify_one();
        None
    }

    /// Pop the highest-priority task, awaiting if the queue is empty.
    pub async fn pop(&self) -> Task {
        loop {
            {
                let mut heap = self.inner.heap.lock().await;
                if let Some(entry) = heap.pop() {
                    if let Some((_, t)) = self.inner.tasks.remove(&entry.id) {
                        let goal_key = t.goal.trim().to_lowercase();
                        self.inner.seen_goals.remove(&goal_key);
                        return t;
                    }
                }
            }
            self.inner.notify.notified().await;
        }
    }

    pub fn len(&self) -> usize {
        self.inner.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.tasks.is_empty()
    }
}

impl Default for TaskQueue {
    fn default() -> Self { Self::new() }
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
        assert!(q.push(make_task("goal x", Priority::Normal)).await.is_none());
    }
}
