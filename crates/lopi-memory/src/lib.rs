//! `lopi-memory`: `SQLite`-backed persistent store for tasks, patterns, turn metrics, and learned lessons.
pub mod store;
pub use store::{
    InstallationRow, LessonRow, MemoryStore, PatternRow, QualityRunRecord, QualityRunRow,
    StabilityEntry, StabilityRecord, TaskRow,
};
