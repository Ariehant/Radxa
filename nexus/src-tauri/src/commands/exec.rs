use crate::rpc::Router;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct RunParams {
    dir: String,
    /// Run this node (and its upstream); omit to run the whole flow.
    #[serde(default)]
    node: Option<String>,
    #[serde(default = "yes")]
    use_cache: bool,
}

fn yes() -> bool {
    true
}

#[derive(Deserialize)]
struct DirParams {
    dir: String,
}

pub fn register(r: &mut Router) {
    r.add("exec.run", |s, p: RunParams| {
        let v = s.vault()?;
        v.engine.run(&v, &p.dir, p.node.as_deref(), p.use_cache)
    });
    r.add("exec.last", |s, p: DirParams| Ok(s.vault()?.engine.last_run(&crate::flow::flow_dir(&p.dir))));
    r.add("exec.clearCache", |s, p: DirParams| {
        s.vault()?.engine.clear(&crate::flow::flow_dir(&p.dir));
        Ok(())
    });
    r.add("llm.models", |s, _p: Value| {
        let cfg = s.vault()?.config();
        crate::agent::provider_for(&cfg.llm)?.models()
    });
    r.add("config.get", |s, _p: Value| Ok(s.vault()?.config()));
    r.add("config.set", |s, p: crate::config::Config| {
        s.vault()?.set_config(p.clone())?;
        Ok(p)
    });
}
