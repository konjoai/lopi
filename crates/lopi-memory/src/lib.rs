//! `lopi-memory`: `SQLite`-backed persistent store for tasks, patterns, turn metrics, and learned lessons.
pub mod store;
pub use store::{
    compute_cache_key, AuditInput, AuditQuery, AuditRow, CacheStats, CachedResult, CheckpointInput,
    CheckpointRow, DeadLetterInput, DeadLetterRow, InstallationRow, LessonRow, MemoryStore,
    PatternRow, QualityRunRecord, QualityRunRow, StabilityEntry, StabilityRecord, TaskRow,
};
