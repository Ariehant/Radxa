//! RPC surface. Each module contributes methods via `register`; the Tauri
//! layer exposes exactly one command, `rpc`, which dispatches JSON-RPC 2.0.

pub mod flows;
pub mod notes;
pub mod vault;

use crate::rpc::{Request, Response, Router};
use crate::state::AppState;
use std::sync::{Arc, OnceLock};

pub fn router() -> &'static Router {
    static ROUTER: OnceLock<Router> = OnceLock::new();
    ROUTER.get_or_init(|| {
        let mut r = Router::default();
        vault::register(&mut r);
        notes::register(&mut r);
        flows::register(&mut r);
        r.add("rpc.methods", |_s, _p: serde_json::Value| Ok(router_methods()));
        r
    })
}

fn router_methods() -> Vec<&'static str> {
    // Called lazily from within a handler, after init has completed.
    router().methods()
}

#[tauri::command]
pub async fn rpc(state: tauri::State<'_, Arc<AppState>>, request: serde_json::Value) -> Result<Response, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let id = request.get("id").cloned().unwrap_or(serde_json::Value::Null);
        match serde_json::from_value::<Request>(request) {
            Ok(req) => router().dispatch(&state, req),
            Err(e) => Response::err(id, -32600, format!("invalid request: {e}")),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::NullSink;
    use serde_json::{json, Value};

    fn call(state: &Arc<AppState>, method: &str, params: Value) -> Value {
        let req: Request = serde_json::from_value(json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).unwrap();
        let res = router().dispatch(state, req);
        if let Some(e) = res.error {
            panic!("{method}: {} ({})", e.message, e.code);
        }
        res.result.unwrap()
    }

    fn call_err(state: &Arc<AppState>, method: &str, params: Value) -> i64 {
        let req: Request = serde_json::from_value(json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})).unwrap();
        router().dispatch(state, req).error.expect("expected error").code
    }

    #[test]
    fn edit_save_reopen_is_identical_and_indexed() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::new(Arc::new(NullSink)));
        assert_eq!(call_err(&state, "vault.tree", json!({})), -32001);
        call(&state, "vault.create", json!({"path": dir.path().to_string_lossy()}));
        assert_eq!(call(&state, "git.status", json!({}))["enabled"], true);

        let original = call(&state, "note.read", json!({"path": "notes/welcome.md"}));
        let edited = format!("{}\nNow linking [[Brand New]].\r\n", original["content"].as_str().unwrap());
        let saved = call(&state, "note.write", json!({"path": "notes/welcome.md", "content": edited, "base_hash": original["hash"]}));
        let reopened = call(&state, "note.read", json!({"path": "notes/welcome.md"}));
        assert_eq!(reopened["content"].as_str().unwrap(), edited.replace("\r\n", "\n"));
        assert_eq!(reopened["hash"], saved["hash"]);

        // Stale base hash → conflict, not a silent overwrite.
        assert_eq!(call_err(&state, "note.write", json!({"path": "notes/welcome.md", "content": "x", "base_hash": original["hash"]})), -32009);

        let created = call(&state, "note.create", json!({"title": "Brand New", "body": "hello"}));
        assert_eq!(created["path"], "notes/brand-new.md");
        let bl = call(&state, "note.backlinks", json!({"path": "notes/brand-new.md"}));
        assert_eq!(bl[0]["path"], "notes/welcome.md");

        assert!(call(&state, "git.flush", json!({})).is_string());
        call(&state, "file.delete", json!({"path": "notes/brand-new.md"}));
        assert_eq!(call_err(&state, "note.read", json!({"path": "notes/brand-new.md"})), -32004);
        assert_eq!(call_err(&state, "note.read", json!({"path": "../etc/passwd"})), -32602);
        assert!(call(&state, "rpc.methods", json!({})).as_array().unwrap().len() > 10);
        state.set_vault(None);
    }
}
