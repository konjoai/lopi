//! Shared planning-prompt construction.
//!
//! Both the direct-API planning path (`runner::api_plan`) and the Layer 5
//! stability harness (`stability`) need the same task-context prompt — the
//! harness measures plan variance against the real prompt the agent would
//! use, so the two must never drift. Neither module is a descendant of the
//! other, so this lives at the crate root as their shared dependency rather
//! than one reaching into the other's private internals.

/// Render the task-context prompt shared by the direct-API planning path
/// and the stability harness. Keeps it small and deterministic so prompt
/// caching hits on the API path.
pub(crate) fn build_user_prompt(
    task: &lopi_core::Task,
    last_error: Option<&str>,
    lessons: &[String],
) -> String {
    let mut parts = Vec::with_capacity(6);
    parts.push(format!("# Task\n{}", task.goal));

    if !task.constraints.is_empty() {
        parts.push(format!(
            "\n# Constraints\n- {}",
            task.constraints.join("\n- ")
        ));
    }
    if !task.allowed_dirs.is_empty() {
        parts.push(format!(
            "\n# Allowed dirs\n- {}",
            task.allowed_dirs.join("\n- ")
        ));
    }
    if !task.forbidden_dirs.is_empty() {
        parts.push(format!(
            "\n# Forbidden dirs\n- {}",
            task.forbidden_dirs.join("\n- ")
        ));
    }
    if !lessons.is_empty() {
        parts.push(format!(
            "\n# Lessons from past patterns\n- {}",
            lessons.join("\n- ")
        ));
    }
    if let Some(err) = last_error {
        parts.push(format!(
            "\n# Previous attempt failed\nAnalyze this error and adjust your approach:\n{}",
            err
        ));
    }
    parts.push(
        "\nProduce a concise step-by-step plan to complete this task. \
         Each step should be a single edit or shell command."
            .to_string(),
    );

    parts.join("\n")
}
