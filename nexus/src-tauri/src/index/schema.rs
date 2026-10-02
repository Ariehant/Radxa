//! SQLite schema (spec §4.8) + migrations via `PRAGMA user_version`.

use rusqlite::Connection;

pub const SCHEMA_VERSION: i64 = 1;

const PRAGMAS: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 268435456;
PRAGMA foreign_keys = ON;
PRAGMA temp_store = MEMORY;
PRAGMA busy_timeout = 5000;
";

const V1: &str = "
CREATE TABLE files (
  id INTEGER PRIMARY KEY,
  path TEXT UNIQUE NOT NULL COLLATE NOCASE,
  hash BLOB NOT NULL,
  mtime INTEGER NOT NULL,
  kind TEXT NOT NULL,
  indexed_at INTEGER NOT NULL
);
CREATE INDEX idx_files_mtime ON files(mtime DESC);
CREATE INDEX idx_files_kind ON files(kind);

CREATE TABLE nodes (
  id TEXT PRIMARY KEY,
  file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  data JSON NOT NULL
);
CREATE INDEX idx_nodes_file ON nodes(file_id);
CREATE INDEX idx_nodes_kind ON nodes(kind);

CREATE TABLE edges (
  from_id TEXT NOT NULL,
  to_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  port TEXT,
  data_type TEXT,
  PRIMARY KEY (from_id, to_id, kind, port)
);
CREATE INDEX idx_edges_from ON edges(from_id, kind);
CREATE INDEX idx_edges_to ON edges(to_id, kind);

CREATE VIRTUAL TABLE fts USING fts5(
  path UNINDEXED, body, tokenize='porter unicode61'
);

CREATE TABLE canvas_layout (
  flow_id TEXT NOT NULL,
  node_id TEXT NOT NULL,
  x REAL, y REAL, w REAL, h REAL,
  PRIMARY KEY (flow_id, node_id)
);

CREATE TABLE rollup_cache (
  node_id TEXT NOT NULL,
  name TEXT NOT NULL,
  value JSON,
  computed_at INTEGER NOT NULL,
  PRIMARY KEY (node_id, name)
);

-- Link resolution: every key a node can be referenced by via [[...]].
-- Edges of kind link/embed/relation store the normalised target key in to_id.
CREATE TABLE aliases (
  key TEXT NOT NULL COLLATE NOCASE,
  node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
  PRIMARY KEY (key, node_id)
);
CREATE INDEX idx_aliases_node ON aliases(node_id);
";

pub fn apply_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(PRAGMAS)
}

pub fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    apply_pragmas(conn)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        // Index from a newer Nexus: it's disposable, so start over.
        drop_all(conn)?;
    } else if version == SCHEMA_VERSION {
        return Ok(());
    }
    let tx = conn.transaction()?;
    if version < 1 {
        tx.execute_batch(V1)?;
    }
    tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
    tx.commit()
}

fn drop_all(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DROP TABLE IF EXISTS aliases; DROP TABLE IF EXISTS rollup_cache; DROP TABLE IF EXISTS canvas_layout;
         DROP TABLE IF EXISTS fts; DROP TABLE IF EXISTS edges; DROP TABLE IF EXISTS nodes; DROP TABLE IF EXISTS files;
         PRAGMA user_version = 0;",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_in_memory_and_is_idempotent() {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        migrate(&mut c).unwrap();
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        c.execute("INSERT INTO fts(path, body) VALUES ('a', 'running tests')", []).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM fts WHERE fts MATCH 'run'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "porter stemming via FTS5");
    }
}
