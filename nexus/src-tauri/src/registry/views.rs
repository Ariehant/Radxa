//! ViewRegistry: views are named, parameterised queries over the index.
//! Views never store data — they query SQLite and the UI renders the rows.
//! The frontend keeps a matching registry of renderers (src/views/registry.ts).

use crate::error::Result;
use crate::vault::Vault;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub type ViewQuery = fn(&Vault, &Value) -> Result<Value>;

#[derive(Clone, Serialize)]
pub struct ViewDef {
    pub id: &'static str,
    pub label: &'static str,
    /// What the view is opened on: `flow`, `note`, `database`, …
    pub target: &'static str,
    /// Renderer ids the UI may use for this view's rows.
    pub layouts: &'static [&'static str],
    #[serde(skip)]
    pub query: ViewQuery,
}

#[derive(Default)]
pub struct ViewRegistry {
    views: BTreeMap<&'static str, ViewDef>,
}

impl ViewRegistry {
    pub fn add(&mut self, v: ViewDef) {
        let prev = self.views.insert(v.id, v);
        assert!(prev.is_none(), "view registered twice");
    }

    pub fn get(&self, id: &str) -> Option<&ViewDef> {
        self.views.get(id)
    }

    pub fn list(&self) -> Vec<&ViewDef> {
        self.views.values().collect()
    }

    pub fn builtin() -> ViewRegistry {
        let mut r = ViewRegistry::default();
        r.add(ViewDef {
            id: "flow.table",
            label: "Flow nodes & edges",
            target: "flow",
            layouts: &["table", "hybrid"],
            query: |v, p| {
                let dir = p.get("dir").and_then(Value::as_str).unwrap_or_default();
                let c = v.index.read()?;
                let t = crate::index::query::flow_table(&c, &format!("{}/flow.md", crate::flow::flow_dir(dir)))?;
                Ok(serde_json::to_value(t)?)
            },
        });
        r.add(ViewDef {
            id: "notes.byKind",
            label: "Notes of a kind",
            target: "vault",
            layouts: &["table", "list"],
            query: |v, p| {
                let kind = p.get("kind").and_then(Value::as_str).unwrap_or("note");
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(500) as usize;
                let c = v.index.read()?;
                Ok(serde_json::to_value(crate::index::query::nodes_by_kind(&c, kind, limit)?)?)
            },
        });
        r
    }
}
