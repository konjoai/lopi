pub mod task;
pub mod agent;
pub mod config;
pub mod event;

pub use task::{Task, TaskId, TaskStatus, Priority, TaskSource};
pub use agent::{AgentRun, Attempt, AgentState, Score};
pub use config::LopiConfig;
pub use event::EventBus;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn task_id_is_unique() {
        let a = TaskId::new();
        let b = TaskId::new();
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn task_new_defaults() {
        let t = Task::new("fix the bug");
        assert_eq!(t.goal, "fix the bug");
        assert_eq!(t.priority, Priority::Normal);
        assert_eq!(t.max_retries, 3);
        assert!(t.allowed_dirs.contains(&"src/".to_string()));
        assert!(t.forbidden_dirs.contains(&".github/".to_string()));
        assert!(matches!(t.source, TaskSource::Cli));
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Critical > Priority::High);
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
    }

    #[test]
    fn score_passed_requires_full_pass_rate_and_no_lint() {
        let s = Score::new(1.0, 0, 10);
        assert!(s.passed());
        let s2 = Score::new(0.9, 0, 10);
        assert!(!s2.passed());
        let s3 = Score::new(1.0, 1, 10);
        assert!(!s3.passed());
    }

    #[test]
    fn score_weighted_clamps_to_zero() {
        let mut s = Score::new(0.0, 100, 100_000);
        s.errors = vec!["bad".into()];
        assert!(s.weighted() >= 0.0);
    }

    #[test]
    fn attempt_new_has_pending_outcome() {
        let tid = TaskId::new();
        let a = Attempt::new(tid, 1, "lopi/abc-attempt-1");
        assert_eq!(a.outcome, "pending");
        assert_eq!(a.attempt_num, 1);
        assert_eq!(a.branch, "lopi/abc-attempt-1");
    }

    #[test]
    fn agent_run_starts_idle() {
        let tid = TaskId::new();
        let run = AgentRun::new(tid);
        assert!(matches!(run.state, AgentState::Idle));
        assert!(run.attempts.is_empty());
        assert!(run.finished_at.is_none());
    }

    #[test]
    fn task_id_display() {
        let id = TaskId::new();
        let s = format!("{id}");
        assert_eq!(s.len(), 36);
    }

    #[test]
    fn task_source_serde_round_trip() {
        let s = TaskSource::Telegram { chat_id: 12345, message_id: 99 };
        let json = serde_json::to_string(&s).unwrap();
        let back: TaskSource = serde_json::from_str(&json).unwrap();
        match back {
            TaskSource::Telegram { chat_id, message_id } => {
                assert_eq!(chat_id, 12345);
                assert_eq!(message_id, 99);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn task_status_serde_round_trip() {
        let st = TaskStatus::Success {
            branch: "lopi/test-attempt-1".into(),
            pr_url: Some("https://github.com/org/repo/pull/1".into()),
        };
        let json = serde_json::to_string(&st).unwrap();
        let back: TaskStatus = serde_json::from_str(&json).unwrap();
        match back {
            TaskStatus::Success { branch, pr_url } => {
                assert_eq!(branch, "lopi/test-attempt-1");
                assert_eq!(pr_url.unwrap(), "https://github.com/org/repo/pull/1");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn created_at_is_recent() {
        let before = Utc::now();
        let t = Task::new("goal");
        let after = Utc::now();
        assert!(t.created_at >= before);
        assert!(t.created_at <= after);
    }

    #[test]
    fn event_bus_broadcast_and_receive() {
        let bus: EventBus<String> = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.send("hello".to_string());
        // recv() is async; test synchronously via try_recv.
        assert_eq!(rx.try_recv().unwrap(), "hello");
    }
}
