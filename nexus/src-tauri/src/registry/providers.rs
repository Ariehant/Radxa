//! ProviderRegistry: LLM providers by id (`[llm] provider = "..."`).

use crate::agent::provider::{LlmProvider, ProviderFactory};
use crate::config::LlmConfig;
use crate::error::{NexusError, Result};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
pub struct ProviderInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub needs_api_key: bool,
    #[serde(skip)]
    pub factory: ProviderFactory,
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<&'static str, ProviderInfo>,
}

impl ProviderRegistry {
    pub fn add(&mut self, p: ProviderInfo) {
        let prev = self.providers.insert(p.id, p);
        assert!(prev.is_none(), "provider registered twice");
    }

    pub fn list(&self) -> Vec<&ProviderInfo> {
        self.providers.values().collect()
    }

    pub fn create(&self, cfg: &LlmConfig) -> Result<Box<dyn LlmProvider>> {
        let p = self
            .providers
            .get(cfg.provider.as_str())
            .ok_or_else(|| NexusError::invalid(format!("unknown LLM provider `{}` (have: {})", cfg.provider, self.providers.keys().copied().collect::<Vec<_>>().join(", "))))?;
        Ok((p.factory)(cfg))
    }

    pub fn builtin() -> ProviderRegistry {
        let mut r = ProviderRegistry::default();
        r.add(ProviderInfo { id: "ollama", label: "Ollama (local)", needs_api_key: false, factory: crate::agent::ollama::factory });
        r.add(ProviderInfo { id: "echo", label: "Echo (offline, for testing)", needs_api_key: false, factory: crate::agent::provider::echo_factory });
        r
    }
}
