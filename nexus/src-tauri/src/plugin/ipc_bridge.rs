//! IPC bridge for external plugins (sidecar processes, separate binaries).
//! JSON-RPC 2.0 over Unix socket (Linux), named pipe (Windows), or TCP localhost.
//! Same protocol as the in-app Tauri RPC (see rpc.rs).

use crate::error::{NexusError, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// External sidecar process speaking JSON-RPC 2.0 over stdin/stdout.
pub struct SidecarBridge {
    process: Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl SidecarBridge {
    /// Spawn a sidecar process and wrap its stdin/stdout.
    pub fn spawn(sidecar_exe: &Path) -> Result<Self> {
        let mut process = Command::new(sidecar_exe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| NexusError::msg(format!("failed to spawn sidecar: {}", e)))?;

        let stdin = process
            .stdin
            .take()
            .ok_or_else(|| NexusError::msg("no stdin handle"))?;
        let stdout = BufReader::new(
            process
                .stdout
                .take()
                .ok_or_else(|| NexusError::msg("no stdout handle"))?,
        );

        Ok(Self {
            process,
            stdin,
            stdout,
            next_id: 1,
        })
    }

    /// Send a JSON-RPC 2.0 request and read the response (line-delimited).
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        // Send request.
        writeln!(self.stdin, "{}", request.to_string())
            .map_err(|e| NexusError::msg(format!("failed to write to sidecar: {}", e)))?;

        // Read response.
        let mut response_line = String::new();
        self.stdout
            .read_line(&mut response_line)
            .map_err(|e| NexusError::msg(format!("failed to read from sidecar: {}", e)))?;

        let response: Value = serde_json::from_str(&response_line)
            .map_err(|e| NexusError::msg(format!("invalid JSON-RPC response: {}", e)))?;

        if let Some(err) = response.get("error") {
            let msg: &str = err
                .get("message")
                .and_then(|m: &Value| m.as_str())
                .unwrap_or("unknown");
            return Err(NexusError::msg(format!("sidecar error: {}", msg)));
        }

        response
            .get("result")
            .cloned()
            .ok_or_else(|| NexusError::msg("no result in response"))
    }

    /// Terminate the sidecar process.
    pub fn kill(&mut self) -> Result<()> {
        self.process
            .kill()
            .map_err(|e| NexusError::msg(format!("failed to kill sidecar: {}", e)))?;
        Ok(())
    }
}

/// Unix socket or named pipe listener for external plugins.
/// TODO: Phase 1.5.2 — socket setup and multiplexing over multiple connections.
pub struct SocketListener {
    // Placeholder for socket path / pipe / TCP listener.
}

impl SocketListener {
    pub fn new(_socket_path: &str) -> Result<Self> {
        // TODO: bind socket and accept connections.
        Ok(Self {})
    }

    /// Accept and handle one JSON-RPC 2.0 request, dispatch to plugin registry.
    pub fn handle_one(&mut self) -> Result<()> {
        // TODO: read line-delimited JSON, call the appropriate plugin method, return response.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_rpc_request_format() {
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "test.method",
            "params": {}
        });
        assert_eq!(req.get("jsonrpc").unwrap().as_str(), Some("2.0"));
    }
}
