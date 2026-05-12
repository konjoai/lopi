#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::util::{db_path, fmt_status, is_self_modify_attempt, load_config, status_label};
use lopi_core::TaskStatus;

#[test]
fn fmt_status_known_values() {
    assert_eq!(fmt_status("queued"), "⏳ queued");
    assert_eq!(fmt_status("planning"), "📋 planning");
    assert_eq!(fmt_status("implementing"), "🔨 implementing");
    assert_eq!(fmt_status("testing"), "🧪 testing");
    assert_eq!(fmt_status("scoring"), "📊 scoring");
    assert_eq!(fmt_status("success"), "✅ success");
    assert_eq!(fmt_status("failed"), "❌ failed");
    assert_eq!(fmt_status("rolled_back"), "⏪ rolled back");
}

#[test]
fn fmt_status_unknown_returns_input() {
    assert_eq!(fmt_status("unknown_state"), "unknown_state");
    assert_eq!(fmt_status(""), "");
}

#[test]
fn status_label_queued_and_planning() {
    assert_eq!(status_label(&TaskStatus::Queued), "queued");
    assert_eq!(status_label(&TaskStatus::Planning), "planning");
    assert_eq!(status_label(&TaskStatus::Implementing), "implementing");
    assert_eq!(status_label(&TaskStatus::Testing), "testing");
    assert_eq!(status_label(&TaskStatus::Scoring), "scoring");
    assert_eq!(status_label(&TaskStatus::RolledBack), "rolled back");
}

#[test]
fn status_label_retrying_includes_attempt() {
    let s = status_label(&TaskStatus::Retrying { attempt: 2 });
    assert!(s.contains('2'), "expected attempt number in: {s}");
}

#[test]
fn status_label_success_includes_branch() {
    let s = status_label(&TaskStatus::Success {
        branch: "feat/xyz".into(),
        pr_url: Some("https://example.com/pr/1".into()),
    });
    assert!(s.contains("feat/xyz"));
    assert!(s.contains("https://example.com/pr/1"));
}

#[test]
fn status_label_failed_includes_reason() {
    let s = status_label(&TaskStatus::Failed {
        reason: "timeout".into(),
    });
    assert!(s.contains("timeout"));
}

#[test]
fn is_self_modify_attempt_false_for_nonexistent_path() {
    assert!(!is_self_modify_attempt(std::path::Path::new(
        "/nonexistent/xyz/path/that/cannot/be/canonicalized"
    )));
}

#[test]
fn is_self_modify_attempt_false_for_unrelated_system_path() {
    assert!(!is_self_modify_attempt(std::path::Path::new("/usr")));
}

#[test]
fn is_self_modify_attempt_true_inside_exe_tree() {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(grandparent) = exe.parent().and_then(|p| p.parent()) {
            let probe = grandparent.join("__self_modify_probe__");
            if std::fs::create_dir_all(&probe).is_ok() {
                let result = is_self_modify_attempt(&probe);
                let _ = std::fs::remove_dir_all(&probe);
                assert!(
                    result,
                    "dir inside exe grandparent should be detected as self-modify"
                );
                return;
            }
        }
    }
    assert!(!is_self_modify_attempt(std::path::Path::new(
        "/nonexistent"
    )));
}

#[test]
fn db_path_has_correct_filename_and_parent() {
    let p = db_path();
    assert_eq!(
        p.file_name(),
        Some(std::ffi::OsStr::new("lopi.db")),
        "filename should be lopi.db"
    );
    assert_eq!(
        p.parent().and_then(|d| d.file_name()),
        Some(std::ffi::OsStr::new(".lopi")),
        "parent dir should be .lopi"
    );
}

#[test]
fn load_config_nonexistent_path_returns_none() {
    let p = std::path::PathBuf::from("/nonexistent/__lopi_test__.toml");
    let result = load_config(Some(&p));
    assert!(
        result.is_none(),
        "nonexistent config path should return None"
    );
}
