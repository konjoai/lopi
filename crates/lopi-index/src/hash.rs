//! `BLAKE3` file hashing — the dirty-working-tree change signal `reindex.rs`
//! uses when there's no clean `git diff` to lean on.

/// Hash `contents` and return the lowercase hex digest.
#[must_use]
pub fn hash_bytes(contents: &[u8]) -> String {
    blake3::hash(contents).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::hash_bytes;

    #[test]
    fn same_bytes_hash_the_same() {
        assert_eq!(hash_bytes(b"fn main() {}"), hash_bytes(b"fn main() {}"));
    }

    #[test]
    fn different_bytes_hash_differently() {
        assert_ne!(hash_bytes(b"a"), hash_bytes(b"b"));
    }

    #[test]
    fn empty_input_is_stable() {
        assert_eq!(hash_bytes(b""), hash_bytes(b""));
    }
}
