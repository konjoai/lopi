//! Convert quality violations into lopi `Task` records for the orchestrator queue.
use crate::QualityViolation;
use lopi_core::{Task, TaskSource};

/// Convert a slice of violations into low-priority fix tasks.
///
/// Each violation becomes a `Task` whose goal is the violation's `fix_hint`.
/// `Severity::Error` violations get `Priority::High`; warnings get `Priority::Normal`.
/// The task `source` is `TaskSource::Maintenance` so it can be identified and
/// distinguished from user-initiated or CI-injected tasks.
pub fn violations_to_tasks(violations: &[QualityViolation]) -> Vec<Task> {
    violations
        .iter()
        .map(|v| {
            let mut t = Task::new(v.fix_hint.clone());
            t.priority = v.task_priority();
            t.source = TaskSource::Maintenance;
            t
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{Severity, ViolationKind};
    use lopi_core::Priority;

    fn make_violation(kind: ViolationKind, severity: Severity, hint: &str) -> QualityViolation {
        QualityViolation {
            file: "src/lib.rs".into(),
            line: Some(10),
            kind,
            severity,
            message: "test message".into(),
            fix_hint: hint.into(),
            confidence: 1.0,
        }
    }

    #[test]
    fn empty_violations_produces_empty_tasks() {
        assert!(violations_to_tasks(&[]).is_empty());
    }

    #[test]
    fn error_violation_produces_high_priority_task() {
        let v = make_violation(ViolationKind::Standards, Severity::Error, "Fix this error");
        let tasks = violations_to_tasks(&[v]);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].goal, "Fix this error");
        assert_eq!(tasks[0].priority, Priority::High);
        assert!(matches!(tasks[0].source, TaskSource::Maintenance));
    }

    #[test]
    fn warning_violation_produces_normal_priority_task() {
        let v = make_violation(
            ViolationKind::Coverage,
            Severity::Warning,
            "Add tests for src/lib.rs",
        );
        let tasks = violations_to_tasks(&[v]);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].priority, Priority::Normal);
    }

    #[test]
    fn multiple_violations_produce_one_task_each() {
        let violations = vec![
            make_violation(
                ViolationKind::Complexity,
                Severity::Warning,
                "Simplify function",
            ),
            make_violation(
                ViolationKind::DeadCode,
                Severity::Warning,
                "Remove dead code",
            ),
            make_violation(ViolationKind::Standards, Severity::Error, "Fix lint error"),
        ];
        let tasks = violations_to_tasks(&violations);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].goal, "Simplify function");
        assert_eq!(tasks[2].priority, Priority::High);
    }
}
