//! Read-side queries. They run on pooled read-only connections, so they never
//! wait on the writer (WAL).

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize, PartialEq)]
pub struct LinkRef {
    pub path: String,
    pub title: String,
    pub kind: String,
    /// Frontmatter property for relation edges, empty for body links.
    pub port: String,
}

#[derive(Debug, Serialize)]
pub struct OutLink {
    pub target: String,
    pub kind: String,
    pub port: String,
    pub resolved: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct NodeRow {
    pub id: String,
    pub path: String,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Serialize, Default)]
pub struct Stats {
    pub files: i64,
    pub nodes: i64,
    pub edges: i64,
}

pub fn backlinks(c: &Connection, rel: &str) -> rusqlite::Result<Vec<LinkRef>> {
    let mut stmt = c.prepare_cached(
        "SELECT DISTINCT f.path, COALESCE(json_extract(n.data, '$.title'), f.path), e.kind, COALESCE(e.port, '')
         FROM files tf
         JOIN nodes t ON t.file_id = tf.id
         JOIN aliases a ON a.node_id = t.id
         JOIN edges e ON e.to_id = a.key AND e.kind IN ('link', 'embed', 'relation')
         JOIN nodes n ON n.id = e.from_id
         JOIN files f ON f.id = n.file_id
         WHERE tf.path = ?1 AND f.path <> tf.path
         ORDER BY f.path",
    )?;
    let rows = stmt.query_map(params![rel], |r| {
        Ok(LinkRef { path: r.get(0)?, title: r.get(1)?, kind: r.get(2)?, port: r.get(3)? })
    })?;
    rows.collect()
}

pub fn outlinks(c: &Connection, rel: &str) -> rusqlite::Result<Vec<OutLink>> {
    let mut stmt = c.prepare_cached(
        "SELECT e.to_id, e.kind, COALESCE(e.port, ''),
                (SELECT tf.path FROM aliases a JOIN nodes tn ON tn.id = a.node_id JOIN files tf ON tf.id = tn.file_id
                 WHERE a.key = e.to_id ORDER BY length(tf.path) LIMIT 1)
         FROM files f JOIN nodes n ON n.file_id = f.id JOIN edges e ON e.from_id = n.id
         WHERE f.path = ?1 AND e.kind IN ('link', 'embed', 'relation')",
    )?;
    let rows = stmt.query_map(params![rel], |r| {
        Ok(OutLink { target: r.get(0)?, kind: r.get(1)?, port: r.get(2)?, resolved: r.get(3)? })
    })?;
    rows.collect()
}

/// Resolve a wikilink target to a vault path.
pub fn resolve(c: &Connection, target: &str) -> rusqlite::Result<Option<String>> {
    let key = super::parser::link_key(target);
    // Attachments aren't nodes: match by file path / file name.
    if let Some(p) = c
        .query_row(
            "SELECT tf.path FROM aliases a JOIN nodes n ON n.id = a.node_id JOIN files tf ON tf.id = n.file_id
             WHERE a.key = ?1 ORDER BY length(tf.path) LIMIT 1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(Some(p));
    }
    c.query_row(
        "SELECT path FROM files WHERE path = ?1 OR path LIKE '%/' || ?1 ORDER BY length(path) LIMIT 1",
        params![target.trim()],
        |r| r.get(0),
    )
    .optional()
}

/// Build a safe FTS5 query: each token quoted, last token prefix-matched.
pub fn fts_query(q: &str) -> Option<String> {
    let toks: Vec<String> = q
        .split_whitespace()
        .map(|t| t.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect::<String>())
        .filter(|t| !t.is_empty())
        .collect();
    if toks.is_empty() {
        return None;
    }
    let n = toks.len();
    Some(
        toks.iter()
            .enumerate()
            .map(|(i, t)| if i + 1 == n { format!("\"{t}\"*") } else { format!("\"{t}\"") })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

pub fn search(c: &Connection, q: &str, limit: usize) -> rusqlite::Result<Vec<SearchHit>> {
    let mut hits: Vec<SearchHit> = Vec::new();
    let like = format!("%{}%", q.trim().replace('%', "").replace('_', ""));
    // Title matches first (cheap, from the node index).
    {
        let mut stmt = c.prepare_cached(
            "SELECT f.path, json_extract(n.data, '$.title') FROM nodes n JOIN files f ON f.id = n.file_id
             WHERE json_extract(n.data, '$.title') LIKE ?1 LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![like, limit as i64], |r| {
            Ok(SearchHit { path: r.get(0)?, title: r.get::<_, Option<String>>(1)?.unwrap_or_default(), snippet: String::new() })
        })?;
        for h in rows {
            hits.push(h?);
        }
    }
    if let Some(fq) = fts_query(q) {
        let mut stmt = c.prepare_cached(
            "SELECT fts.path, COALESCE((SELECT json_extract(n.data, '$.title') FROM nodes n
                                        WHERE n.file_id = fts.rowid LIMIT 1), fts.path),
                    snippet(fts, 1, '«', '»', '…', 12)
             FROM fts WHERE fts MATCH ?1 ORDER BY rank LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![fq, limit as i64], |r| {
            Ok(SearchHit { path: r.get(0)?, title: r.get(1)?, snippet: r.get(2)? })
        })?;
        for h in rows {
            let h = h?;
            if let Some(existing) = hits.iter_mut().find(|e| e.path == h.path) {
                existing.snippet = h.snippet;
            } else {
                hits.push(h);
            }
        }
    }
    hits.truncate(limit);
    Ok(hits)
}

pub fn node_by_path(c: &Connection, rel: &str) -> rusqlite::Result<Option<NodeRow>> {
    c.query_row(
        "SELECT n.id, f.path, n.kind, n.data FROM files f JOIN nodes n ON n.file_id = f.id WHERE f.path = ?1 LIMIT 1",
        params![rel],
        node_row,
    )
    .optional()
}

pub fn nodes_by_kind(c: &Connection, kind: &str, limit: usize) -> rusqlite::Result<Vec<NodeRow>> {
    let mut stmt = c.prepare_cached(
        "SELECT n.id, f.path, n.kind, n.data FROM nodes n JOIN files f ON f.id = n.file_id WHERE n.kind = ?1 ORDER BY f.path LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![kind, limit as i64], node_row)?;
    rows.collect()
}

fn node_row(r: &rusqlite::Row) -> rusqlite::Result<NodeRow> {
    let data: String = r.get(3)?;
    Ok(NodeRow {
        id: r.get(0)?,
        path: r.get(1)?,
        kind: r.get(2)?,
        data: serde_json::from_str(&data).unwrap_or(Value::Null),
    })
}

pub fn exists(c: &Connection, target: &str) -> rusqlite::Result<bool> {
    Ok(resolve(c, target)?.is_some())
}

pub fn stats(c: &Connection) -> rusqlite::Result<Stats> {
    c.query_row(
        "SELECT (SELECT count(*) FROM files), (SELECT count(*) FROM nodes), (SELECT count(*) FROM edges)",
        [],
        |r| Ok(Stats { files: r.get(0)?, nodes: r.get(1)?, edges: r.get(2)? }),
    )
}

#[cfg(test)]
mod tests {
    use super::fts_query;

    #[test]
    fn fts_query_is_sanitised() {
        assert_eq!(fts_query("rust own").unwrap(), "\"rust\" \"own\"*");
        assert_eq!(fts_query("a\" OR 1=1 --").unwrap(), "\"a\" \"OR\" \"11\" \"--\"*");
        assert!(fts_query("  ").is_none());
    }
}
