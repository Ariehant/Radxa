//! WASM module loading and execution. Uses wasmtime.
//! Phase 1.5.1: WASI context, IPC bridge, and guest API functions are TODO.

use crate::error::{NexusError, Result};
use std::path::Path;
use wasmtime::{Engine, Instance, Linker, Module, Store};

/// WASM host context: engine and module loader. Data stored in the store is ().
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

    /// Create an empty store for instantiation.
    pub fn create_store(&self) -> Store<()> {
        Store::new(&self.engine, ())
    }

    /// Instantiate a WASM module with a linker.
    pub fn instantiate(&self, module: &Module, mut store: Store<()>) -> Result<Instance> {
        let linker = Linker::new(&self.engine);
        // TODO: Phase 1.5.1 — add WASI + guest API functions to linker.
        linker
            .instantiate(&mut store, module)
            .map_err(|e| NexusError::msg(format!("failed to instantiate module: {}", e)))
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
