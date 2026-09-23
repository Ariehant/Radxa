use crate::index::{query, sql};
use crate::rpc::Router;
use serde::Deserialize;

#[derive(Deserialize)]
struct DirParams {
    dir: String,
}

pub fn register(r: &mut Router) {
    // Views never store data: they query the index and render.
    r.add("view.flowTable", |s, p: DirParams| {
        let v = s.vault()?;
        let c = v.index.read()?;
        query::flow_table(&c, &format!("{}/flow.md", crate::flow::flow_dir(&p.dir))).map_err(sql)
    });
}
