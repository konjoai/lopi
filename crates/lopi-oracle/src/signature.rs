use std::collections::BTreeSet;

/// Identity of one open collision: the conflicted file set plus the
/// merge-base commit both sides diverged from.
///
/// Two polls of the same still-unresolved collision produce the same
/// signature — that is the whole point. [`CollisionOracle`](crate::CollisionOracle)
/// alerts once per signature, not once per poll, which is the direct fix for
/// KT-2's noise-floor finding (`KILL_TEST_REGISTER.md`): a poll-and-count
/// metric cannot tell "12 fresh collisions" from "1 collision, polled 12
/// times before anyone resolved it." Either side advancing changes the
/// merge-base, which changes the signature, which is correctly treated as
/// new information rather than the same alert repeating.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConflictSignature {
    files: BTreeSet<String>,
    merge_base: String,
}

impl ConflictSignature {
    /// Build a signature from a conflicted file list and the pair's
    /// merge-base commit. File order does not affect equality — the set is
    /// what identifies the collision, not the order git happened to report it.
    #[must_use]
    pub fn new(files: impl IntoIterator<Item = String>, merge_base: impl Into<String>) -> Self {
        Self {
            files: files.into_iter().collect(),
            merge_base: merge_base.into(),
        }
    }

    /// The conflicted file paths, in sorted order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(String::as_str)
    }

    /// The merge-base commit both sides diverged from.
    #[must_use]
    pub fn merge_base(&self) -> &str {
        &self.merge_base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_order_does_not_affect_equality() {
        let a = ConflictSignature::new(vec!["b.txt".to_string(), "a.txt".to_string()], "base1");
        let b = ConflictSignature::new(vec!["a.txt".to_string(), "b.txt".to_string()], "base1");
        assert_eq!(a, b, "same file set + base must be the same signature");
    }

    #[test]
    fn different_merge_base_is_a_different_signature() {
        let a = ConflictSignature::new(vec!["f.txt".to_string()], "base1");
        let b = ConflictSignature::new(vec!["f.txt".to_string()], "base2");
        assert_ne!(a, b, "an advanced base is new information, not a repeat");
    }

    #[test]
    fn different_file_set_is_a_different_signature() {
        let a = ConflictSignature::new(vec!["f.txt".to_string()], "base1");
        let b = ConflictSignature::new(vec!["g.txt".to_string()], "base1");
        assert_ne!(a, b);
    }

    #[test]
    fn files_reports_sorted_paths() {
        let sig = ConflictSignature::new(vec!["z.txt".to_string(), "a.txt".to_string()], "base1");
        assert_eq!(sig.files().collect::<Vec<_>>(), vec!["a.txt", "z.txt"]);
        assert_eq!(sig.merge_base(), "base1");
    }
}
