use crate::error::{NexusError, Result};
use crate::vault::Vault;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

/// Push channel from backend to UI (Tauri events in the app, a recorder in tests).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _event: &str, _payload: Value) {}
}

pub struct AppState {
    vault: RwLock<Option<Arc<Vault>>>,
    pub events: Arc<dyn EventSink>,
}

impl AppState {
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        AppState { vault: RwLock::new(None), events }
    }

    pub fn vault(&self) -> Result<Arc<Vault>> {
        self.vault.read().clone().ok_or(NexusError::NoVault)
    }

    /// Swap in a new vault; the old one (and its workers) is dropped.
    pub fn set_vault(&self, v: Option<Arc<Vault>>) {
        let old = std::mem::replace(&mut *self.vault.write(), v);
        if let Some(old) = old {
            old.shutdown();
        }
    }
}
