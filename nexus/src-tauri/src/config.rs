//! Per-vault settings in `.nexus/config.toml`.

use crate::error::{NexusError, Result};
use crate::fs::atomic_write;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LlmConfig {
    /// Provider id from the provider registry (`ollama`, `echo`, …).
    pub provider: String,
    pub base_url: String,
    pub model: String,
    /// Only for providers that need one; Ollama doesn't. Never written to
    /// config.toml — kept in `.nexus/credentials.toml`, which is gitignored
    /// and never auto-committed (OS keychain storage lands in Phase 1.5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    pub timeout_secs: u64,
    pub max_tool_steps: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            provider: "ollama".into(),
            base_url: "http://127.0.0.1:11434".into(),
            model: "llama3.2".into(),
            api_key: None,
            timeout_secs: 180,
            max_tool_steps: 6,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct UiConfig {
    /// `system` | `light` | `dark`
    pub theme: String,
    pub editor_mode: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        UiConfig { theme: "system".into(), editor_mode: "rich".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub ui: UiConfig,
    #[serde(default)]
    pub git: GitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GitConfig {
    pub auto_commit: bool,
}

impl Default for GitConfig {
    fn default() -> Self {
        GitConfig { auto_commit: true }
    }
}

pub fn path(root: &Path) -> PathBuf {
    root.join(".nexus").join("config.toml")
}

/// Vault-relative path of the secrets file; excluded from git unconditionally.
pub const CREDENTIALS_REL: &str = ".nexus/credentials.toml";

pub fn credentials_path(root: &Path) -> PathBuf {
    root.join(".nexus").join("credentials.toml")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Credentials {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    llm_api_key: Option<String>,
}

impl Config {
    pub fn load(root: &Path) -> Config {
        let mut c: Config = match std::fs::read_to_string(path(root)) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                log::warn!("config.toml invalid, using defaults: {e}");
                Config::default()
            }),
            Err(_) => Config::default(),
        };
        if let Ok(s) = std::fs::read_to_string(credentials_path(root)) {
            if let Ok(cred) = toml::from_str::<Credentials>(&s) {
                c.llm.api_key = cred.llm_api_key;
            }
        }
        c
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let mut public = self.clone();
        let key = public.llm.api_key.take().filter(|k| !k.is_empty());
        let s = toml::to_string_pretty(&public).map_err(|e| NexusError::Other(e.to_string()))?;
        atomic_write(&path(root), s.as_bytes())?;
        let cred_path = credentials_path(root);
        match key {
            Some(k) => {
                let s = toml::to_string_pretty(&Credentials { llm_api_key: Some(k) }).map_err(|e| NexusError::Other(e.to_string()))?;
                atomic_write(&cred_path, s.as_bytes())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&cred_path, std::fs::Permissions::from_mode(0o600));
                }
            }
            None => crate::fs::remove_file(&cred_path)?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_partial_files() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(Config::load(d.path()), Config::default());
        std::fs::create_dir_all(d.path().join(".nexus")).unwrap();
        std::fs::write(path(d.path()), "[llm]\nmodel = \"qwen2.5\"\n").unwrap();
        let c = Config::load(d.path());
        assert_eq!(c.llm.model, "qwen2.5");
        assert_eq!(c.llm.provider, "ollama");
        c.save(d.path()).unwrap();
        assert_eq!(Config::load(d.path()), c);
    }

    #[test]
    fn api_key_never_lands_in_config_toml() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Config::default();
        c.llm.api_key = Some("sk-secret".into());
        c.save(d.path()).unwrap();
        assert!(!std::fs::read_to_string(path(d.path())).unwrap().contains("sk-secret"));
        assert_eq!(Config::load(d.path()).llm.api_key.as_deref(), Some("sk-secret"));
        c.llm.api_key = None;
        c.save(d.path()).unwrap();
        assert!(!credentials_path(d.path()).exists());
    }
}
