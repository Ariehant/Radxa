//! Ollama provider (`POST /api/chat`, non-streaming, native tool calling).
//! No API key: talks to a local server, default http://127.0.0.1:11434.

use super::provider::{ChatMessage, ChatRequest, ChatResponse, LlmProvider, ToolCall};
use crate::config::LlmConfig;
use crate::error::{NexusError, Result};
use serde_json::{json, Value};
use std::time::Duration;

pub struct Ollama {
    base: String,
    agent: ureq::Agent,
}

impl Ollama {
    pub fn new(cfg: &LlmConfig) -> Ollama {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(cfg.timeout_secs.max(5)))
            .build();
        Ollama { base: cfg.base_url.trim_end_matches('/').to_owned(), agent }
    }

    fn err(&self, e: ureq::Error) -> NexusError {
        match e {
            ureq::Error::Status(code, r) => {
                let body = r.into_string().unwrap_or_default();
                let msg = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["error"].as_str().map(str::to_owned)).unwrap_or(body);
                NexusError::Other(format!("ollama: HTTP {code}: {msg}"))
            }
            ureq::Error::Transport(t) => NexusError::Other(format!(
                "ollama: cannot reach {} ({t}). Is `ollama serve` running?",
                self.base
            )),
        }
    }
}

pub fn factory(cfg: &LlmConfig) -> Box<dyn LlmProvider> {
    Box::new(Ollama::new(cfg))
}

/// Request body in Ollama's wire format.
pub fn request_body(req: &ChatRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            let mut o = json!({ "role": m.role, "content": m.content });
            if !m.tool_calls.is_empty() {
                o["tool_calls"] = m.tool_calls.iter().map(|c| json!({ "function": { "name": c.name, "arguments": c.arguments } })).collect();
            }
            if let Some(t) = &m.tool_name {
                o["tool_name"] = json!(t);
            }
            o
        })
        .collect();
    let mut body = json!({ "model": req.model, "messages": messages, "stream": false });
    if !req.tools.is_empty() {
        body["tools"] = req
            .tools
            .iter()
            .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.parameters } }))
            .collect();
    }
    if let Some(t) = req.temperature {
        body["options"] = json!({ "temperature": t });
    }
    body
}

pub fn parse_response(v: &Value) -> Result<ChatResponse> {
    let m = v.get("message").ok_or_else(|| NexusError::Other(format!("ollama: unexpected response: {v}")))?;
    let tool_calls = m["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .filter_map(|c| {
                    let f = &c["function"];
                    let name = f["name"].as_str()?.to_owned();
                    // Some models return arguments as a JSON string.
                    let arguments = match &f["arguments"] {
                        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
                        other => other.clone(),
                    };
                    Some(ToolCall { name, arguments })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ChatResponse {
        message: ChatMessage {
            role: m["role"].as_str().unwrap_or("assistant").to_owned(),
            content: m["content"].as_str().unwrap_or_default().to_owned(),
            tool_calls,
            tool_name: None,
        },
    })
}

impl LlmProvider for Ollama {
    fn id(&self) -> &str {
        "ollama"
    }

    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let resp = self.agent.post(&format!("{}/api/chat", self.base)).send_json(request_body(req)).map_err(|e| self.err(e))?;
        let v: Value = resp.into_json()?;
        parse_response(&v)
    }

    fn models(&self) -> Result<Vec<String>> {
        let resp = self.agent.get(&format!("{}/api/tags", self.base)).call().map_err(|e| self.err(e))?;
        let v: Value = resp.into_json()?;
        Ok(v["models"].as_array().map(|a| a.iter().filter_map(|m| m["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::provider::ToolSpec;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    /// A one-shot fake Ollama server capturing the request body.
    fn fake_server(reply: Value) -> (String, std::thread::JoinHandle<Value>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let h = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut len = 0;
            loop {
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            r.read_exact(&mut body).unwrap();
            let out = reply.to_string();
            let mut s = s;
            write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{out}", out.len()).unwrap();
            serde_json::from_slice(&body).unwrap()
        });
        (url, h)
    }

    #[test]
    fn wire_format_roundtrip_with_tool_calls() {
        let (url, h) = fake_server(json!({
            "model": "llama3.2",
            "message": { "role": "assistant", "content": "", "tool_calls": [
                { "function": { "name": "read_note", "arguments": { "path": "notes/a.md" } } },
                { "function": { "name": "query_graph", "arguments": "{\"search\":\"x\"}" } }
            ]},
            "done": true
        }));
        let cfg = LlmConfig { base_url: url, ..LlmConfig::default() };
        let p = Ollama::new(&cfg);
        let req = ChatRequest {
            model: "llama3.2".into(),
            messages: vec![ChatMessage::new("user", "hi"), ChatMessage::tool("read_note", "ok")],
            tools: vec![ToolSpec { name: "read_note".into(), description: "d".into(), parameters: json!({"type": "object"}) }],
            temperature: Some(0.2),
        };
        let r = p.chat(&req).unwrap();
        assert_eq!(r.message.tool_calls.len(), 2);
        assert_eq!(r.message.tool_calls[0].arguments["path"], "notes/a.md");
        assert_eq!(r.message.tool_calls[1].arguments["search"], "x", "string-encoded arguments are decoded");
        let sent = h.join().unwrap();
        assert_eq!(sent["stream"], false);
        assert_eq!(sent["tools"][0]["function"]["name"], "read_note");
        assert_eq!(sent["messages"][1]["tool_name"], "read_note");
        assert_eq!(sent["options"]["temperature"], 0.2f32 as f64);
    }

    #[test]
    fn unreachable_server_has_a_helpful_error() {
        let cfg = LlmConfig { base_url: "http://127.0.0.1:9".into(), timeout_secs: 5, ..LlmConfig::default() };
        let e = Ollama::new(&cfg).chat(&ChatRequest { model: "m".into(), messages: vec![], tools: vec![], temperature: None }).unwrap_err();
        assert!(e.to_string().contains("ollama serve"), "{e}");
    }
}
