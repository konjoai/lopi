#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_raw_string_hashes
)]
use super::*;

#[test]
fn schedule_entry_deserializes() {
    let toml = r#"
name = "nightly-lint"
repo = "/Users/wesleyscholl/myrepo"
goal = "Fix all clippy warnings"
cron = "0 2 * * *"
priority = "low"
"#;
    let entry: ScheduleEntry = toml::from_str(toml).unwrap();
    assert_eq!(entry.name, "nightly-lint");
    assert_eq!(entry.cron, "0 2 * * *");
    assert_eq!(entry.priority, "low");
}

#[test]
fn config_with_schedules_deserializes() {
    let toml = r#"
[lopi]
max_agents = 2

[claude]
cli_path = "claude"

[git]
default_allowed_dirs = ["src/"]
default_forbidden_dirs = [".github/"]

[[schedules]]
name = "weekly-deps"
repo = "/repo"
goal = "Update dependencies"
cron = "0 9 * * MON"
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.schedules.len(), 1);
    assert_eq!(cfg.schedules[0].name, "weekly-deps");
    assert_eq!(cfg.lopi.max_agents, 2);
}

#[test]
fn config_empty_schedules_is_default() {
    let toml = r#"
[lopi]
max_agents = 4

[claude]
cli_path = "claude"

[git]
default_allowed_dirs = ["src/"]
default_forbidden_dirs = []
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert!(cfg.schedules.is_empty());
}

#[test]
fn repo_profile_default_is_empty() {
    let p = RepoProfile::default();
    assert!(p.allowed_dirs.is_empty());
    assert!(p.test_command.is_none());
}

#[test]
fn repo_profile_apply_overrides_task() {
    let mut task = crate::task::Task::new("do something");
    let profile = RepoProfile {
        allowed_dirs: vec!["lib/".into()],
        forbidden_dirs: vec!["vendor/".into()],
        default_constraints: vec!["no new dependencies".into()],
        max_retries: Some(5),
        ..Default::default()
    };
    profile.apply(&mut task);
    assert_eq!(task.allowed_dirs, vec!["lib/"]);
    assert_eq!(task.max_retries, 5);
    assert!(task
        .constraints
        .contains(&"no new dependencies".to_string()));
}

#[test]
fn repo_profile_apply_skips_empty_overrides() {
    let mut task = crate::task::Task::new("do something");
    let original_allowed = task.allowed_dirs.clone();
    let profile = RepoProfile::default();
    profile.apply(&mut task);
    // Empty profile should not override task defaults.
    assert_eq!(task.allowed_dirs, original_allowed);
}

#[test]
fn lopi_config_default_values_are_set() {
    let toml = r#"
[lopi]

[claude]

[git]
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.lopi.max_agents, 4);
    assert_eq!(cfg.lopi.log_level, "info");
    assert_eq!(cfg.claude.cli_path, "claude");
    assert_eq!(cfg.claude.timeout_secs, 300);
    assert!(cfg.git.auto_pr);
}

#[test]
fn lopi_config_web_defaults() {
    let toml = r#"
[lopi]

[claude]

[git]
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.web.port, 3000);
    assert_eq!(cfg.web.host, "127.0.0.1");
    assert!(cfg.web.auth_token.is_none());
}

#[test]
fn lopi_config_web_with_auth_token() {
    let toml = r#"
[lopi]

[claude]

[git]

[web]
port = 8080
host = "0.0.0.0"
auth_token = "my-secret-token"
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.web.port, 8080);
    assert_eq!(cfg.web.host, "0.0.0.0");
    assert_eq!(cfg.web.auth_token.unwrap(), "my-secret-token");
}

#[test]
fn lopi_config_remote_defaults() {
    let toml = r#"
[lopi]

[claude]

[git]
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert!(cfg.remote.telegram.token.is_none());
    assert!(cfg.remote.telegram.chat_id.is_none());
    assert!(cfg.remote.telegram.allowed_chat_ids.is_empty());
    assert!(cfg.remote.whatsapp.account_sid.is_none());
    assert!(cfg.remote.whatsapp.auth_token.is_none());
}

#[test]
fn lopi_config_telegram_settings() {
    let toml = r#"
[lopi]

[claude]

[git]

[remote.telegram]
token = "12345:ABC"
chat_id = 999
allowed_chat_ids = [111, 222, 333]
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.remote.telegram.token.unwrap(), "12345:ABC");
    assert_eq!(cfg.remote.telegram.chat_id.unwrap(), 999);
    assert_eq!(cfg.remote.telegram.allowed_chat_ids, vec![111, 222, 333]);
}

#[test]
fn lopi_config_whatsapp_settings() {
    let toml = r#"
[lopi]

[claude]

[git]

[remote.whatsapp]
account_sid = "ACtest"
auth_token = "authtoken"
from = "whatsapp:+15551234567"
signing_secret = "mysigningsecret"
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.remote.whatsapp.account_sid.unwrap(), "ACtest");
    assert_eq!(cfg.remote.whatsapp.auth_token.unwrap(), "authtoken");
    assert_eq!(cfg.remote.whatsapp.from.unwrap(), "whatsapp:+15551234567");
    assert_eq!(
        cfg.remote.whatsapp.signing_secret.unwrap(),
        "mysigningsecret"
    );
}

#[test]
fn git_config_allowed_and_forbidden_defaults() {
    let toml = r#"
[lopi]

[claude]

[git]
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert!(cfg.git.default_allowed_dirs.contains(&"src/".to_string()));
    assert!(cfg
        .git
        .default_forbidden_dirs
        .contains(&".github/".to_string()));
}

#[test]
fn schedule_entry_default_priority_is_normal() {
    let toml = r#"
name = "my-schedule"
repo = "/tmp/repo"
goal = "Run tests"
cron = "0 3 * * *"
"#;
    let entry: ScheduleEntry = toml::from_str(toml).unwrap();
    assert_eq!(entry.priority, "normal");
    assert!(entry.allowed_dirs.is_empty());
    assert!(entry.forbidden_dirs.is_empty());
}

#[test]
fn schedule_entry_with_dirs() {
    let toml = r#"
name = "targeted"
repo = "/tmp/repo"
goal = "Fix lint"
cron = "0 4 * * *"
priority = "high"
allowed_dirs = ["src/", "lib/"]
forbidden_dirs = ["vendor/"]
"#;
    let entry: ScheduleEntry = toml::from_str(toml).unwrap();
    assert_eq!(entry.priority, "high");
    assert_eq!(entry.allowed_dirs, vec!["src/", "lib/"]);
    assert_eq!(entry.forbidden_dirs, vec!["vendor/"]);
}

#[test]
fn repo_profile_apply_sets_max_retries() {
    let mut task = crate::task::Task::new("test");
    let profile = RepoProfile {
        max_retries: Some(7),
        ..Default::default()
    };
    profile.apply(&mut task);
    assert_eq!(task.max_retries, 7);
}

#[test]
fn repo_profile_apply_does_not_override_when_empty() {
    let mut task = crate::task::Task::new("test");
    task.max_retries = 3;
    let profile = RepoProfile::default();
    profile.apply(&mut task);
    // max_retries None means no override
    assert_eq!(task.max_retries, 3);
}

#[test]
fn repo_profile_load_from_nonexistent_path_returns_default() {
    let p = RepoProfile::load_from_repo(std::path::Path::new("/nonexistent/path"));
    assert!(p.allowed_dirs.is_empty());
    assert!(p.test_command.is_none());
    assert!(p.lint_command.is_none());
}

#[test]
fn lopi_config_multiple_schedules() {
    let toml = r#"
[lopi]
max_agents = 8

[claude]
timeout_secs = 600

[git]
auto_pr = false

[[schedules]]
name = "daily-tests"
repo = "/repo1"
goal = "Run all tests"
cron = "0 1 * * *"

[[schedules]]
name = "weekly-cleanup"
repo = "/repo2"
goal = "Clean up stale branches"
cron = "0 9 * * SUN"
priority = "low"
"#;
    let cfg: LopiConfig = toml::from_str(toml).unwrap();
    assert_eq!(cfg.schedules.len(), 2);
    assert_eq!(cfg.lopi.max_agents, 8);
    assert_eq!(cfg.claude.timeout_secs, 600);
    assert!(!cfg.git.auto_pr);
    assert_eq!(cfg.schedules[0].name, "daily-tests");
    assert_eq!(cfg.schedules[1].priority, "low");
}

#[test]
fn web_config_default_impl() {
    let web = WebConfig::default();
    assert_eq!(web.port, 3000);
    assert_eq!(web.host, "127.0.0.1");
    assert!(web.auth_token.is_none());
}
