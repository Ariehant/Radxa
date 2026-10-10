//! `plugin.toml` manifest schema (spec §10.3). Phase 1 defines and validates
//! the manifest only; the WASM host that loads plugins lands in Phase 1.5.
//! See docs/PLUGIN_API.md.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    #[serde(default)]
    pub entry: Entry,
    #[serde(default)]
    pub connection: Option<Connection>,
    #[serde(default)]
    pub auth: Option<Auth>,
    #[serde(default)]
    pub permissions: Permissions,
    #[serde(default)]
    pub socket: Option<Socket>,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Node kinds / providers / views the plugin contributes (Phase 1.5).
    #[serde(default)]
    pub contributes: Contributes,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginInfo {
    /// Reverse-DNS id, e.g. `com.nexus.github`.
    pub id: String,
    pub name: String,
    /// SemVer.
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub min_nexus_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// WASI module, relative to the plugin directory.
    #[serde(default)]
    pub wasm: Option<String>,
    /// Optional UI bundle (ES module) rendered in a sandboxed frame.
    #[serde(default)]
    pub ui: Option<String>,
    /// External sidecar (stdin/stdout JSON-RPC), Tier 7.
    #[serde(default)]
    pub sidecar: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionType {
    Http,
    Websocket,
    Unix,
    Pipe,
    Tcp,
    Stdio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    #[serde(rename = "type")]
    pub kind: ConnectionType,
    #[serde(default)]
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    None,
    ApiKey,
    Oauth2,
    Basic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Auth {
    pub method: AuthMethod,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Capability-based permissions; the user approves these on install.
/// Nothing is granted by default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Permissions {
    /// Globs, each starting with `vault/`.
    #[serde(default)]
    pub fs_read: Vec<String>,
    #[serde(default)]
    pub fs_write: Vec<String>,
    /// Host allowlist (no scheme, no path). Empty = no network.
    #[serde(default)]
    pub network: Vec<String>,
    #[serde(default)]
    pub llm_call: bool,
    #[serde(default)]
    pub git: bool,
    /// Resource limits enforced by the host.
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
    #[serde(default)]
    pub max_cpu_ms: Option<u32>,
    #[serde(default)]
    pub max_disk_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Socket {
    /// Unix domain socket path (Linux).
    #[serde(default)]
    pub path: Option<String>,
    /// Named pipe (Windows), e.g. `\\.\pipe\nexus-github`.
    #[serde(default)]
    pub pipe: Option<String>,
    /// TCP localhost fallback port.
    #[serde(default)]
    pub tcp_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct Contributes {
    /// Node kind id → exported WASM function.
    #[serde(default)]
    pub nodes: BTreeMap<String, String>,
    #[serde(default)]
    pub providers: BTreeMap<String, String>,
    #[serde(default)]
    pub views: BTreeMap<String, String>,
}

fn semver_ok(v: &str) -> bool {
    let core = v.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

fn relative_inside(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('/') && !p.starts_with('\\') && !p.contains(':') && !p.split(['/', '\\']).any(|s| s == "..")
}

impl PluginManifest {
    pub fn parse(src: &str) -> Result<PluginManifest, String> {
        let m: PluginManifest = toml::from_str(src).map_err(|e| e.to_string())?;
        let errs = m.validate();
        if errs.is_empty() {
            Ok(m)
        } else {
            Err(errs.join("; "))
        }
    }

    /// All problems, not just the first, so authors can fix them in one go.
    pub fn validate(&self) -> Vec<String> {
        let mut e = Vec::new();
        let id = &self.plugin.id;
        let segs: Vec<&str> = id.split('.').collect();
        if segs.len() < 2 || segs.iter().any(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')) {
            e.push(format!("plugin.id `{id}` must be reverse-DNS (e.g. com.example.tool)"));
        }
        if self.plugin.name.trim().is_empty() {
            e.push("plugin.name is empty".into());
        }
        for (k, v) in [("plugin.version", &self.plugin.version), ("plugin.min_nexus_version", &self.plugin.min_nexus_version)] {
            if !semver_ok(v) {
                e.push(format!("{k} `{v}` is not semver"));
            }
        }
        if self.entry.wasm.is_none() && self.entry.sidecar.is_none() && self.entry.ui.is_none() {
            e.push("entry needs at least one of wasm, sidecar, ui".into());
        }
        for (k, v) in [("entry.wasm", &self.entry.wasm), ("entry.ui", &self.entry.ui), ("entry.sidecar", &self.entry.sidecar)] {
            if let Some(p) = v {
                if !relative_inside(p) {
                    e.push(format!("{k} `{p}` must be a relative path inside the plugin"));
                }
            }
        }
        if let Some(w) = &self.entry.wasm {
            if !w.ends_with(".wasm") {
                e.push(format!("entry.wasm `{w}` must be a .wasm file"));
            }
        }
        for (k, globs) in [("permissions.fs_read", &self.permissions.fs_read), ("permissions.fs_write", &self.permissions.fs_write)] {
            for g in globs {
                if !(g == "vault" || g.starts_with("vault/")) || g.split('/').any(|s| s == "..") {
                    e.push(format!("{k} `{g}` must be a glob under vault/"));
                }
                if g.starts_with("vault/.nexus") {
                    e.push(format!("{k} `{g}`: .nexus/ (index, credentials) is off-limits"));
                }
            }
        }
        for h in &self.permissions.network {
            if h.contains("://") || h.contains('/') || h.is_empty() || h == "*" {
                e.push(format!("permissions.network `{h}` must be a bare host name"));
            }
        }
        if let Some(c) = &self.connection {
            if matches!(c.kind, ConnectionType::Http | ConnectionType::Websocket) {
                match &c.endpoint {
                    None => e.push("connection.endpoint is required for http/websocket".into()),
                    Some(ep) => {
                        let host = ep.split("://").nth(1).unwrap_or("").split(['/', ':']).next().unwrap_or("");
                        let local = host == "localhost" || host == "127.0.0.1";
                        if !local && !self.permissions.network.iter().any(|h| h == host) {
                            e.push(format!("connection.endpoint host `{host}` is not in permissions.network"));
                        }
                    }
                }
            }
        }
        if let Some(a) = &self.auth {
            if a.method == AuthMethod::Oauth2 && a.scopes.is_empty() {
                e.push("auth.scopes is required for oauth2".into());
            }
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The manifest exactly as written in spec §10.3.
    pub const SPEC_EXAMPLE: &str = r#"
[plugin]
id = "com.nexus.github"
name = "GitHub Integration"
version = "1.0.0"
author = "Nexus"
min_nexus_version = "0.2.0"

[entry]
wasm = "plugin.wasm"
ui = "ui.js"

[connection]
type = "http"
endpoint = "https://api.github.com"

[auth]
method = "oauth2"
scopes = ["repo", "issues"]

[permissions]
fs_read = ["vault/**/*.md"]
fs_write = ["vault/runs/**"]
network = ["api.github.com"]
llm_call = true
git = true

[socket]
path = "/tmp/nexus-github.sock"
pipe = "\\\\.\\pipe\\nexus-github"

[capabilities]
read = ["issues", "prs", "repos"]
write = ["issues.comment", "prs.review"]
events = ["webhook.push", "webhook.pr"]
"#;

    #[test]
    fn spec_example_parses_and_validates() {
        let m = PluginManifest::parse(SPEC_EXAMPLE).unwrap();
        assert_eq!(m.plugin.id, "com.nexus.github");
        assert_eq!(m.auth.unwrap().method, AuthMethod::Oauth2);
        assert_eq!(m.socket.unwrap().pipe.as_deref(), Some(r"\\.\pipe\nexus-github"));
        assert!(m.permissions.llm_call);
        let shipped = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/examples/plugin.toml")).unwrap();
        PluginManifest::parse(&shipped).unwrap();
    }

    #[test]
    fn validation_catches_unsafe_manifests() {
        let bad = r#"
[plugin]
id = "GitHub"
name = ""
version = "1.0"
min_nexus_version = "0.2.0"

[entry]
wasm = "../evil.so"

[connection]
type = "http"
endpoint = "https://exfil.example.com/x"

[permissions]
fs_read = ["/etc/**", "vault/.nexus/credentials.toml"]
network = ["https://api.github.com", "*"]
"#;
        let m: PluginManifest = toml::from_str(bad).unwrap();
        let errs = m.validate().join("\n");
        for needle in [
            "reverse-DNS",
            "plugin.name is empty",
            "plugin.version `1.0`",
            "relative path inside",
            "must be a .wasm file",
            "`/etc/**` must be a glob under vault/",
            ".nexus/ (index, credentials) is off-limits",
            "bare host name",
            "`exfil.example.com` is not in permissions.network",
        ] {
            assert!(errs.contains(needle), "missing `{needle}` in:\n{errs}");
        }
        assert!(toml::from_str::<PluginManifest>("[plugin]\nid='a.b'\nname='x'\nversion='1.0.0'\nmin_nexus_version='0.1.0'\nsurprise=1").is_err(), "unknown keys rejected");
    }
}
