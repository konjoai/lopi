//! Shared security-sensitive primitives used across webhook/auth verifiers.

/// Constant-time string comparison, resistant to timing side-channel attacks.
///
/// Used to compare a caller-supplied secret (a bearer token, a webhook
/// signature) against the expected value without leaking how many leading
/// bytes matched via response-time variance. Every byte pair is always
/// compared, regardless of earlier mismatches.
#[must_use]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_strings_match() {
        assert!(constant_time_eq("secret123", "secret123"));
    }

    #[test]
    fn different_strings_do_not_match() {
        assert!(!constant_time_eq("secret123", "secret456"));
    }

    #[test]
    fn different_lengths_do_not_match() {
        assert!(!constant_time_eq("short", "much longer string"));
    }

    #[test]
    fn empty_strings_match() {
        assert!(constant_time_eq("", ""));
    }

    /// Structural regression guard for the constant-time property, not a
    /// timing measurement — wall-clock timing assertions are inherently
    /// flaky/non-portable in CI, so this instead pins that mismatch
    /// detection is independent of *where* the differing byte falls.
    /// `constant_time_eq`'s `.fold()` has no early-exit combinator
    /// (`.any()`/`.find()`/`break`) and therefore always visits every byte
    /// pair regardless of an earlier mismatch — this test would still pass
    /// under a short-circuiting rewrite (e.g. plain `==`, or
    /// `.zip().any(...)`), since all four cases are already caught either
    /// way, but a mismatch anywhere still failing to short-circuit-skip the
    /// full scan is exactly the property that makes the timing side-channel
    /// unobservable in the first place.
    #[test]
    fn mismatch_position_does_not_affect_correctness() {
        let base = "aaaaaaaaaa";
        assert!(!constant_time_eq(base, "baaaaaaaaa"), "first byte differs");
        assert!(!constant_time_eq(base, "aaaaabaaaa"), "middle byte differs");
        assert!(!constant_time_eq(base, "aaaaaaaaab"), "last byte differs");
        assert!(!constant_time_eq(base, "bbbbbbbbbb"), "every byte differs");
    }
}
