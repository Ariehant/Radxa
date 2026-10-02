//! JSON-RPC 2.0 over Tauri IPC. The frontend calls a single `rpc` command with
//! a JSON-RPC request; methods are looked up in a `Router` built from each
//! command module's `register` function — no central match statement.

use crate::error::NexusError;
use crate::state::AppState;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub id: Value,
}

#[derive(Debug, Serialize)]
pub struct ErrorObject {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorObject>,
    pub id: Value,
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Self {
        Response { jsonrpc: "2.0", result: Some(result), error: None, id }
    }
    pub fn err(id: Value, code: i64, message: String) -> Self {
        Response { jsonrpc: "2.0", result: None, error: Some(ErrorObject { code, message }), id }
    }
}

type Handler = Box<dyn Fn(&Arc<AppState>, Value) -> Result<Value, NexusError> + Send + Sync>;

#[derive(Default)]
pub struct Router {
    handlers: HashMap<&'static str, Handler>,
}

impl Router {
    /// Register a typed handler. Params are deserialised from the JSON-RPC
    /// `params` object; `null`/missing params deserialise as `{}`.
    pub fn add<P, R, F>(&mut self, method: &'static str, f: F)
    where
        P: DeserializeOwned + 'static,
        R: Serialize + 'static,
        F: Fn(&Arc<AppState>, P) -> Result<R, NexusError> + Send + Sync + 'static,
    {
        let prev = self.handlers.insert(
            method,
            Box::new(move |state, params| {
                let params = if params.is_null() { Value::Object(Default::default()) } else { params };
                let p: P = serde_json::from_value(params)
                    .map_err(|e| NexusError::invalid(format!("{method}: bad params: {e}")))?;
                let r = f(state, p)?;
                Ok(serde_json::to_value(r)?)
            }),
        );
        assert!(prev.is_none(), "duplicate rpc method {method}");
    }

    pub fn methods(&self) -> Vec<&'static str> {
        let mut m: Vec<_> = self.handlers.keys().copied().collect();
        m.sort_unstable();
        m
    }

    pub fn dispatch(&self, state: &Arc<AppState>, req: Request) -> Response {
        if req.jsonrpc != "2.0" {
            return Response::err(req.id, -32600, "jsonrpc must be \"2.0\"".into());
        }
        match self.handlers.get(req.method.as_str()) {
            None => Response::err(req.id, -32601, format!("method not found: {}", req.method)),
            Some(h) => match h(state, req.params) {
                Ok(v) => Response::ok(req.id, v),
                Err(e) => Response::err(req.id, e.code(), e.to_string()),
            },
        }
    }
}
