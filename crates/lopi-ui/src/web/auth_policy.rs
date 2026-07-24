//! Sprint S2, Phase 1 — fail-closed auth startup policy.
//!
//! Auth is required by default. Running without it is an explicit opt-out
//! (`--insecure-no-auth` / `[web].insecure_no_auth`), and that opt-out is
//! itself refused on any non-loopback bind address — there is no legitimate
//! reason to disable auth on an interface other than localhost, and it is
//! the single highest-value check in this sprint (an unauthenticated,
//! publicly reachable dashboard holding a bearer token's worth of trust).
//!
//! Lives in `lopi-ui` (not the CLI wrapper) so the guard applies to every
//! caller of [`super::serve`] / [`super::serve_with_repo`], not just
//! `lopi sail` — the same reasoning F3's hard stops use: the check lives in
//! the runner, where a caller doesn't get to skip it by accident.

use anyhow::{bail, Result};

/// Validate the auth/bind-address policy before the server starts
/// listening. Called once at startup, before the `TcpListener::bind`.
///
/// - `auth_token` present → auth enabled, always `Ok`.
/// - `auth_token` absent and `insecure_no_auth` not set → refuse: auth is
///   required unless explicitly disabled.
/// - `auth_token` absent, `insecure_no_auth` set, `host` loopback → `Ok`,
///   with a loud warning naming the bind address.
/// - `auth_token` absent, `insecure_no_auth` set, `host` not loopback →
///   refuse: that combination has no legitimate use.
///
/// # Errors
/// Returns an error with a message identifying which case fired and how to
/// fix it — never a generic "refused" with no next step.
pub fn validate_auth_policy(
    auth_token: Option<&str>,
    insecure_no_auth: bool,
    host: &str,
) -> Result<()> {
    if auth_token.is_some() {
        return Ok(());
    }

    if !insecure_no_auth {
        bail!(
            "refusing to start: no [web].auth_token is configured and \
             --insecure-no-auth was not passed. Auth is required by default. \
             Set [web].auth_token in lopi.toml (or LOPI_WEB_AUTH_TOKEN — see \
             docs/RUNNING.md), or pass --insecure-no-auth to explicitly run \
             without authentication (permitted only on a loopback bind address)."
        );
    }

    if is_loopback_host(host) {
        tracing::warn!(
            "⚠️  auth is DISABLED (--insecure-no-auth) on {host} — reachable only from this \
             machine. Never pass --insecure-no-auth with --host 0.0.0.0 or any public interface."
        );
        Ok(())
    } else {
        bail!(
            "refusing to start: --insecure-no-auth was passed but the bind address '{host}' is \
             not loopback. Running without authentication on a non-loopback interface exposes \
             the API to the network — this combination has no legitimate use. Set \
             [web].auth_token instead, or bind to a loopback address."
        );
    }
}

/// Whether `host` is a loopback address — `127.0.0.1`, `::1`, or the
/// literal string `"localhost"` (not itself an IP, but never routable).
/// Anything else — including an unparseable string — is treated as
/// non-loopback, the fail-closed default.
fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_present_always_passes_regardless_of_flag_or_host() {
        assert!(validate_auth_policy(Some("secret"), false, "127.0.0.1").is_ok());
        assert!(validate_auth_policy(Some("secret"), false, "0.0.0.0").is_ok());
        assert!(validate_auth_policy(Some("secret"), true, "0.0.0.0").is_ok());
    }

    #[test]
    fn no_token_no_opt_out_refuses_regardless_of_host() {
        assert!(validate_auth_policy(None, false, "127.0.0.1").is_err());
        assert!(validate_auth_policy(None, false, "0.0.0.0").is_err());
    }

    #[test]
    fn no_token_opt_out_on_loopback_passes() {
        assert!(validate_auth_policy(None, true, "127.0.0.1").is_ok());
        assert!(validate_auth_policy(None, true, "::1").is_ok());
        assert!(validate_auth_policy(None, true, "localhost").is_ok());
        assert!(validate_auth_policy(None, true, "LOCALHOST").is_ok());
    }

    #[test]
    fn no_token_opt_out_on_non_loopback_refuses() {
        assert!(validate_auth_policy(None, true, "0.0.0.0").is_err());
        assert!(validate_auth_policy(None, true, "::").is_err());
        assert!(validate_auth_policy(None, true, "203.0.113.5").is_err());
    }

    #[test]
    fn unparseable_host_treated_as_non_loopback() {
        assert!(validate_auth_policy(None, true, "not-an-ip.example.com").is_err());
    }
}
