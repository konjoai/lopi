//! `lopi-memory`: `SQLite`-backed persistent store for tasks, patterns, turn metrics, and learned lessons.

/// SQLite store implementations for all lopi memory tables.
pub mod store;
pub use store::{
    compute_cache_key, AuditInput, AuditQuery, AuditRow, CacheStats, CachedResult, CheckpointInput,
    CheckpointRow, DagNodeRow, DeadLetterInput, DeadLetterRow, InstallationRow, LessonRow,
    MemoryStore, PatternRow, QualityRunRecord, QualityRunRow, ScheduleInput, ScheduleRow,
    ScheduleRunRow, StabilityEntry, StabilityRecord, TaskLogRow, TaskRow, TrustLedgerRow,
    VerifierVerdictRow, TASK_LOG_MAX_PER_TASK,
};
