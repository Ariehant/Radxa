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
    /// Only for providers that need one; Ollama doesn't.
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

impl Config {
    pub fn load(root: &Path) -> Config {
        match std::fs::read_to_string(path(root)) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                log::warn!("config.toml invalid, using defaults: {e}");
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let s = toml::to_string_pretty(self).map_err(|e| NexusError::Other(e.to_string()))?;
        atomic_write(&path(root), s.as_bytes())
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
}
