use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Content hash of the *source definition* backing a grant (a config block,
/// a hook script, a build-script body) — never the containing path. Any
/// diff in that source invalidates the grant. See the "approve once,
/// exploit forever" flaw documented in `02-security-core.md` §1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        ContentHash(out)
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_bytes_hash_identically() {
        assert_eq!(ContentHash::of_bytes(b"hello"), ContentHash::of_bytes(b"hello"));
    }

    #[test]
    fn changed_bytes_invalidate_hash() {
        assert_ne!(ContentHash::of_bytes(b"hello"), ContentHash::of_bytes(b"hello!"));
    }
}
