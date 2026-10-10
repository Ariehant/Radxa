use super::*;
use crate::state::NullSink;
use std::fs;
use std::time::Instant;

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

fn open(root: &Path, watch: bool) -> Index {
    Index::open(root, Arc::new(NullSink), vec![], watch).unwrap()
}

fn backlink_paths(ix: &Index, rel: &str) -> Vec<String> {
    query::backlinks(&ix.read().unwrap(), rel).unwrap().into_iter().map(|l| l.path).collect()
}

#[test]
fn scan_links_and_backlinks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "notes/rust-ownership.md", "# Rust Ownership\n");
    write(root, "notes/a.md", "---\ntitle: A\nproject: \"[[Alpha Launch]]\"\n---\nsee [[Rust Ownership]]\n");
    write(root, "notes/b.md", "see [[notes/rust-ownership|borrowck]] and ![[diagram.png]]\n");
    write(root, "notes/alpha.md", "---\ntitle: Alpha Launch\n---\n");
    write(root, "attachments/diagram.png", "\u{89}PNG");
    let ix = open(root, false);
    ix.flush().unwrap();

    assert_eq!(backlink_paths(&ix, "notes/rust-ownership.md"), vec!["notes/a.md", "notes/b.md"]);
    let rel = query::backlinks(&ix.read().unwrap(), "notes/alpha.md").unwrap();
    assert_eq!(rel.len(), 1);
    assert_eq!(rel[0].kind, "relation");
    assert_eq!(rel[0].port, "project");

    let out = query::outlinks(&ix.read().unwrap(), "notes/b.md").unwrap();
    let link = out.iter().find(|o| o.kind == "link").unwrap();
    assert_eq!(link.resolved.as_deref(), Some("notes/rust-ownership.md"));
    assert_eq!(query::resolve(&ix.read().unwrap(), "diagram.png").unwrap().as_deref(), Some("attachments/diagram.png"));
    assert!(ix.exists("Rust Ownership").unwrap());
    assert!(!ix.exists("Nope Nothing").unwrap());

    ix.flush_fts().unwrap();
    let hits = query::search(&ix.read().unwrap(), "borrow", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "notes/b.md");
}

#[test]
fn incremental_update_hash_skip_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "notes/t.md", "# Target\n");
    write(root, "notes/s.md", "[[Target]]\n");
    let ix = open(root, false);
    ix.flush().unwrap();
    assert_eq!(backlink_paths(&ix, "notes/t.md"), vec!["notes/s.md"]);

    write(root, "notes/s.md", "no links now\n");
    ix.index_now("notes/s.md").unwrap();
    assert!(backlink_paths(&ix, "notes/t.md").is_empty());

    fs::remove_file(root.join("notes/s.md")).unwrap();
    ix.index_now("notes/s.md").unwrap();
    assert_eq!(query::stats(&ix.read().unwrap()).unwrap().files, 1);

    // Directory removal drops everything below it.
    write(root, "notes/sub/x.md", "x");
    write(root, "notes/sub/y.md", "y");
    ix.touch("notes/sub", Priority::Recent);
    ix.flush().unwrap();
    ix.flush().unwrap();
    assert_eq!(query::stats(&ix.read().unwrap()).unwrap().files, 3);
    fs::remove_dir_all(root.join("notes/sub")).unwrap();
    ix.index_now("notes/sub").unwrap();
    assert_eq!(query::stats(&ix.read().unwrap()).unwrap().files, 1);
}

#[test]
fn delete_index_rebuild_restores_everything() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "notes/t.md", "# Target\n");
    write(root, "notes/s.md", "[[Target]]\n");
    {
        let ix = open(root, false);
        ix.flush().unwrap();
    }
    for s in ["", "-wal", "-shm"] {
        let _ = fs::remove_file(format!("{}{s}", Index::db_path(root).display()));
    }
    let ix = open(root, false);
    ix.flush().unwrap();
    assert_eq!(backlink_paths(&ix, "notes/t.md"), vec!["notes/s.md"]);
    ix.rebuild().unwrap();
    ix.flush().unwrap();
    assert_eq!(backlink_paths(&ix, "notes/t.md"), vec!["notes/s.md"]);
}

#[test]
fn watcher_picks_up_external_edits_and_atomic_renames() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "notes/t.md", "# Target\n");
    let ix = open(root, true);
    ix.flush().unwrap();

    // Simulate an editor's atomic save: write tmp, rename over target.
    write(root, "notes/s.md.swp", "[[Target]]\n");
    fs::rename(root.join("notes/s.md.swp"), root.join("notes/s.md")).unwrap();

    let start = Instant::now();
    loop {
        ix.flush().unwrap();
        if backlink_paths(&ix, "notes/t.md") == vec!["notes/s.md"] {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "watcher never indexed the rename");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn duplicate_frontmatter_ids_do_not_collide() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(root, "notes/a.md", "---\nid: SAME\n---\n");
    write(root, "notes/b.md", "---\nid: SAME\n---\n");
    let ix = open(root, false);
    ix.flush().unwrap();
    assert_eq!(query::stats(&ix.read().unwrap()).unwrap().nodes, 2);
}

/// Spec §9: full reindex of 10k files < 30 s. Run with `cargo test --release -- --ignored`.
#[test]
#[ignore]
fn perf_full_reindex_10k() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for i in 0..10_000 {
        write(
            root,
            &format!("notes/d{}/note-{i}.md", i % 50),
            &format!("---\ntitle: Note {i}\ntype: task\nstatus: Todo\nproject: \"[[Note {}]]\"\n---\n\n# Note {i}\n\nLinks [[Note {}]] and [[Note {}]].\nLorem ipsum dolor sit amet {i}.\n", i / 10, (i + 1) % 10_000, (i * 7) % 10_000),
        );
    }
    let t = Instant::now();
    let ix = open(root, false);
    ix.flush().unwrap();
    let full = t.elapsed();
    let st = query::stats(&ix.read().unwrap()).unwrap();
    assert_eq!(st.files, 10_000);

    let t = Instant::now();
    ix.flush_fts().unwrap();
    let fts = t.elapsed();

    let t = Instant::now();
    let hits = query::search(&ix.read().unwrap(), "lorem 4242", 50).unwrap();
    let search = t.elapsed();
    assert!(!hits.is_empty());

    let t = Instant::now();
    let bl = query::backlinks(&ix.read().unwrap(), "notes/d0/note-100.md").unwrap();
    let rel = t.elapsed();
    assert!(!bl.is_empty());

    let t = Instant::now();
    write(root, "notes/d0/note-0.md", "changed [[Note 5]]");
    ix.index_now("notes/d0/note-0.md").unwrap();
    let save = t.elapsed();

    let t = Instant::now();
    ix.rescan().unwrap();
    ix.flush().unwrap();
    let noop_rescan = t.elapsed();

    eprintln!("full index 10k: {full:?} | fts flush: {fts:?} | search: {search:?} | backlinks: {rel:?} | save→index: {save:?} | no-op rescan: {noop_rescan:?}");
    assert!(full < Duration::from_secs(30));
    assert!(search < Duration::from_millis(50));
    assert!(rel < Duration::from_millis(5), "relation query");
    assert!(save < Duration::from_millis(200));
}

/// Spec §9: vault open (10k files) < 3 s to first paint, indexing continues
/// in the background. First paint = open + file tree listing.
#[test]
#[ignore]
fn perf_vault_open_10k_first_paint() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for i in 0..10_000 {
        write(root, &format!("notes/d{}/n{i}.md", i % 100), &format!("# N{i}\n[[N{}]]\n", (i + 1) % 10_000));
    }
    let t = Instant::now();
    let v = crate::vault::Vault::open(root, Arc::new(NullSink)).unwrap();
    let tree = v.tree().unwrap();
    let first_paint = t.elapsed();
    let json = serde_json::to_string(&tree).unwrap();
    let with_ipc = t.elapsed();
    assert!(tree.len() > 10_000);
    v.index.flush().unwrap();
    let indexed = t.elapsed();
    eprintln!("10k vault: open+tree {first_paint:?} | +serialize ({} KB) {with_ipc:?} | fully indexed {indexed:?}", json.len() / 1024);
    assert!(with_ipc < Duration::from_secs(3));
    v.shutdown();
}
