//! Frontmatter + wikilink extraction. No Markdown AST is built: the indexer
//! only needs YAML frontmatter and `[[links]]`; the body goes to FTS5 lazily.

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Note,
    Database,
    View,
    Flow,
    Canvas,
    NodeTemplate,
    Run,
    Attachment,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Note => "note",
            FileKind::Database => "database",
            FileKind::View => "view",
            FileKind::Flow => "flow",
            FileKind::Canvas => "canvas",
            FileKind::NodeTemplate => "node-template",
            FileKind::Run => "run",
            FileKind::Attachment => "attachment",
        }
    }

    /// Classify by path alone (the frontmatter `type` can refine notes later).
    pub fn from_path(rel: &str) -> FileKind {
        let lower = rel.to_ascii_lowercase();
        if lower.ends_with(".canvas") {
            FileKind::Canvas
        } else if !lower.ends_with(".md") {
            FileKind::Attachment
        } else if lower.ends_with(".db.md") {
            FileKind::Database
        } else if lower.ends_with(".view.md") {
            FileKind::View
        } else if lower.starts_with("flows/") && lower.ends_with("/flow.md") {
            FileKind::Flow
        } else if lower.starts_with("templates/nodes/") {
            FileKind::NodeTemplate
        } else if lower.starts_with("runs/") {
            FileKind::Run
        } else {
            FileKind::Note
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Raw target as written, without `|alias` / `#heading`.
    pub target: String,
    pub embed: bool,
}

#[derive(Debug, Clone)]
pub struct Parsed {
    pub kind: FileKind,
    pub id: Option<String>,
    pub title: String,
    pub frontmatter: Map<String, Value>,
    pub frontmatter_error: Option<String>,
    pub links: Vec<Link>,
    /// Frontmatter property → wikilink targets (relations).
    pub relations: Vec<(String, String)>,
}

/// Split `---\n yaml \n---\n body`. Returns (yaml, body).
pub fn split_frontmatter(src: &str) -> (Option<&str>, &str) {
    let s = src.strip_prefix('\u{feff}').unwrap_or(src);
    let Some(rest) = s.strip_prefix("---\n").or_else(|| s.strip_prefix("---\r\n")) else {
        return (None, s);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            return (Some(yaml), body);
        }
        offset += line.len();
    }
    (None, s)
}

pub fn parse_frontmatter(yaml: &str) -> Result<Map<String, Value>, String> {
    if yaml.trim().is_empty() {
        return Ok(Map::new());
    }
    let v: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    match serde_json::to_value(v).map_err(|e| e.to_string())? {
        Value::Object(m) => Ok(m),
        Value::Null => Ok(Map::new()),
        _ => Err("frontmatter is not a mapping".into()),
    }
}

/// Extract `[[target]]`, `[[target|alias]]`, `[[target#h]]`, `![[embed]]`,
/// skipping fenced code blocks and inline code spans.
pub fn extract_links(body: &str) -> Vec<Link> {
    let mut out = Vec::new();
    let mut in_fence: Option<&str> = None;
    for line in body.lines() {
        let t = line.trim_start();
        if let Some(fence) = in_fence {
            if t.starts_with(fence) {
                in_fence = None;
            }
            continue;
        }
        if t.starts_with("```") {
            in_fence = Some("```");
            continue;
        }
        if t.starts_with("~~~") {
            in_fence = Some("~~~");
            continue;
        }
        scan_line(line, &mut out);
    }
    out
}

fn scan_line(line: &str, out: &mut Vec<Link>) {
    let b = line.as_bytes();
    let mut i = 0;
    let mut in_code = false;
    while i < b.len() {
        match b[i] {
            b'`' => {
                in_code = !in_code;
                i += 1;
            }
            b'[' if !in_code && b.get(i + 1) == Some(&b'[') => {
                let embed = i > 0 && b[i - 1] == b'!';
                let start = i + 2;
                match line[start..].find("]]") {
                    Some(len) => {
                        let inner = &line[start..start + len];
                        if !inner.contains("[[") {
                            if let Some(target) = link_target(inner) {
                                out.push(Link { target, embed });
                            }
                        }
                        i = start + len + 2;
                    }
                    None => return,
                }
            }
            _ => i += 1,
        }
    }
}

fn link_target(inner: &str) -> Option<String> {
    let t = inner.split('|').next()?.split('#').next()?.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_owned())
    }
}

/// `"[[Alpha Launch]]"` → `Some("Alpha Launch")`.
pub fn wikilink_value(s: &str) -> Option<String> {
    let t = s.trim();
    let inner = t.strip_prefix("[[")?.strip_suffix("]]")?;
    link_target(inner)
}

/// Canonical lookup key for a link target or note alias.
pub fn link_key(target: &str) -> String {
    let t = target.trim().replace('\\', "/");
    let t = t.strip_suffix(".md").unwrap_or(&t);
    t.trim_start_matches("./").to_lowercase()
}

/// All keys a note can be linked by: path without `.md`, file stem,
/// stem with `-`/`_` as spaces, title, and frontmatter `aliases`.
pub fn alias_keys(rel: &str, title: &str, fm: &Map<String, Value>) -> Vec<String> {
    let mut keys = Vec::new();
    let no_ext = rel.strip_suffix(".md").unwrap_or(rel);
    keys.push(link_key(no_ext));
    keys.push(link_key(rel));
    let stem = no_ext.rsplit('/').next().unwrap_or(no_ext);
    keys.push(link_key(stem));
    keys.push(link_key(&stem.replace(['-', '_'], " ")));
    if !title.is_empty() {
        keys.push(link_key(title));
    }
    if let Some(Value::Array(a)) = fm.get("aliases") {
        for v in a {
            if let Some(s) = v.as_str() {
                keys.push(link_key(s));
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

fn first_heading(body: &str) -> Option<&str> {
    body.lines().find_map(|l| l.strip_prefix("# ")).map(str::trim)
}

fn file_stem(rel: &str) -> &str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    for suffix in [".db.md", ".view.md", ".md", ".canvas"] {
        if let Some(s) = name.strip_suffix(suffix) {
            return s;
        }
    }
    name
}

pub fn parse(rel: &str, src: &str) -> Parsed {
    let path_kind = FileKind::from_path(rel);
    let (yaml, body) = split_frontmatter(src);
    let (frontmatter, frontmatter_error) = match yaml.map(parse_frontmatter) {
        None => (Map::new(), None),
        Some(Ok(m)) => (m, None),
        Some(Err(e)) => (Map::new(), Some(e)),
    };

    let kind = match (path_kind, frontmatter.get("type").and_then(Value::as_str)) {
        (FileKind::Note, Some("database")) => FileKind::Database,
        (FileKind::Note, Some("view")) => FileKind::View,
        (FileKind::Note, Some("flow")) => FileKind::Flow,
        (FileKind::Note, Some("node-template")) => FileKind::NodeTemplate,
        (k, _) => k,
    };

    let id = frontmatter.get("id").and_then(|v| match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });

    let title = frontmatter
        .get("title")
        .or_else(|| frontmatter.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| first_heading(body).map(str::to_owned))
        .unwrap_or_else(|| file_stem(rel).to_owned());

    let mut relations = Vec::new();
    for (k, v) in &frontmatter {
        match v {
            Value::String(s) => {
                if let Some(t) = wikilink_value(s) {
                    relations.push((k.clone(), t));
                }
            }
            Value::Array(items) => {
                for it in items {
                    if let Some(t) = it.as_str().and_then(wikilink_value) {
                        relations.push((k.clone(), t));
                    }
                }
            }
            _ => {}
        }
    }

    Parsed { kind, id, title, frontmatter, frontmatter_error, links: extract_links(body), relations }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\nid: 01HQX\ntype: task\ntitle: Fix Auth Bug\nassignee: \"[[Rahul]]\"\nblocked_by: [\"[[a]]\", \"[[b]]\"]\ndue: 2026-10-01\n---\n\nBody [[Rust Ownership]] and ![[diagram.png]] and [[x|alias]] [[y#h]].\n```\n[[not a link]]\n```\n`[[nor this]]` [[ok]]\n";

    #[test]
    fn parses_note() {
        let p = parse("notes/task-fix-auth.md", NOTE);
        assert_eq!(p.kind, FileKind::Note);
        assert_eq!(p.id.as_deref(), Some("01HQX"));
        assert_eq!(p.title, "Fix Auth Bug");
        assert_eq!(p.frontmatter["due"], "2026-10-01");
        let targets: Vec<_> = p.links.iter().map(|l| (l.target.as_str(), l.embed)).collect();
        assert_eq!(
            targets,
            vec![("Rust Ownership", false), ("diagram.png", true), ("x", false), ("y", false), ("ok", false)]
        );
        assert_eq!(
            p.relations,
            vec![
                ("assignee".to_string(), "Rahul".to_string()),
                ("blocked_by".to_string(), "a".to_string()),
                ("blocked_by".to_string(), "b".to_string())
            ]
        );
    }

    #[test]
    fn kinds() {
        assert_eq!(FileKind::from_path("databases/tasks.db.md"), FileKind::Database);
        assert_eq!(FileKind::from_path("views/a.view.md"), FileKind::View);
        assert_eq!(FileKind::from_path("flows/deploy/flow.md"), FileKind::Flow);
        assert_eq!(FileKind::from_path("flows/deploy/flow.canvas"), FileKind::Canvas);
        assert_eq!(FileKind::from_path("templates/nodes/x.md"), FileKind::NodeTemplate);
        assert_eq!(FileKind::from_path("attachments/a.png"), FileKind::Attachment);
    }

    #[test]
    fn no_frontmatter_title_from_heading() {
        let p = parse("notes/a.md", "# Hello\n\ntext");
        assert_eq!(p.title, "Hello");
        let p = parse("notes/rust-ownership.md", "text");
        assert_eq!(p.title, "rust-ownership");
        let keys = alias_keys("notes/rust-ownership.md", &p.title, &p.frontmatter);
        assert!(keys.contains(&"rust ownership".to_string()));
        assert!(keys.contains(&"notes/rust-ownership".to_string()));
    }

    #[test]
    fn bad_yaml_is_reported_not_fatal() {
        let p = parse("notes/a.md", "---\n: : :\n  - [\n---\nbody [[x]]");
        assert!(p.frontmatter_error.is_some());
        assert_eq!(p.links.len(), 1);
    }

    #[test]
    fn crlf_frontmatter() {
        let (y, b) = split_frontmatter("---\r\na: 1\r\n---\r\nbody");
        assert_eq!(y, Some("a: 1\r\n"));
        assert_eq!(b, "body");
    }
}
