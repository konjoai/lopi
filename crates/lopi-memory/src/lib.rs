//! `lopi-memory`: `SQLite`-backed persistent store for tasks, patterns, turn metrics, and learned lessons.

/// SQLite store implementations for all lopi memory tables.
pub mod store;
pub use store::{
    compute_cache_key, AuditInput, AuditQuery, AuditRow, CacheStats, CachedResult, CheckpointInput,
    CheckpointRow, DagNodeRow, EvalOutcomeRow, InstallationRow, LearningRow, LessonRow,
    LoopAttemptRow, LoopRunRow, LoopTurnRow, MemoryStore, PatternRow, QualityRunRecord,
    QualityRunRow, QuotaObservationRow, RunAttemptRow, RunTurnAgg, ScheduleInput, ScheduleRow,
    ScheduleRunRow, ScorePoint, StabilityEntry, StabilityRecord, TaskLogRow, TaskRow,
    TaskStatusCounts, VerifierVerdictRow, TASK_LOG_MAX_PER_TASK,
};
