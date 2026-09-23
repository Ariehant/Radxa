//! RPC surface. Each module contributes methods via `register`; the Tauri
//! layer exposes exactly one command, `rpc`, which dispatches JSON-RPC 2.0.

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
