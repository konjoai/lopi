//! `lopi-memory`: `SQLite`-backed persistent store for tasks, patterns, turn metrics, and learned lessons.
pub mod store;
pub use store::{LessonRow, MemoryStore, PatternRow, TaskRow};
