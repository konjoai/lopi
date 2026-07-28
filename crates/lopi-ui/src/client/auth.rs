//! Auth-token resolution for TUI clients — mirrors `src/sail_commands.rs`'s
//! server-side precedence exactly, so a TUI pointed at a locally-running
//! `lopi sail` authenticates without separate configuration.

use lopi_core::LopiConfig;

/// Resolve the bearer token a [`super::RemoteClient`] should send, using the
/// same precedence `sail_commands::run` uses to decide what token the
/// server itself expects: `[web].auth_token` from the loaded config wins
/// over the `LOPI_WEB_AUTH_TOKEN` env var; `None` when neither is set.
#[must_use]
pub fn resolve_auth_token(cfg: Option<&LopiConfig>) -> Option<String> {
    cfg.and_then(|c| c.web.auth_token.clone())
        .or_else(|| std::env::var("LOPI_WEB_AUTH_TOKEN").ok())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // `std::env::set_var` mutates process-global state; serialize this
    // module's tests so they don't race each other's env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const MINIMAL_TOML: &str = r#"
[lopi]
[claude]
[git]
"#;

    fn cfg_with_token(token: Option<&str>) -> LopiConfig {
        let mut cfg: LopiConfig = toml::from_str(MINIMAL_TOML).unwrap();
        cfg.web.auth_token = token.map(str::to_string);
        cfg
    }

    #[test]
    fn config_token_wins_over_env_var() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK for the duration of this test.
        unsafe {
            std::env::set_var("LOPI_WEB_AUTH_TOKEN", "from-env");
        }
        let cfg = cfg_with_token(Some("from-config"));
        assert_eq!(resolve_auth_token(Some(&cfg)), Some("from-config".to_string()));
        // SAFETY: serialized by ENV_LOCK for the duration of this test.
        unsafe {
            std::env::remove_var("LOPI_WEB_AUTH_TOKEN");
        }
    }

    #[test]
    fn falls_back_to_env_var_when_config_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK for the duration of this test.
        unsafe {
            std::env::set_var("LOPI_WEB_AUTH_TOKEN", "from-env");
        }
        let cfg = cfg_with_token(None);
        assert_eq!(resolve_auth_token(Some(&cfg)), Some("from-env".to_string()));
        // SAFETY: serialized by ENV_LOCK for the duration of this test.
        unsafe {
            std::env::remove_var("LOPI_WEB_AUTH_TOKEN");
        }
    }

    #[test]
    fn none_when_neither_source_set() {
        let _guard = ENV_LOCK.lock().unwrap();
        // SAFETY: serialized by ENV_LOCK for the duration of this test.
        unsafe {
            std::env::remove_var("LOPI_WEB_AUTH_TOKEN");
        }
        assert_eq!(resolve_auth_token(None), None);
        assert_eq!(resolve_auth_token(Some(&cfg_with_token(None))), None);
    }
}
