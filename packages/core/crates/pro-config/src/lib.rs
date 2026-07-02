use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: String,
    pub model: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileName {
    ReadOnly,
    Workspace,
    DangerFullAccess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub default: ProfileName,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    pub log_path: String,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub provider: ProviderConfig,
    pub profile: ProfileConfig,
    pub audit: AuditConfig,
    /// Optional second provider for `pro-confidence`'s model cascade: the
    /// primary `[provider]` is the cheap/local tier, `[cascade_provider]`
    /// is the strong tier only called on low-confidence escalation.
    #[serde(default)]
    pub cascade_provider: Option<ProviderConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config file at {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let raw = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config: Config = toml::from_str(&raw)?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let raw = r#"
            [provider]
            name = "ollama"
            base_url = "http://localhost:11434/v1"
            model = "functiongemma:latest"

            [profile]
            default = "read-only"

            [audit]
            log_path = ".pro/audit.jsonl"
            retention_days = 90
        "#;
        let config: Config = toml::from_str(raw).expect("valid config");
        assert_eq!(config.provider.name, "ollama");
        assert_eq!(config.profile.default, ProfileName::ReadOnly);
        assert_eq!(config.audit.retention_days, 90);
    }
}
