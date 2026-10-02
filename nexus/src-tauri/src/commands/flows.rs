use crate::flow::{self, types::PortType, Position, TemplateSummary};
use crate::rpc::Router;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
struct DirParams {
    dir: String,
}

#[derive(Deserialize)]
struct NameParams {
    name: String,
}

#[derive(Deserialize)]
struct LayoutParams {
    dir: String,
    positions: Vec<Position>,
}

#[derive(Deserialize)]
struct EdgeParams {
    dir: String,
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct AddNodeParams {
    dir: String,
    #[serde(rename = "ref")]
    template: String,
    x: f64,
    y: f64,
    #[serde(default)]
    config: Option<Value>,
}

#[derive(Deserialize)]
struct NodeParams {
    dir: String,
    id: String,
}

#[derive(Deserialize)]
struct UpdateNodeParams {
    dir: String,
    id: String,
    #[serde(default)]
    config: Option<Value>,
    #[serde(default)]
    title: Option<String>,
}

pub fn register(r: &mut Router) {
    r.add("flow.list", |s, _p: Value| flow::list(&*s.vault()?));
    r.add("flow.load", |s, p: DirParams| Ok(flow::view(&flow::load(&*s.vault()?, &p.dir)?)));
    r.add("flow.create", |s, p: NameParams| flow::create(&*s.vault()?, &p.name));
    r.add("flow.saveLayout", |s, p: LayoutParams| flow::save_layout(&*s.vault()?, &p.dir, &p.positions));
    r.add("flow.connect", |s, p: EdgeParams| flow::connect(&*s.vault()?, &p.dir, &p.from, &p.to));
    r.add("flow.disconnect", |s, p: EdgeParams| flow::disconnect(&*s.vault()?, &p.dir, &p.from, &p.to));
    r.add("flow.addNode", |s, p: AddNodeParams| flow::add_node(&*s.vault()?, &p.dir, &p.template, p.x, p.y, p.config));
    r.add("flow.removeNode", |s, p: NodeParams| flow::remove_node(&*s.vault()?, &p.dir, &p.id));
    r.add("flow.updateNode", |s, p: UpdateNodeParams| flow::update_node(&*s.vault()?, &p.dir, &p.id, p.config, p.title));
    r.add("template.list", |s, _p: Value| {
        let mut t: Vec<TemplateSummary> = flow::load_templates(&*s.vault()?)?.values().map(TemplateSummary::from).collect();
        t.sort_by(|a, b| a.title.cmp(&b.title));
        Ok(t)
    });
    // The UI mirrors these rules for instant feedback; the backend re-checks.
    r.add("port.types", |_s, _p: Value| {
        let names: Vec<&str> = PortType::ALL.iter().map(|t| t.as_str()).collect();
        let compat: Vec<Vec<bool>> = PortType::ALL.iter().map(|a| PortType::ALL.iter().map(|b| a.connects_to(*b)).collect()).collect();
        Ok(json!({ "types": names, "compatible": compat }))
    });
}
