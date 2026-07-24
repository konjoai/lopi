//! `lopi-memory`: `SQLite`-backed persistent store for tasks, patterns, turn metrics, and learned lessons.

/// SQLite store implementations for all lopi memory tables.
pub mod store;
pub use store::{
    current_stage, dag_graph_json, AuditInput, AuditQuery, AuditRow, BackfillOutcome,
    ChainRunRow, ChainStepInput, ChainStepRow, CheckpointInput, CheckpointRow, DagNodeRow,
    EvalOutcomeRow, InstallationRow, LearningRow, LessonRow, LoopAttemptRow, LoopRunRow,
    LoopTurnRow, MaxxInput, MaxxRow, MaxxRunRow, MemoryStore, OnboardingPattern, PatternRow,
    QualityRunRecord, QualityRunRow, QuotaObservationRow, RunAttemptRow, RunTurnAgg,
    ScheduleChainInput, ScheduleChainRow, ScheduleInput, ScheduleRow, ScheduleRunRow, ScorePoint,
    StabilityEntry, StabilityRecord, TaskLogRow, TaskRow, TaskStatusCounts, VerifierVerdictRow,
    TASK_LOG_MAX_PER_TASK,
};
