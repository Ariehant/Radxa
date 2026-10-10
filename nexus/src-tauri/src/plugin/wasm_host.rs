//! WASM module loading and execution. Uses wasmtime.
//! Phase 1.5.1 note: Full WASI context setup with filesystem preopen is deferred to
//! when plugin instantiation is actually used. For now, the host loads and validates modules.

use crate::error::{NexusError, Result};
use std::path::Path;
use wasmtime::{Engine, Linker, Module};

/// WASM host context: engine singleton and module cache.
pub struct WasmHost {
    engine: Engine,
}

impl WasmHost {
    pub fn new() -> Result<Self> {
        let engine = Engine::default();
        Ok(Self { engine })
    }

    /// Load a WASM module from disk.
    pub fn load_module(&self, wasm_path: &Path) -> Result<Module> {
        Module::from_file(&self.engine, wasm_path)
            .map_err(|e| NexusError::msg(format!("failed to load WASM: {}", e)))
    }

    /// Create a linker for instantiation. Phase 1.5.2 will add WASI + guest API.
    pub fn create_linker(&self) -> Linker<()> {
        Linker::new(&self.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;

    #[test]
    fn wasm_host_constructs() -> Result<()> {
        let host = WasmHost::new()?;
        assert_eq!(std::mem::size_of_val(&host) > 0, true);
        Ok(())
    }
}
