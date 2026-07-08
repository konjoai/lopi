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
}
