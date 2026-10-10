//! Host functions exported to guest WASM plugins.
//! These are linked into the linker and called by plugin code.
//! Capability-checked against the manifest permissions.

use crate::error::Result;
use serde_json::{json, Value};

/// Guest API context: carries capability flags and references to vault/index/engine.
pub struct GuestApiCtx {
    /// Manifest permissions as capability flags.
    pub can_fs_read: bool,
    pub can_fs_write: bool,
    pub can_llm_call: bool,
    pub can_git: bool,
    pub can_network: bool,
}

impl GuestApiCtx {
    pub fn new(
        can_fs_read: bool,
        can_fs_write: bool,
        can_llm_call: bool,
        can_git: bool,
        can_network: bool,
    ) -> Self {
        Self {
            can_fs_read,
            can_fs_write,
            can_llm_call,
            can_git,
            can_network,
        }
    }

    /// Host function: read_note(path: &str) -> Result<String>
    /// Capability: fs_read. Returns note content or error JSON.
    pub fn read_note(&self, path: &str) -> Result<Value> {
        if !self.can_fs_read {
            return Ok(json!({ "error": "fs_read capability not granted" }));
        }
        // TODO: delegate to vault/index to read the note.
        // For now, placeholder.
        Ok(json!({ "path": path, "content": "" }))
    }

    /// Host function: write_note(path: &str, content: &str) -> Result<String>
    /// Capability: fs_write. Returns hash or error JSON.
    pub fn write_note(&self, path: &str, _content: &str) -> Result<Value> {
        if !self.can_fs_write {
            return Ok(json!({ "error": "fs_write capability not granted" }));
        }
        // TODO: delegate to vault/index to write and index the note.
        Ok(json!({ "path": path, "hash": "0x0" }))
    }

    /// Host function: query_graph(query: &str) -> Result<String> (JSON)
    /// Capability: fs_read. Returns results or error JSON.
    pub fn query_graph(&self, query: &str) -> Result<Value> {
        if !self.can_fs_read {
            return Ok(json!({ "error": "fs_read capability not granted" }));
        }
        // TODO: parse the query (backlinks_of, links_of, search) and delegate to index.
        Ok(json!({ "query": query, "results": [] }))
    }

    /// Host function: llm_chat(request: &str) -> Result<String> (JSON)
    /// Capability: llm_call. Returns response or error JSON.
    pub fn llm_chat(&self, request: &str) -> Result<Value> {
        if !self.can_llm_call {
            return Ok(json!({ "error": "llm_call capability not granted" }));
        }
        // TODO: delegate to agent loop.
        Ok(json!({ "request": request, "response": "" }))
    }

    /// Host function: git_commit(message: &str) -> Result<String> (sha)
    /// Capability: git. Returns commit sha or error JSON.
    pub fn git_commit(&self, message: &str) -> Result<Value> {
        if !self.can_git {
            return Ok(json!({ "error": "git capability not granted" }));
        }
        // TODO: delegate to git layer.
        Ok(json!({ "sha": "0x0", "message": message }))
    }

    /// Host function: http(request: &str) -> Result<String> (JSON response)
    /// Capability: network (with host allowlist). Returns response or error JSON.
    pub fn http(&self, request: &str) -> Result<Value> {
        if !self.can_network {
            return Ok(json!({ "error": "network capability not granted" }));
        }
        // TODO: parse request (method, url, body), check host allowlist, delegate to reqwest.
        Ok(json!({ "request": request, "status": 200, "body": "" }))
    }

    /// Host function: log(level: &str, message: &str) -> Result<()>
    /// Always allowed. Appended to .nexus/audit/<timestamp>.log.
    pub fn log(&self, level: &str, message: &str) -> Result<()> {
        // TODO: append to audit log. For now, println.
        eprintln!("[{}] {}", level, message);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_api_respects_capabilities() {
        let ctx = GuestApiCtx::new(false, false, false, false, false);
        let result = ctx.read_note("test.md").unwrap();
        assert!(result.get("error").is_some());
    }

    #[test]
    fn guest_api_grants_capability() {
        let ctx = GuestApiCtx::new(true, false, false, false, false);
        let result = ctx.read_note("test.md").unwrap();
        assert!(result.get("path").is_some());
    }
}
