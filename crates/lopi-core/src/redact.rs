//! Pattern-based secret redaction for agent-produced log text.
//!
//! Sprint S12, Phase 1: agent stdout (a `cat` of a `.env`, a build script
//! echoing a variable, a test failure printing a connection string) reaches
//! `AgentEvent::LogLine` and, from there, both the `task_logs` SQLite table
//! and the live SSE/WS stream, with nothing on that path ever redacting it.
//! [`redact_secrets`] is the single point both sinks are meant to call
//! through — call it once at the boundary where a `LogLine` is produced,
//! not separately in the persister and the serializer, or the two will
//! drift out of sync.
//!
//! This is a mitigation, not a guarantee. Pattern matching catches known
//! secret shapes (see `redact_patterns.txt`) and will miss anything novel —
//! a bespoke internal token format, a secret split across two log lines, an
//! unusual encoding. It does not make the SSE stream, or `task_logs`, safe
//! to expose to an untrusted subscriber; that is what auth on the stream
//! itself is for (see `docs/security/TRIFECTA_PATHS.md`).

use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

const PATTERNS_SRC: &str = include_str!("../redact_patterns.txt");

struct Pattern<'a> {
    label: &'a str,
    re: Regex,
}

/// Parse `label<TAB>regex` lines (blank lines and `#`-comments skipped) into
/// compiled patterns. A malformed regex is skipped — logged, not fatal —
/// rather than taking every other pattern down with it.
fn parse_patterns(src: &str) -> Vec<Pattern<'_>> {
    src.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let (label, pattern) = l.split_once('\t')?;
            match Regex::new(pattern) {
                Ok(re) => Some(Pattern { label, re }),
                Err(e) => {
                    tracing::warn!(label, "invalid redact pattern, skipping: {e}");
                    None
                }
            }
        })
        .collect()
}

static PATTERNS: LazyLock<Vec<Pattern<'static>>> = LazyLock::new(|| parse_patterns(PATTERNS_SRC));

/// Replace every known secret shape in `line` with `[REDACTED:<label>]`.
///
/// Returns a borrowed [`Cow`] when nothing matched, so the common case (a
/// log line with no secret in it) allocates nothing.
#[must_use]
pub fn redact_secrets(line: &str) -> Cow<'_, str> {
    let mut current = Cow::Borrowed(line);
    for pattern in PATTERNS.iter() {
        if pattern.re.is_match(&current) {
            let replacement = format!("[REDACTED:{}]", pattern.label);
            let next = pattern.re.replace_all(&current, replacement.as_str());
            current = Cow::Owned(next.into_owned());
        }
    }
    current
}

/// Verification-gate secrets check (Finding #1) — scan `text` (a diff, not a
/// log line) against the same known secret shapes [`redact_secrets`] uses,
/// returning the distinct labels found, in pattern-file order, without
/// exposing the matched value anywhere in the return. `Vec::is_empty()` means
/// clean.
///
/// Deliberately reuses [`PATTERNS`] rather than a second pattern set — a
/// shape that would be redacted from a log line is exactly a shape that must
/// never reach a commit either; keeping one canonical list means adding a
/// pattern to `redact_patterns.txt` protects both the log stream and the
/// gate in the same one-line diff.
#[must_use]
pub fn scan_for_secrets(text: &str) -> Vec<&'static str> {
    PATTERNS
        .iter()
        .filter(|p| p.re.is_match(text))
        .map(|p| p.label)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn benign_line_is_untouched_and_unallocated() {
        let line = "running cargo test --workspace";
        let redacted = redact_secrets(line);
        assert_eq!(redacted, line);
        assert!(matches!(redacted, Cow::Borrowed(_)));
    }

    #[test]
    fn anthropic_key_is_redacted() {
        let line = "found key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789 in .env";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("sk-ant-"));
        assert!(redacted.contains("[REDACTED:anthropic_key]"));
    }

    #[test]
    fn github_pat_is_redacted() {
        let line = "token=ghp_abcdefghijklmnopqrstuvwxyz0123456789AB";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("ghp_"));
        assert!(redacted.contains("[REDACTED:github_token]"));
    }

    #[test]
    fn aws_secret_env_assignment_is_redacted() {
        let line = "AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("wJalrXUtnFEMI"));
        assert!(redacted.contains("[REDACTED:secret_env_assignment]"));
    }

    #[test]
    fn jwt_is_redacted() {
        let line = "Authorization header: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));
        assert!(redacted.contains("[REDACTED:jwt]"));
    }

    #[test]
    fn postgres_url_with_credentials_is_redacted() {
        let line = "connecting to postgres://dbuser:hunter2@db.internal:5432/prod";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("hunter2"));
        assert!(redacted.contains("[REDACTED:credentialed_url]"));
    }

    #[test]
    fn bearer_token_is_redacted() {
        let line = "curl -H 'Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345'";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("abcdefghijklmnopqrstuvwxyz012345"));
        assert!(redacted.contains("[REDACTED:bearer_header]"));
    }

    #[test]
    fn multiple_secrets_in_one_line_are_all_redacted() {
        let line = "key=sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789 token=ghp_abcdefghijklmnopqrstuvwxyz0123456789AB";
        let redacted = redact_secrets(line);
        assert!(!redacted.contains("sk-ant-"));
        assert!(!redacted.contains("ghp_"));
    }

    #[test]
    fn patterns_file_parses_without_panicking() {
        // Forces PATTERNS to initialize — a malformed regex in
        // redact_patterns.txt would be silently skipped (see
        // `malformed_pattern_is_skipped_not_fatal` below), not panic, but
        // this still asserts the real shipped file yields a non-empty set.
        let _ = redact_secrets("warm the pattern cache");
        assert!(!PATTERNS.is_empty());
    }

    #[test]
    fn malformed_pattern_is_skipped_not_fatal() {
        let src = "good_label\tabc\nbad_label\t[unclosed\nanother_good\txyz";
        let patterns = parse_patterns(src);
        let labels: Vec<&str> = patterns.iter().map(|p| p.label).collect();
        assert_eq!(labels, vec!["good_label", "another_good"]);
    }

    #[test]
    fn line_without_tab_is_skipped() {
        let src = "no_tab_here_at_all";
        assert!(parse_patterns(src).is_empty());
    }

    // ── scan_for_secrets (verification-gate diff check) ─────────────────────

    #[test]
    fn scan_clean_diff_finds_nothing() {
        let diff =
            "diff --git a/src/lib.rs b/src/lib.rs\n+pub fn add(a: i32, b: i32) -> i32 { a + b }\n";
        assert!(scan_for_secrets(diff).is_empty());
    }

    #[test]
    fn scan_flags_a_leaked_key_by_label() {
        let diff = "+const KEY: &str = \"sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789\";";
        let labels = scan_for_secrets(diff);
        assert_eq!(labels, vec!["anthropic_key"]);
    }

    #[test]
    fn scan_never_exposes_the_matched_value() {
        let secret = "AKIAABCDEFGHIJKLMNOP";
        let diff = format!("+aws_access_key_id = \"{secret}\"");
        let labels = scan_for_secrets(&diff);
        assert_eq!(labels, vec!["aws_access_key_id"]);
        // The return type is `Vec<&'static str>` of pattern labels only — it
        // is structurally impossible for a label to equal the scanned value,
        // but assert the obvious anyway as a regression guard.
        assert!(!labels.contains(&secret));
    }

    #[test]
    fn scan_deduplicates_a_label_matched_multiple_times() {
        let diff = "+key1=sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789\n+key2=sk-ant-api03-zyxwvutsrqponmlkjihgfedcba9876543210";
        assert_eq!(scan_for_secrets(diff), vec!["anthropic_key"]);
    }

    #[test]
    fn scan_reports_multiple_distinct_labels() {
        let diff = "+a=sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789\n+b=ghp_abcdefghijklmnopqrstuvwxyz0123456789AB";
        let labels = scan_for_secrets(diff);
        assert!(labels.contains(&"anthropic_key"));
        assert!(labels.contains(&"github_token"));
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn scan_empty_text_is_clean() {
        assert!(scan_for_secrets("").is_empty());
    }
}
