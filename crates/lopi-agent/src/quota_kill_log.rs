//! Prep-only instrumentation for MAXX kill tests 1–2 (see
//! `docs/ops/NEXT_SESSION_PROMPT.md`). Neither built to run against a live
//! session nor wired on by default — this module exists so that when someone
//! *does* run it on real hardware, the answer is "run this and read the
//! file," not "instrument it from scratch while sitting at the keyboard."
//!
//! Kill test 1 asks two things: does `rate_limit_event` fire every turn, or
//! only once utilization crosses a threshold; and is `resetsAt` reliably
//! present. Both are answerable from the raw `rate_limit_info` object, which
//! `claude_events::parse_rate_limit` already decodes — including
//! `surpassedThreshold`/`isUsingOverage`, present in the real capture
//! (`artifacts/STREAM_CAPTURE.jsonl`) but not previously threaded anywhere.
//! `QuotaKillLogScanner` logs the raw payload verbatim, plus how many
//! decoded stream events and assistant-text turns passed since the previous
//! observation — the cadence signal a human needs to tell "every turn" from
//! "threshold-gated" apart at a glance.
//!
//! This sits on decoded `StreamEvent`s, not raw NDJSON lines — deliberately,
//! so it can hook into `forward_stream_event`
//! (`crates/lopi-agent/src/runner/stream.rs`), where every event already
//! passes through as a `&StreamEvent`, rather than `claude.rs`'s own read
//! loop. One raw NDJSON line can decode to zero, one, or several
//! `StreamEvent`s (see `parse_line`'s doc comment), so the counters here are
//! "decoded events observed," not "raw lines read" — named as such below
//! rather than blurring the two.

use crate::claude_events::StreamEvent;
use serde::Serialize;

/// One logged observation of a `rate_limit_event`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QuotaKillLogRecord {
    /// Wall-clock unix seconds when this observation was made. Supplied by
    /// the caller (see [`QuotaKillLogScanner::observe`]) rather than read
    /// internally, so this module stays a pure, unit-testable scanner.
    pub observed_at_unix: i64,
    /// Decoded [`StreamEvent`]s observed since the previous observation (or
    /// since the scanner was created, for the first one). The core cadence
    /// signal: a small, steady count means "fires every turn"; a large,
    /// irregular count that only shrinks once utilization is high means
    /// threshold-gated.
    pub events_since_last: u64,
    /// [`StreamEvent::Text`] blocks seen since the previous observation — a
    /// coarser proxy for "conversational turns," since one turn can emit
    /// several other `StreamEvent`s (thinking, tool use) around a single text
    /// block. A proxy, not an exact turn count — kill test 1's answer should
    /// be read primarily off `events_since_last`, with this as corroboration.
    pub text_turns_since_last: u64,
    /// Status string, e.g. `allowed_warning`.
    pub status: String,
    /// Window type, e.g. `seven_day` / `five_hour`.
    pub limit_type: String,
    /// Window utilization in `[0.0, 1.0]`.
    pub utilization: f32,
    /// Unix seconds the window resets, if the CLI reported one. Kill test
    /// 1's second question — is this reliably present for both window
    /// types — is answered by scanning this column across a real log.
    pub resets_at: Option<i64>,
    /// The CLI's own `surpassedThreshold`, if reported. When this is
    /// consistently absent until a specific utilization and then always
    /// present, that alone answers "is it threshold-gated."
    pub surpassed_threshold: Option<f32>,
    /// The CLI's own `isUsingOverage`, if reported.
    pub is_using_overage: Option<bool>,
}

/// Stateful scanner. Feed it every decoded [`StreamEvent`] from a
/// `claude -p --output-format stream-json` session, in order; it returns
/// `Some(record)` for every [`StreamEvent::RateLimit`], `None` otherwise.
#[derive(Debug, Default)]
pub struct QuotaKillLogScanner {
    events_seen: u64,
    text_turns_seen: u64,
    events_at_last_observation: u64,
    text_turns_at_last_observation: u64,
}

impl QuotaKillLogScanner {
    /// A fresh scanner with no prior observations.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one decoded stream event. `now_unix` is the caller's wall clock
    /// at observation time — injected rather than read internally so this
    /// stays a pure, unit-testable scanner (see `quota_kill_log_tests.rs`).
    pub fn observe(&mut self, ev: &StreamEvent, now_unix: i64) -> Option<QuotaKillLogRecord> {
        self.events_seen += 1;
        if matches!(ev, StreamEvent::Text(_)) {
            self.text_turns_seen += 1;
        }
        let StreamEvent::RateLimit {
            status,
            limit_type,
            utilization,
            resets_at,
            surpassed_threshold,
            is_using_overage,
        } = ev
        else {
            return None;
        };
        let record = QuotaKillLogRecord {
            observed_at_unix: now_unix,
            events_since_last: self.events_seen - self.events_at_last_observation,
            text_turns_since_last: self.text_turns_seen - self.text_turns_at_last_observation,
            status: status.clone(),
            limit_type: limit_type.clone(),
            utilization: *utilization,
            resets_at: *resets_at,
            surpassed_threshold: *surpassed_threshold,
            is_using_overage: *is_using_overage,
        };
        self.events_at_last_observation = self.events_seen;
        self.text_turns_at_last_observation = self.text_turns_seen;
        Some(record)
    }
}

/// Serialize a record as one NDJSON output line (no trailing newline — the
/// caller appends it, matching every other line-writer in this crate).
///
/// # Errors
///
/// Returns an error only if `record` somehow fails to serialize, which does
/// not happen for this plain-data struct in practice — kept fallible rather
/// than force-unwrapped, per the workspace's no-panicking-helpers rule.
pub fn to_ndjson_line(record: &QuotaKillLogRecord) -> serde_json::Result<String> {
    serde_json::to_string(record)
}

/// The env var that turns this on. Unset (the default) means every call in
/// this module is a no-op — zero behavior change for anyone not opted in.
pub const ENV_VAR: &str = "LOPI_QUOTA_KILL_TEST_LOG";

/// Async writer half — owns an unbounded channel so the synchronous
/// `on_event` callback (`Fn(&StreamEvent) + Send`, per `claude.rs`'s
/// `plan_streamed`) never touches a file descriptor itself; a background
/// task does the actual `tokio::fs` append. No blocking I/O reaches the
/// async path this hooks into, per the workspace's I/O rule.
struct QuotaKillLogSink {
    tx: tokio::sync::mpsc::UnboundedSender<String>,
}

impl QuotaKillLogSink {
    /// Build a sink from [`ENV_VAR`], if set; spawns the background writer.
    /// Returns `None` — a pure no-op — when the var is absent, the default.
    fn from_env() -> Option<Self> {
        let path = std::env::var(ENV_VAR).ok()?;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await;
            let mut file = match file {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(error = %e, %path, "quota_kill_log: failed to open log file, disabling for this run");
                    return;
                }
            };
            while let Some(line) = rx.recv().await {
                if let Err(e) = file.write_all(format!("{line}\n").as_bytes()).await {
                    tracing::warn!(error = %e, "quota_kill_log: write failed, dropping this observation");
                }
            }
        });
        Some(Self { tx })
    }

    /// Hand off one already-serialized line to the writer task. Never
    /// blocks; a dead writer task (channel closed) just drops the line —
    /// this is a best-effort diagnostic sidecar, never allowed to affect the
    /// agent run it's observing.
    fn send(&self, line: String) {
        if self.tx.send(line).is_err() {
            tracing::warn!("quota_kill_log: writer task gone, dropping observation");
        }
    }
}

struct QuotaKillLogState {
    sink: QuotaKillLogSink,
    scanner: std::sync::Mutex<QuotaKillLogScanner>,
}

static STATE: std::sync::OnceLock<Option<QuotaKillLogState>> = std::sync::OnceLock::new();

/// Feed one decoded stream event to the process-wide kill-test logger, if
/// [`ENV_VAR`] is set. A no-op (one `OnceLock` read) when it isn't — the
/// default, off state. Intended hookup point: `forward_stream_event`
/// (`crates/lopi-agent/src/runner/stream.rs`), so every event either
/// `stream_plan` or `stream_implement` sees also reaches this.
///
/// Scoped process-wide (a static, not a per-task field) rather than threaded
/// through `AgentRunner`: a single `lopi run` CLI invocation is one process,
/// so this covers "the whole session" for the kill-test protocol's intended
/// single-task usage (`docs/ops/NEXT_SESSION_PROMPT.md`'s pre-flight calls
/// for exactly that). Running this against `lopi sail` with multiple
/// concurrent tasks would interleave every task's events into one cadence
/// count — a real caveat, flagged here rather than silently wrong, but not
/// one the kill-test protocol needs since it calls for a single `lopi run`.
pub fn observe_global(ev: &StreamEvent, now_unix: i64) {
    let Some(state) = STATE.get_or_init(|| {
        QuotaKillLogSink::from_env().map(|sink| QuotaKillLogState {
            sink,
            scanner: std::sync::Mutex::new(QuotaKillLogScanner::new()),
        })
    }) else {
        return;
    };
    let Ok(mut scanner) = state.scanner.lock() else {
        return;
    };
    let Some(record) = scanner.observe(ev, now_unix) else {
        return;
    };
    match to_ndjson_line(&record) {
        Ok(line) => state.sink.send(line),
        Err(e) => tracing::warn!(error = %e, "quota_kill_log: failed to serialize record"),
    }
}

#[cfg(test)]
#[path = "quota_kill_log_tests.rs"]
mod tests;
