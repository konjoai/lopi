use anyhow::Context;
use lopi_core::Task;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncBufReadExt;

/// Stream plan steps as they are generated. Returns a channel receiver that emits
/// numbered plan steps (lines matching `^\d+\.`) and a join handle that resolves to
/// the full plan text when the claude process exits.
pub fn plan_streaming(
    repo_path: &Path,
    cli_path: &str,
    timeout: Duration,
    task: &Task,
    all_constraints: Vec<String>,
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
            .current_dir(&repo_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
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
