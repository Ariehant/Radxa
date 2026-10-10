//! Registries replace hard-coded dispatch: node kinds, LLM providers, views
//! (and, via `rpc::Router`, RPC methods). Phase 1.5 plugins register into
//! these same tables; `plugin` defines the manifest they ship with.

pub mod nodes;
pub mod plugin;
pub mod providers;
pub mod views;

use std::sync::OnceLock;

pub struct Registries {
    pub nodes: nodes::NodeRegistry,
    pub providers: providers::ProviderRegistry,
    pub views: views::ViewRegistry,
}

/// Process-wide built-in registries.
pub fn get() -> &'static Registries {
    static R: OnceLock<Registries> = OnceLock::new();
    R.get_or_init(|| Registries {
        nodes: nodes::NodeRegistry::builtin(),
        providers: providers::ProviderRegistry::builtin(),
        views: views::ViewRegistry::builtin(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_in_src_nodes_is_a_registered_kind() {
        let mut files: Vec<String> = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src/nodes"))
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".rs").map(str::to_owned))
            .collect();
        files.sort();
        let kinds: Vec<&str> = get().nodes.list().iter().map(|k| k.id).collect();
        assert_eq!(kinds, files, "node kind id must match its file name");
        assert!(!get().nodes.get("fetch").unwrap().cacheable);
        assert!(get().nodes.get("agent").unwrap().cacheable);
    }

    #[test]
    fn providers_and_views() {
        let ids: Vec<&str> = get().providers.list().iter().map(|p| p.id).collect();
        assert_eq!(ids, vec!["echo", "ollama"]);
        let cfg = crate::config::LlmConfig { provider: "nope".into(), ..Default::default() };
        let err = get().providers.create(&cfg).err().unwrap().to_string();
        assert!(err.contains("have: echo, ollama"), "{err}");
        assert!(get().views.get("flow.table").is_some());
    }

    #[test]
    fn new_kind_runs_in_a_flow_via_a_vault_template() {
        use crate::state::NullSink;
        let d = tempfile::tempdir().unwrap();
        let v = crate::vault::Vault::create(d.path(), std::sync::Arc::new(NullSink), false).unwrap();
        v.write_text(
            "templates/nodes/greet.md",
            "---\nkind: text_template\ninputs: []\noutputs:\n  - { name: out, type: string }\nconfig:\n  template: \"Hello from {{flow}} / {{node}}\"\n---\n",
            None,
        )
        .unwrap();
        let f = crate::flow::create(&v, "Greeter").unwrap();
        crate::flow::add_node(&v, &f.dir, "templates/nodes/greet.md", 0.0, 0.0, None).unwrap();
        let r = v.engine.run(&v, &f.dir, None, true).unwrap();
        assert_eq!(r.nodes[0].outputs["out"], "Hello from Greeter / n1");
    }
}
