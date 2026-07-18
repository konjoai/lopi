//! UTF-8-safe string truncation shared across crates that excerpt
//! arbitrary, unvalidated text (diffs, issue bodies, task goals) for
//! display or prompting.

/// Truncate `s` to at most `max_bytes` bytes, backing off to the nearest
/// preceding UTF-8 character boundary so the cut never lands mid-character.
///
/// Plain byte-index slicing (`&s[..n]`) panics when `n` splits a multibyte
/// character; this never does, and never returns more than `max_bytes` bytes.
#[must_use]
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorter_than_limit_is_unchanged() {
        assert_eq!(safe_truncate("hello", 10), "hello");
    }

    #[test]
    fn exact_length_is_unchanged() {
        assert_eq!(safe_truncate("hello", 5), "hello");
    }

    #[test]
    fn ascii_cutoff_slices_cleanly() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn multibyte_char_straddling_cutoff_backs_off_instead_of_panicking() {
        // "é" is 2 bytes (0xC3 0xA9); "a" + "é" is 3 bytes total, so cutting
        // at byte 2 would land inside "é" if we sliced naively.
        let s = "aé";
        assert_eq!(safe_truncate(s, 2), "a");
    }

    #[test]
    fn emoji_straddling_cutoff_backs_off() {
        // "🦀" is 4 bytes; cutting at byte 3 must not panic and must not
        // include a partial character.
        let s = "ab🦀cd";
        assert_eq!(safe_truncate(s, 3), "ab");
        assert_eq!(safe_truncate(s, 5), "ab");
        assert_eq!(safe_truncate(s, 6), "ab🦀");
    }

    #[test]
    fn zero_max_bytes_returns_empty() {
        assert_eq!(safe_truncate("hello", 0), "");
    }
}
