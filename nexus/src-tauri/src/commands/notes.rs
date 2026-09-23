use crate::rpc::Router;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct PathParams {
    path: String,
}

#[derive(Serialize)]
struct NoteContent {
    path: String,
    content: String,
}

pub fn register(r: &mut Router) {
    r.add("note.read", |s, p: PathParams| {
        let v = s.vault()?;
        let content = v.read_text(&p.path)?;
        Ok(NoteContent { path: crate::fs::normalize_rel(&p.path), content })
    });
}
