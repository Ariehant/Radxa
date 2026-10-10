//! Plugin discovery and loading from a vault's .nexus/plugins/ directory.

use crate::error::{NexusError, Result};
use crate::registry::plugin::PluginManifest;
use std::collections::BTreeMap;
use std::path::PathBuf;
use walkdir::WalkDir;

/// Discovers and loads all plugins in `vault/.nexus/plugins/`.
pub struct PluginLoader {
    plugins_dir: PathBuf,
}

impl PluginLoader {
    pub fn new(vault_root: &PathBuf) -> Self {
        let plugins_dir = vault_root.join(".nexus").join("plugins");
        Self { plugins_dir }
    }

    /// Scan the plugins directory and load all valid plugin manifests.
    /// Returns a map of plugin id → (manifest, plugin_dir_path).
    pub fn load_all(&self) -> Result<BTreeMap<String, (PluginManifest, PathBuf)>> {
        let mut plugins = BTreeMap::new();

        // Plugins are directories like vault/.nexus/plugins/com.example.tool/
        if !self.plugins_dir.exists() {
            return Ok(plugins);
        }

        for entry in WalkDir::new(&self.plugins_dir)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
        {
            let plugin_dir = entry.path();
            let manifest_path = plugin_dir.join("plugin.toml");

            // Load and parse the manifest.
            let manifest_text = match std::fs::read_to_string(&manifest_path) {
                Ok(text) => text,
                Err(e) => {
                    log::warn!("failed to read {}: {}", manifest_path.display(), e);
                    continue;
                }
            };

            match PluginManifest::parse(&manifest_text) {
                Ok(manifest) => {
                    plugins.insert(manifest.plugin.id.clone(), (manifest, plugin_dir.to_path_buf()));
                }
                Err(e) => {
                    log::warn!("invalid plugin manifest at {}: {}", manifest_path.display(), e);
                }
            }
        }

        Ok(plugins)
    }

    /// Load a single plugin by id.
    pub fn load_plugin(&self, plugin_id: &str) -> Result<(PluginManifest, PathBuf)> {
        let plugin_dir = self.plugins_dir.join(plugin_id);
        let manifest_path = plugin_dir.join("plugin.toml");

        let manifest_text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| NexusError::msg(format!("failed to read plugin manifest: {}", e)))?;

        let manifest = PluginManifest::parse(&manifest_text)
            .map_err(|e| NexusError::msg(format!("invalid plugin manifest: {}", e)))?;

        Ok((manifest, plugin_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loader_handles_missing_plugins_dir() -> Result<()> {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let loader = PluginLoader::new(&temp_dir.path().to_path_buf());
        let plugins = loader.load_all()?;
        assert!(plugins.is_empty());
        Ok(())
    }

    #[test]
    fn loader_skips_invalid_manifests() -> Result<()> {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let plugins_dir = temp_dir.path().join(".nexus").join("plugins");
        fs::create_dir_all(&plugins_dir).unwrap();

        // Create a dir with an invalid manifest.
        let bad_plugin_dir = plugins_dir.join("com.bad.plugin");
        fs::create_dir(&bad_plugin_dir).unwrap();
        fs::write(bad_plugin_dir.join("plugin.toml"), "invalid toml [[[").unwrap();

        let loader = PluginLoader::new(&temp_dir.path().to_path_buf());
        let plugins = loader.load_all()?;
        assert!(plugins.is_empty()); // Bad manifest is skipped with a warning.
        Ok(())
    }
}
