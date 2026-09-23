use super::*;
use crate::state::NullSink;
use std::sync::Arc;

fn vault() -> (tempfile::TempDir, Arc<Vault>) {
    let dir = tempfile::tempdir().unwrap();
    let v = Vault::create(dir.path(), Arc::new(NullSink), false).unwrap();
    (dir, v)
}

const FETCH: &str = "templates/nodes/fetch-data.md";
const LLM: &str = "templates/nodes/llm-summarize.md";
const WRITE: &str = "templates/nodes/write-note.md";

#[test]
fn three_nodes_connected_saved_reopened_preserved() {
    let (_d, v) = vault();
    let f = create(&v, "My Flow").unwrap();
    assert_eq!(f.dir, "flows/my-flow");
    add_node(&v, &f.dir, FETCH, 100.0, 100.0, None).unwrap();
    add_node(&v, &f.dir, LLM, 480.0, 100.0, None).unwrap();
    add_node(&v, &f.dir, WRITE, 860.0, 100.0, None).unwrap();
    connect(&v, &f.dir, "n1.out", "n2.in").unwrap();
    let after = connect(&v, &f.dir, "n2.out", "n3.content").unwrap();
    assert!(after.errors.is_empty(), "{:?}", after.errors);
    assert!(after.edges.iter().all(|e| e.valid));

    // Reopen from disk: nodes, positions and edges preserved.
    let re = view(&load(&v, "flows/my-flow/flow.md").unwrap());
    assert_eq!(re.nodes.iter().map(|n| (n.id.as_str(), n.x)).collect::<Vec<_>>(), vec![("n1", 100.0), ("n2", 480.0), ("n3", 860.0)]);
    assert_eq!(re.edges.len(), 2);
    assert_eq!(re.order.unwrap(), vec!["n1", "n2", "n3"]);
    let md = v.read_text("flows/my-flow/flow.md").unwrap();
    assert!(md.contains("type: document[]"), "{md}");
}

#[test]
fn moving_touches_canvas_only_and_edges_touch_flow_md_only() {
    let (_d, v) = vault();
    let dir = "flows/deploy-pipeline";
    let md_before = v.read_text(&md_path(dir)).unwrap();
    save_layout(&v, dir, &[Position { id: "n2".into(), x: 500.4, y: 260.0, w: None, h: None }]).unwrap();
    assert_eq!(v.read_text(&md_path(dir)).unwrap(), md_before, "flow.md untouched by a move");
    assert_eq!(load(&v, dir).unwrap().canvas.get("n2").unwrap().x, 500.0);

    let canvas_before = v.read_text(&canvas_path(dir)).unwrap();
    disconnect(&v, dir, "n2.out", "n3.content").unwrap();
    assert_eq!(v.read_text(&canvas_path(dir)).unwrap(), canvas_before, "canvas untouched by an edge edit");
    assert_eq!(load(&v, dir).unwrap().flow.meta.edges.len(), 1);
}

#[test]
fn incompatible_edges_are_blocked_and_not_written() {
    let (_d, v) = vault();
    let f = create(&v, "t").unwrap();
    add_node(&v, &f.dir, LLM, 0.0, 0.0, None).unwrap();
    add_node(&v, &f.dir, LLM, 400.0, 0.0, None).unwrap();
    let before = v.read_text(&md_path(&f.dir)).unwrap();
    let err = connect(&v, &f.dir, "n1.out", "n2.in").unwrap_err();
    assert_eq!(err.to_string(), "invalid: type mismatch: string → document[]");
    assert!(connect(&v, &f.dir, "n1.nope", "n2.in").is_err());
    assert_eq!(v.read_text(&md_path(&f.dir)).unwrap(), before);
}

#[test]
fn remove_node_drops_its_edges_and_layout() {
    let (_d, v) = vault();
    let dir = "flows/deploy-pipeline";
    let after = remove_node(&v, dir, "n2").unwrap();
    assert_eq!(after.nodes.len(), 2);
    assert!(after.edges.is_empty());
    assert!(load(&v, dir).unwrap().canvas.get("n2").is_none());
}

#[test]
fn flows_are_indexed_as_queryable_nodes_and_edges() {
    let (_d, v) = vault();
    v.index.flush().unwrap();
    let flows = list(&v).unwrap();
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].nodes, 3);
    let c = v.index.read().unwrap();
    let steps: i64 = c.query_row("SELECT count(*) FROM nodes WHERE kind = 'flow-node'", [], |r| r.get(0)).unwrap();
    assert_eq!(steps, 3);
    let typed: Vec<(String, String)> = c
        .prepare("SELECT port, data_type FROM edges WHERE kind = 'flow' ORDER BY port")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(typed, vec![("out>content".to_string(), "string".to_string()), ("out>in".to_string(), "document[]".to_string())]);
    let layout: i64 = c.query_row("SELECT count(*) FROM canvas_layout", [], |r| r.get(0)).unwrap();
    assert_eq!(layout, 3);
    // Templates show "used by" backlinks.
    let bl = crate::index::query::backlinks(&c, LLM).unwrap();
    assert_eq!(bl[0].path, "flows/deploy-pipeline/flow.md");
}

#[test]
fn flow_table_reads_the_index() {
    let (_d, v) = vault();
    let dir = "flows/deploy-pipeline";
    v.index.flush().unwrap();
    save_layout(&v, dir, &[Position { id: "n3".into(), x: 900.0, y: 333.0, w: None, h: None }]).unwrap();
    let c = v.index.read().unwrap();
    let t = crate::index::query::flow_table(&c, "flows/deploy-pipeline/flow.md").unwrap();
    assert_eq!(t.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), vec!["n1", "n2", "n3"]);
    assert_eq!(t.nodes[1].kind.as_deref(), Some("agent"));
    assert_eq!(t.nodes[1].template_title.as_deref(), Some("LLM Summarize"));
    assert_eq!((t.nodes[1].inputs, t.nodes[1].outputs), (1, 1));
    assert_eq!(t.nodes[2].y, Some(333.0), "positions come from canvas_layout");
    assert_eq!(t.edges[0].from, "n1.out");
    assert_eq!(t.edges[0].to, "n2.in");
    assert_eq!(t.edges[0].data_type.as_deref(), Some("document[]"));
}
