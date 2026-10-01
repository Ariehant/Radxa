//! Phase 1.5: WASM plugin host, IPC bridge, and guest API bindings.
//! See docs/PLUGIN_API.md.

pub mod wasm_host;
pub mod guest_api;
pub mod ipc_bridge;

use crate::error::Result;
use crate::registry::plugin::PluginManifest;
use std::path::PathBuf;

/// A loaded plugin instance. Carries state, exports, and lifecycle hooks.
pub struct Plugin {
    pub manifest: PluginManifest,
    pub plugin_dir: PathBuf,
    // WASM instance and store will be added in Phase 1.5.1
}

impl Plugin {
    /// Load a plugin from `vault/.nexus/plugins/<id>/`.
    pub fn load(manifest: PluginManifest, plugin_dir: PathBuf) -> Result<Self> {
        // Validation already done by registry; manifest is trusted.
        Ok(Self {
            manifest,
            plugin_dir,
        })
    }
}
