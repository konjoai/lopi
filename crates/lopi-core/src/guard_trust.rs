//! Sprint S10, Phase 0 — repo-supplied guard-command trust resolution. Split
//! out of `loop_config.rs` purely to keep that file under the 500-line CI
//! file-size gate (same pattern as `autonomy.rs`); `run_guard_command` and
//! `resolve_guard_command` are re-exported from `loop_config` unchanged, so
//! every existing `lopi_core::loop_config::{run_guard_command,
//! resolve_guard_command}` import path stays valid.

use crate::loop_config::LoopConfig;
use std::path::Path;

/// Run a shell command in `cwd` and report whether it exited `0`.
///
/// Shared by the `gate`/`until` guardrails, eval tier 1 (`ShellTestEval`/
/// `SuiteEval`), and the scorer's configured `test_command` — every place a
/// free-form shell string from loop-engineering config is executed. Invoked
/// via `sh -c` (unlike the codebase's other shell-outs, which always run a
/// fixed known binary with explicit args) since these are free-form command
/// strings, not an argv array. Only the exit status is inspected —
/// stdout/stderr are discarded, since the pass/fail decision this guards
/// needs nothing else.
///
/// SECURITY (Sprint S10, Phase 0 — rewritten; the previous comment here was
/// wrong and the wrongness was the finding): `cmd` is **not** trustworthy
/// merely because it is "the repo's own config." `.lopi/loop.toml` on a
/// branch under evaluation is content an attacker can add via a pull
/// request — `lopi serve-webhooks` will dispatch a task against that branch,
/// and this function has no way to tell an operator's own config from a
/// hostile one once it's holding a `cmd: &str`. The trust decision is made
/// **before** this function is ever called: [`resolve_guard_command`] (or
/// the equivalent per-source check in `eval::tiers`) must have already
/// established that `cmd` came from operator-controlled config — a repo's
/// `.lopi/loop.toml` when the task's source is trusted
/// ([`crate::is_untrusted_source`] is `false`), or the operator's own
/// `~/.lopi/loop.toml` ([`LoopConfig::load_operator_overrides`]) regardless
/// of task source — before it reaches here. This function itself performs
/// no trust check and executes whatever it is given; treat every new call
/// site as security-sensitive and route it through the same resolution.
///
/// # Errors
/// Returns `Err` only if the shell itself could not be spawned (e.g. `sh`
/// missing from `PATH`). A command that runs and exits non-zero is a normal
/// `Ok(false)`, not an error.
pub async fn run_guard_command(cmd: &str, cwd: &Path) -> std::io::Result<bool> {
    let status = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(cwd)
        .status()
        .await?;
    Ok(status.success())
}

/// Decide the effective value of one guard command (`gate`/`until`/
/// `test_command`), given the repo-supplied value, the operator's global
/// override for the same field, and whether the task's source is trusted.
///
/// Sprint S10, Phase 0 remediation — pure and I/O-free so the trust decision
/// itself is directly unit-testable without a filesystem or a real task:
///
/// - The operator override always wins when set — it cannot have arrived
///   via a branch under evaluation (see [`LoopConfig::load_operator_overrides`]).
/// - Otherwise, the repo-supplied value is honored only when `source_trusted`
///   — i.e. [`is_untrusted_source`](crate::is_untrusted_source) returned
///   `false` for the task's [`crate::TaskSource`].
/// - Otherwise (repo-supplied, untrusted source, no operator override):
///   `None`. The command is dropped, not queued for later approval — a
///   `gate`/`until` is a precondition/exit-check, and treating it as unset
///   reproduces the safe "no guardrail configured" behavior rather than
///   inventing a new blocked state. `test_command` and eval-tier-1 checks
///   fail closed the same way at their own call sites.
///
/// Callers that skip this resolution and pass a repo-supplied value straight
/// to [`run_guard_command`] reintroduce the Phase 0 finding — every caller
/// of `run_guard_command` must route through here (or the equivalent
/// per-source check in `lopi-agent`'s `eval::tiers`, for the one guard
/// vector — `Task::acceptance` — that isn't `LoopConfig`-sourced).
#[must_use]
pub fn resolve_guard_command(
    repo_value: Option<&str>,
    operator_value: Option<&str>,
    source_trusted: bool,
) -> Option<String> {
    if let Some(v) = operator_value {
        return Some(v.to_string());
    }
    match repo_value {
        Some(v) if source_trusted => Some(v.to_string()),
        Some(v) => {
            tracing::warn!(
                target: "lopi_core::security",
                cmd = v,
                "refusing repo-supplied guard command: task source is untrusted \
                 and no operator override is configured — see docs/security/TRIFECTA_PATHS.md"
            );
            None
        }
        None => None,
    }
}

impl LoopConfig {
    /// Load the operator's own global loop config from `~/.lopi/loop.toml`,
    /// if present and parseable.
    ///
    /// Sprint S10, Phase 0 — the one config path a branch under evaluation
    /// can never influence: it lives outside any repo checkout, in the
    /// operator's own home directory. `gate`/`until`/`test_command` set here
    /// are always honored regardless of a task's [`crate::TaskSource`] (see
    /// [`resolve_guard_command`]), the same way an operator's own CLI
    /// invocation is trusted. Absent, unreadable, or malformed files all
    /// yield `None` — a missing or broken operator override is silently
    /// equivalent to "no override configured," never an error, since (unlike
    /// [`LoopConfig::load_from_repo`]) there is no repo context in which to
    /// surface a parse failure loudly.
    ///
    /// `--config`-flag and operator-pinned-commit overrides described in the
    /// Sprint S10 brief are not implemented by this function — this is the
    /// one operator-controlled source it covers. See `docs/security/TRIFECTA_PATHS.md`.
    #[must_use]
    pub fn load_operator_overrides() -> Option<Self> {
        let home = std::env::var("HOME").ok()?;
        let p = Path::new(&home).join(".lopi").join("loop.toml");
        let text = std::fs::read_to_string(&p).ok()?;
        match toml::from_str(&text) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                tracing::warn!(
                    path = %p.display(),
                    "operator loop config exists but failed to parse ({e}); ignoring"
                );
                None
            }
        }
    }
}

#[cfg(test)]
#[path = "guard_trust_tests.rs"]
mod tests;
