//! NodeRegistry: node kinds by id. Templates in `templates/nodes/*.md`
//! declare a `kind`; the engine looks the executor up here.

use crate::error::Result;
use crate::flow::engine::{NodeCtx, Outputs};
use serde::Serialize;
use std::collections::BTreeMap;

pub type Executor = fn(&NodeCtx) -> Result<Outputs>;

#[derive(Clone, Serialize)]
pub struct NodeKind {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// Reuse the last output while config + inputs are unchanged. Nodes that
    /// read live vault state must say `false` so they always re-run.
    pub cacheable: bool,
    #[serde(skip)]
    pub run: Executor,
}

#[derive(Default)]
pub struct NodeRegistry {
    kinds: BTreeMap<&'static str, NodeKind>,
}

mod generated {
    include!(concat!(env!("OUT_DIR"), "/node_kinds.rs"));
}

impl NodeRegistry {
    pub fn add(&mut self, kind: NodeKind) {
        let prev = self.kinds.insert(kind.id, kind);
        assert!(prev.is_none(), "node kind registered twice");
    }

    pub fn get(&self, id: &str) -> Option<&NodeKind> {
        self.kinds.get(id)
    }

    pub fn list(&self) -> Vec<&NodeKind> {
        self.kinds.values().collect()
    }

    /// All built-in kinds from `src/nodes/*.rs` (discovered by build.rs).
    pub fn builtin() -> NodeRegistry {
        let mut r = NodeRegistry::default();
        generated::register_all(&mut r);
        r
    }
}
