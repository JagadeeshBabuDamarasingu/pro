use async_trait::async_trait;
use time::OffsetDateTime;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("registry request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected registry response shape: {0}")]
    UnexpectedShape(String),
}

#[derive(Debug, Clone)]
pub struct PublishRecord {
    pub version: String,
    pub published_at: OffsetDateTime,
}

/// One client per package ecosystem, all queried the same way by
/// `SupplyChainGate`. Extensible to more ecosystems (PyPI, etc.) without
/// touching the gate logic itself.
#[async_trait]
pub trait RegistryClient: Send + Sync {
    fn ecosystem(&self) -> &'static str;

    async fn exists(&self, package: &str) -> Result<bool, RegistryError>;

    /// `None` when the registry doesn't expose a download count (or the
    /// package doesn't exist).
    async fn download_count(&self, package: &str) -> Result<Option<u64>, RegistryError>;

    /// Raw install/build script bodies to run through
    /// `scan_install_script`. Empty when the ecosystem has no equivalent
    /// mechanism, or when inspecting it would require downloading and
    /// extracting the package source (deliberately out of scope here — see
    /// `CratesIoRegistryClient`'s doc comment).
    async fn install_scripts(&self, package: &str) -> Result<Vec<String>, RegistryError>;

    async fn publish_history(&self, package: &str) -> Result<Vec<PublishRecord>, RegistryError>;
}

pub struct NpmRegistryClient {
    http: reqwest::Client,
}

impl Default for NpmRegistryClient {
    fn default() -> Self {
        NpmRegistryClient { http: reqwest::Client::new() }
    }
}

impl NpmRegistryClient {
    async fn fetch_metadata(&self, package: &str) -> Result<Option<serde_json::Value>, RegistryError> {
        let url = format!("https://registry.npmjs.org/{package}");
        let response = self.http.get(url).send().await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(response.json().await?))
    }
}

#[async_trait]
impl RegistryClient for NpmRegistryClient {
    fn ecosystem(&self) -> &'static str {
        "npm"
    }

    async fn exists(&self, package: &str) -> Result<bool, RegistryError> {
        Ok(self.fetch_metadata(package).await?.is_some())
    }

    async fn download_count(&self, package: &str) -> Result<Option<u64>, RegistryError> {
        let url = format!("https://api.npmjs.org/downloads/point/last-week/{package}");
        let response = self.http.get(url).send().await?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let body: serde_json::Value = response.json().await?;
        Ok(body.get("downloads").and_then(|d| d.as_u64()))
    }

    async fn install_scripts(&self, package: &str) -> Result<Vec<String>, RegistryError> {
        let Some(meta) = self.fetch_metadata(package).await? else {
            return Ok(Vec::new());
        };
        let latest = meta.get("dist-tags").and_then(|d| d.get("latest")).and_then(|v| v.as_str());
        let Some(latest) = latest else {
            return Ok(Vec::new());
        };
        let scripts = meta
            .get("versions")
            .and_then(|v| v.get(latest))
            .and_then(|v| v.get("scripts"))
            .and_then(|s| s.as_object());
        let Some(scripts) = scripts else {
            return Ok(Vec::new());
        };
        Ok(["preinstall", "install", "postinstall"]
            .iter()
            .filter_map(|hook| scripts.get(*hook).and_then(|v| v.as_str()).map(str::to_string))
            .collect())
    }

    async fn publish_history(&self, package: &str) -> Result<Vec<PublishRecord>, RegistryError> {
        let Some(meta) = self.fetch_metadata(package).await? else {
            return Ok(Vec::new());
        };
        let Some(time_map) = meta.get("time").and_then(|t| t.as_object()) else {
            return Ok(Vec::new());
        };
        let mut records = Vec::new();
        for (version, ts) in time_map {
            if version == "created" || version == "modified" {
                continue;
            }
            if let Some(ts) = ts.as_str() {
                if let Ok(published_at) = OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339) {
                    records.push(PublishRecord { version: version.clone(), published_at });
                }
            }
        }
        records.sort_by_key(|r| r.published_at);
        Ok(records)
    }
}

/// crates.io API access. Deliberately does **not** implement
/// `install_scripts` beyond an empty vec: unlike npm's registry metadata
/// (which inlines `scripts` directly), inspecting a crate's `build.rs`
/// requires downloading and extracting its source tarball — a materially
/// bigger network/parsing footprint this gate doesn't take on in Phase 2.
/// The existence/edit-distance/download-disparity/publish-velocity checks
/// still apply in full.
pub struct CratesIoRegistryClient {
    http: reqwest::Client,
}

impl Default for CratesIoRegistryClient {
    fn default() -> Self {
        // crates.io requires a descriptive User-Agent on all API requests.
        let http = reqwest::Client::builder()
            .user_agent("pro-supply-chain (https://github.com/pro; supply-chain gate)")
            .build()
            .expect("client builder with a static user agent never fails");
        CratesIoRegistryClient { http }
    }
}

impl CratesIoRegistryClient {
    async fn fetch_metadata(&self, package: &str) -> Result<Option<serde_json::Value>, RegistryError> {
        let url = format!("https://crates.io/api/v1/crates/{package}");
        let response = self.http.get(url).send().await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(response.json().await?))
    }
}

#[async_trait]
impl RegistryClient for CratesIoRegistryClient {
    fn ecosystem(&self) -> &'static str {
        "crates.io"
    }

    async fn exists(&self, package: &str) -> Result<bool, RegistryError> {
        Ok(self.fetch_metadata(package).await?.is_some())
    }

    async fn download_count(&self, package: &str) -> Result<Option<u64>, RegistryError> {
        let Some(meta) = self.fetch_metadata(package).await? else {
            return Ok(None);
        };
        Ok(meta.get("crate").and_then(|c| c.get("downloads")).and_then(|d| d.as_u64()))
    }

    async fn install_scripts(&self, _package: &str) -> Result<Vec<String>, RegistryError> {
        Ok(Vec::new())
    }

    async fn publish_history(&self, package: &str) -> Result<Vec<PublishRecord>, RegistryError> {
        let Some(meta) = self.fetch_metadata(package).await? else {
            return Ok(Vec::new());
        };
        let versions = meta.get("versions").and_then(|v| v.as_array());
        let Some(versions) = versions else {
            return Ok(Vec::new());
        };
        let mut records = Vec::new();
        for v in versions {
            let num = v.get("num").and_then(|n| n.as_str());
            let created = v.get("created_at").and_then(|c| c.as_str());
            if let (Some(num), Some(created)) = (num, created) {
                if let Ok(published_at) = OffsetDateTime::parse(created, &time::format_description::well_known::Rfc3339)
                {
                    records.push(PublishRecord { version: num.to_string(), published_at });
                }
            }
        }
        records.sort_by_key(|r| r.published_at);
        Ok(records)
    }
}
