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
        // Higher priority first; older sequence first as tiebreaker.
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

    /// Enqueue a task. Returns Some(existing_id) if a task with the same goal is already queued.
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

    /// Pop the next task, awaiting if the queue is empty.
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
