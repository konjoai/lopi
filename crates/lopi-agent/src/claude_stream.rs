use anyhow::Context;
use lopi_core::Task;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;

/// Stream plan steps as they are generated. Returns a channel receiver that emits
/// numbered plan steps (lines matching `^\d+\.`) and a join handle that resolves to
/// the full plan text when the claude process exits.
///
/// `model`/`max_budget_usd`/`max_turns`/`allowed_tools`/`disallowed_tools` mirror
/// the caps [`ClaudeCode::run`](crate::claude::ClaudeCode) and
/// [`ClaudeCode::run_streamed`] already apply — this is the third (speculative)
/// `claude -p` spawn site, and until these were threaded through it was the one
/// path a `--speculative` run could still spend on with no cap at all, even when
/// `.lopi/loop.toml` configured one.
#[allow(clippy::too_many_arguments)]
pub fn plan_streaming(
    repo_path: &Path,
    cli_path: &str,
    timeout: Duration,
    task: &Task,
    all_constraints: Vec<String>,
    model: Option<&str>,
    max_budget_usd: Option<f64>,
    max_turns: Option<u32>,
    allowed_tools: &[String],
    disallowed_tools: &[String],
) -> (
    tokio::task::JoinHandle<anyhow::Result<String>>,
    tokio::sync::mpsc::Receiver<String>,
) {
    let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

    let allowed: Vec<String> = task.allowed_dirs.clone();
    let forbidden: Vec<String> = task.forbidden_dirs.clone();
    let goal = task.goal.clone();
    let cli_path = cli_path.to_string();
    let repo_path = repo_path.to_path_buf();
    let model = model.map(str::to_string);
    let allowed_tools = allowed_tools.to_vec();
    let disallowed_tools = disallowed_tools.to_vec();

    let handle = tokio::spawn(async move {
        let ctx = lopi_toon::encode_task_context(
            &goal,
            &allowed.iter().map(String::as_str).collect::<Vec<_>>(),
            &forbidden.iter().map(String::as_str).collect::<Vec<_>>(),
            &all_constraints
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            &[],
            &[],
        );
        let prompt = format!(
            "You are running inside lopi. \
             Produce a concise implementation plan. \
             Output a numbered list of steps only.\n\n\
             ## Task context (TOON)\n{ctx}"
        );

        let mut cmd = tokio::process::Command::new(&cli_path);
        cmd.arg("-p")
            .arg(&prompt)
            // Same unattended-session guard as the streaming plan/implement
            // path (`ClaudeCode::run_streamed`) — without it a tool call
            // needing approval stalls this headless pipeline forever.
            .arg("--dangerously-skip-permissions")
            .current_dir(&repo_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        crate::claude_support::apply_cli_caps(
            &mut cmd,
            model.as_deref(),
            max_turns,
            max_budget_usd,
            &allowed_tools,
            &disallowed_tools,
        );
        // Same auth guard as the one-shot path: never let inherited routing
        // env (ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL, etc.) silently switch
        // the CLI from the user's subscription to API-key billing.
        crate::claude::scrub_inherited_anthropic_env(&mut cmd);
        let mut child = cmd.spawn().context("spawning claude for streaming plan")?;

        let stdout = child.stdout.take().context("claude stdout unavailable")?;
        let mut reader = tokio::io::BufReader::new(stdout).lines();
        let mut full_text = String::new();

        let deadline = tokio::time::sleep(timeout);
        tokio::pin!(deadline);

        loop {
            tokio::select! {
                line = reader.next_line() => {
                    match line? {
                        Some(l) => {
                            // Emit numbered plan steps immediately so the implement
                            // worker can begin applying them speculatively.
                            if l.trim_start().chars().next().is_some_and(|c| c.is_ascii_digit()) {
                                let _ = tx.send(l.clone()).await;
                            }
                            full_text.push_str(&l);
                            full_text.push('\n');
                        }
                        None => break,
                    }
                }
                () = &mut deadline => {
                    child.kill().await.ok();
                    anyhow::bail!("claude plan stream timed out");
                }
            }
        }
        child.wait().await.ok();
        Ok(full_text)
    });

    (handle, rx)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Write an executable shell stub standing in for the `claude` CLI: it
    /// dumps its full argv to `capture_path` (one arg per line, so a
    /// multi-line TOON prompt can't swallow a flag into ambiguity) and emits
    /// one numbered line so the plan-step reader loop has something to
    /// stream before the process exits.
    fn write_argv_capture_stub(script_path: &Path, capture_path: &Path) {
        let script = format!(
            "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> {capture}; done\necho '1. stub plan step'\n",
            capture = capture_path.display(),
        );
        std::fs::write(script_path, script).unwrap();
        let mut perms = std::fs::metadata(script_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(script_path, perms).unwrap();
    }

    fn unique_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("lopi_plan_streaming_{tag}_{}", std::process::id()))
    }

    /// Part 0 — the speculative `claude -p` spawn site: until `model`/
    /// `max_budget_usd`/`max_turns`/`allowed_tools`/`disallowed_tools` were
    /// threaded through, this function fired with none of the caps
    /// `.lopi/loop.toml` configured, regardless of `--speculative` even
    /// being wired end-to-end everywhere else. Asserts the built subprocess
    /// argv genuinely carries every cap, not just that the fields exist.
    #[tokio::test]
    async fn plan_streaming_forwards_all_caps_to_the_subprocess_argv() {
        let script = unique_path("caps_script");
        let capture = unique_path("caps_capture");
        std::fs::remove_file(&capture).ok();
        write_argv_capture_stub(&script, &capture);

        let task = Task::new("plan_streaming cap forwarding test");
        let (handle, mut rx) = plan_streaming(
            Path::new("."),
            script.to_str().unwrap(),
            Duration::from_secs(10),
            &task,
            vec![],
            Some("claude-opus-4-7"),
            Some(2.5),
            Some(7),
            &["Bash".to_string()],
            &["Workflow".to_string()],
        );
        while rx.recv().await.is_some() {}
        handle.await.unwrap().unwrap();

        let argv = std::fs::read_to_string(&capture).unwrap();
        assert!(
            argv.contains("--dangerously-skip-permissions"),
            "argv={argv}"
        );
        assert!(argv.contains("--model\nclaude-opus-4-7"), "argv={argv}");
        assert!(argv.contains("--max-budget-usd\n2.5"), "argv={argv}");
        assert!(argv.contains("--max-turns\n7"), "argv={argv}");
        assert!(argv.contains("--allowedTools\nBash"), "argv={argv}");
        assert!(argv.contains("--disallowedTools\nWorkflow"), "argv={argv}");

        std::fs::remove_file(&script).ok();
        std::fs::remove_file(&capture).ok();
    }

    /// Absent caps (the pre-Part-0 default) must still add nothing — `None`/
    /// empty stays a true no-op, matching every other `claude -p` spawn site's
    /// "0/None = disabled" convention.
    #[tokio::test]
    async fn plan_streaming_omits_flags_for_absent_caps() {
        let script = unique_path("nocaps_script");
        let capture = unique_path("nocaps_capture");
        std::fs::remove_file(&capture).ok();
        write_argv_capture_stub(&script, &capture);

        let task = Task::new("plan_streaming no-cap test");
        let (handle, mut rx) = plan_streaming(
            Path::new("."),
            script.to_str().unwrap(),
            Duration::from_secs(10),
            &task,
            vec![],
            None,
            None,
            None,
            &[],
            &[],
        );
        while rx.recv().await.is_some() {}
        handle.await.unwrap().unwrap();

        let argv = std::fs::read_to_string(&capture).unwrap();
        assert!(argv.contains("--dangerously-skip-permissions"));
        assert!(!argv.contains("--model"));
        assert!(!argv.contains("--max-budget-usd"));
        assert!(!argv.contains("--max-turns"));
        assert!(!argv.contains("--allowedTools"));
        assert!(!argv.contains("--disallowedTools"));

        std::fs::remove_file(&script).ok();
        std::fs::remove_file(&capture).ok();
    }
}
