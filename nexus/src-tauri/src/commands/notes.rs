use crate::index::{query, sql, Priority};
use crate::rpc::Router;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct PathParams {
    path: String,
}

#[derive(Deserialize)]
struct SearchParams {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Deserialize)]
struct PrioritizeParams {
    paths: Vec<String>,
    priority: Priority,
}

#[derive(Deserialize)]
struct TargetsParams {
    targets: Vec<String>,
}

#[derive(Serialize)]
struct NoteContent {
    path: String,
    content: String,
}

pub fn register(r: &mut Router) {
    r.add("note.read", |s, p: PathParams| {
        let v = s.vault()?;
        let path = crate::fs::normalize_rel(&p.path);
        let content = v.read_text(&path)?;
        v.index.touch(&path, Priority::Open);
        Ok(NoteContent { path, content: (*content).clone() })
    });
    r.add("note.backlinks", |s, p: PathParams| {
        let v = s.vault()?;
        let c = v.index.read()?;
        query::backlinks(&c, &crate::fs::normalize_rel(&p.path)).map_err(sql)
    });
    r.add("note.outlinks", |s, p: PathParams| {
        let v = s.vault()?;
        let c = v.index.read()?;
        query::outlinks(&c, &crate::fs::normalize_rel(&p.path)).map_err(sql)
    });
    r.add("note.meta", |s, p: PathParams| {
        let v = s.vault()?;
        let c = v.index.read()?;
        query::node_by_path(&c, &crate::fs::normalize_rel(&p.path)).map_err(sql)
    });
    r.add("link.resolve", |s, p: TargetsParams| {
        let v = s.vault()?;
        let c = v.index.read()?;
        p.targets.iter().map(|t| query::resolve(&c, t).map_err(sql)).collect::<Result<Vec<_>, _>>()
    });
    r.add("link.exists", |s, p: TargetsParams| {
        let v = s.vault()?;
        p.targets.iter().map(|t| v.index.exists(t)).collect::<Result<Vec<_>, _>>()
    });
    r.add("search.query", |s, p: SearchParams| {
        let v = s.vault()?;
        v.index.flush_fts()?;
        let c = v.index.read()?;
        query::search(&c, &p.query, p.limit.min(500)).map_err(sql)
    });
    r.add("index.prioritize", |s, p: PrioritizeParams| {
        let v = s.vault()?;
        for path in p.paths {
            v.index.touch(&path, p.priority);
        }
        Ok(())
    });
    r.add("index.status", |s, _p: serde_json::Value| s.vault()?.index.status());
    r.add("index.rebuild", |s, _p: serde_json::Value| s.vault()?.index.rebuild());
}
