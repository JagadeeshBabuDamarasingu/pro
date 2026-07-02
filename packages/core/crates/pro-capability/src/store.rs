use crate::capability::Capability;
use crate::grant::Grant;
use crate::hash::ContentHash;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    /// An explicit deny matched, or no grant covers the request.
    Denied { reason: String },
    /// A matching grant exists but the source definition's content hash no
    /// longer matches — requires diff-annotated re-approval, never a silent
    /// carry-forward of trust (`02-security-core.md` §1).
    Stale {
        expected: ContentHash,
        actual: ContentHash,
    },
}

/// Holds active grants and explicit denies for one session/project.
/// **Explicit deny always wins** — checked before any allow-grant lookup.
#[derive(Debug, Default)]
pub struct GrantStore {
    grants: Vec<Grant>,
    denies: Vec<Capability>,
}

impl GrantStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_grant(&mut self, grant: Grant) {
        self.grants.push(grant);
    }

    pub fn add_deny(&mut self, capability: Capability) {
        self.denies.push(capability);
    }

    pub fn grants(&self) -> &[Grant] {
        &self.grants
    }

    /// Checks a requested action against active grants. `current_content_hash`
    /// is the hash of the source definition *as it exists right now* (e.g.
    /// re-hashed from the live config/hook/script on every use); if it
    /// differs from the hash recorded at grant time, the grant is stale.
    pub fn check(&self, requested: &Capability, current_content_hash: Option<ContentHash>) -> Decision {
        if self.denies.iter().any(|deny| deny.covers(requested)) {
            return Decision::Denied {
                reason: format!("explicit deny matches requested {}", requested.kind()),
            };
        }

        let matching = self.grants.iter().find(|g| g.capability.covers(requested));
        let Some(grant) = matching else {
            return Decision::Denied {
                reason: format!("no grant covers requested {}", requested.kind()),
            };
        };

        if let Some(actual) = current_content_hash {
            if actual != grant.content_hash {
                return Decision::Stale {
                    expected: grant.content_hash,
                    actual,
                };
            }
        }

        Decision::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fs_read(paths: &[&str]) -> Capability {
        Capability::FsRead { paths: paths.iter().map(PathBuf::from).collect() }
    }

    #[test]
    fn allows_request_within_granted_scope() {
        let mut store = GrantStore::new();
        store.add_grant(Grant::new_shadow(fs_read(&["/project"])));
        let decision = store.check(&fs_read(&["/project/src/main.rs"]), None);
        assert_eq!(decision, Decision::Allowed);
    }

    #[test]
    fn denies_request_with_no_matching_grant() {
        let store = GrantStore::new();
        let decision = store.check(&fs_read(&["/project"]), None);
        assert!(matches!(decision, Decision::Denied { .. }));
    }

    #[test]
    fn explicit_deny_wins_over_allow_grant() {
        let mut store = GrantStore::new();
        store.add_grant(Grant::new_shadow(fs_read(&["/project"])));
        store.add_deny(fs_read(&["/project/secrets"]));
        let decision = store.check(&fs_read(&["/project/secrets/key.pem"]), None);
        assert!(matches!(decision, Decision::Denied { .. }));
    }

    #[test]
    fn changed_content_hash_marks_grant_stale() {
        let mut store = GrantStore::new();
        let grant = Grant::new_shadow(fs_read(&["/project"]));
        let stale_hash = ContentHash::of_bytes(b"a different source definition");
        store.add_grant(grant);
        let decision = store.check(&fs_read(&["/project/src/main.rs"]), Some(stale_hash));
        assert!(matches!(decision, Decision::Stale { .. }));
    }
}
