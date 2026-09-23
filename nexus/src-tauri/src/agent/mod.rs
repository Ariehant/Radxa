//! Agent loop: chat with an LLM, executing vault tool calls until the model
//! answers (or the step budget runs out).

pub mod ollama;
pub mod provider;
pub mod tools;

use crate::error::Result;
use provider::{ChatMessage, ChatRequest, LlmProvider};
use serde::Serialize;
use serde_json::Value;
use tools::{Tool, ToolCtx};

#[derive(Debug, Clone, Serialize)]
pub struct ToolTrace {
    pub name: String,
    pub arguments: Value,
    pub ok: bool,
    /// Result (or error) as sent back to the model, truncated for logs.
    pub result: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentOutcome {
    pub text: String,
    pub steps: usize,
    pub tools: Vec<ToolTrace>,
}

const TOOL_RESULT_LOG_CHARS: usize = 600;

/// Build the configured provider.
pub fn provider_for(cfg: &crate::config::LlmConfig) -> Result<Box<dyn LlmProvider>> {
    match cfg.provider.as_str() {
        "ollama" => Ok(ollama::factory(cfg)),
        "echo" => Ok(provider::echo_factory(cfg)),
        other => Err(crate::error::NexusError::invalid(format!("unknown LLM provider `{other}`"))),
    }
}

pub fn run(
    provider: &dyn LlmProvider,
    model: &str,
    system: &str,
    prompt: &str,
    tools: &[&dyn Tool],
    ctx: &ToolCtx,
    max_steps: usize,
) -> Result<AgentOutcome> {
    let mut messages = Vec::new();
    if !system.is_empty() {
        messages.push(ChatMessage::new("system", system));
    }
    messages.push(ChatMessage::new("user", prompt));
    let specs = tools.iter().map(|t| t.spec()).collect::<Vec<_>>();
    let mut traces = Vec::new();

    for step in 1..=max_steps.max(1) {
        // On the last step, withhold tools so the model must answer.
        let offer_tools = step < max_steps.max(1);
        let req = ChatRequest {
            model: model.to_owned(),
            messages: messages.clone(),
            tools: if offer_tools { specs.clone() } else { vec![] },
            temperature: None,
        };
        let reply = provider.chat(&req)?.message;
        if reply.tool_calls.is_empty() || !offer_tools {
            return Ok(AgentOutcome { text: reply.content.trim().to_owned(), steps: step, tools: traces });
        }
        let calls = reply.tool_calls.clone();
        messages.push(reply);
        for call in calls {
            let (ok, result) = match tools.iter().find(|t| t.spec().name == call.name) {
                None => (false, format!("error: unknown tool `{}`", call.name)),
                Some(t) => match t.call(ctx, &call.arguments) {
                    Ok(v) => (true, v.to_string()),
                    Err(e) => (false, format!("error: {e}")),
                },
            };
            traces.push(ToolTrace {
                name: call.name.clone(),
                arguments: call.arguments.clone(),
                ok,
                result: result.chars().take(TOOL_RESULT_LOG_CHARS).collect(),
            });
            messages.push(ChatMessage::tool(&call.name, result));
        }
    }
    unreachable!("loop always returns on the final step")
}

#[cfg(test)]
mod tests {
    use super::provider::{ChatResponse, ToolCall};
    use super::*;
    use crate::state::NullSink;
    use parking_lot::Mutex;
    use serde_json::json;
    use std::sync::Arc;

    /// Scripted provider: returns queued replies and records requests.
    pub struct Scripted {
        pub replies: Mutex<Vec<ChatMessage>>,
        pub seen: Mutex<Vec<ChatRequest>>,
    }
    impl LlmProvider for Scripted {
        fn id(&self) -> &str {
            "scripted"
        }
        fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
            self.seen.lock().push(req.clone());
            let mut r = self.replies.lock();
            let m = if r.is_empty() { ChatMessage::new("assistant", "done") } else { r.remove(0) };
            Ok(ChatResponse { message: m })
        }
    }

    fn call(name: &str, args: Value) -> ChatMessage {
        ChatMessage { role: "assistant".into(), content: String::new(), tool_calls: vec![ToolCall { name: name.into(), arguments: args }], tool_name: None }
    }

    #[test]
    fn agent_uses_vault_tools_and_creates_knowledge() {
        let d = tempfile::tempdir().unwrap();
        let v = crate::vault::Vault::create(d.path(), Arc::new(NullSink), false).unwrap();
        v.index.flush().unwrap();
        let p = Scripted {
            replies: Mutex::new(vec![
                call("read_note", json!({ "title": "Release Notes" })),
                call("query_graph", json!({ "backlinks_of": "Welcome to Nexus" })),
                call("create_note", json!({ "title": "Release Digest", "body": "See [[Release Notes]]." })),
                call("nope", json!({})),
                ChatMessage::new("assistant", "  All done.  "),
            ]),
            seen: Mutex::new(vec![]),
        };
        let ctx = ToolCtx { vault: &v, flow: "flows/x/flow", node: "n2", run: "r1", created: Mutex::new(vec![]) };
        let all = tools::builtin();
        let refs: Vec<&dyn Tool> = all.iter().map(|b| b.as_ref()).collect();
        let out = run(&p, "m", "sys", "go", &refs, &ctx, 8).unwrap();
        assert_eq!(out.text, "All done.");
        assert_eq!(out.steps, 5);
        assert_eq!(out.tools.iter().map(|t| (t.name.as_str(), t.ok)).collect::<Vec<_>>(), vec![
            ("read_note", true),
            ("query_graph", true),
            ("create_note", true),
            ("nope", false)
        ]);
        assert!(out.tools[0].result.contains("0.1.0"));
        assert!(out.tools[1].result.contains("notes/getting-started.md"));
        // The created note is real, indexed knowledge with provenance.
        let created = ctx.created.lock().clone();
        assert_eq!(created, vec!["notes/generated/release-digest.md"]);
        let c = v.index.read().unwrap();
        let bl = crate::index::query::backlinks(&c, "notes/release-notes.md").unwrap();
        assert!(bl.iter().any(|b| b.path == "notes/generated/release-digest.md"));
        let meta = crate::index::query::node_by_path(&c, "notes/generated/release-digest.md").unwrap().unwrap();
        assert_eq!(meta.data["frontmatter"]["generated_by"], "[[flows/x/flow]]");
        // Tool results were fed back to the model with the tool's name.
        let seen = p.seen.lock();
        assert_eq!(seen[1].messages.last().unwrap().tool_name.as_deref(), Some("read_note"));
        assert_eq!(seen[0].tools.len(), 3);
    }

    #[test]
    fn last_step_withholds_tools() {
        let d = tempfile::tempdir().unwrap();
        let v = crate::vault::Vault::create(d.path(), Arc::new(NullSink), false).unwrap();
        let p = Scripted { replies: Mutex::new((0..10).map(|_| call("read_note", json!({"path": "notes/welcome.md"}))).collect()), seen: Mutex::new(vec![]) };
        let ctx = ToolCtx { vault: &v, flow: "f", node: "n", run: "r", created: Mutex::new(vec![]) };
        let all = tools::builtin();
        let refs: Vec<&dyn Tool> = all.iter().map(|b| b.as_ref()).collect();
        let out = run(&p, "m", "", "go", &refs, &ctx, 3).unwrap();
        assert_eq!(out.steps, 3);
        assert!(p.seen.lock()[2].tools.is_empty());
    }

    #[test]
    fn agents_cannot_write_outside_notes() {
        let d = tempfile::tempdir().unwrap();
        let v = crate::vault::Vault::create(d.path(), Arc::new(NullSink), false).unwrap();
        let ctx = ToolCtx { vault: &v, flow: "f", node: "n", run: "r", created: Mutex::new(vec![]) };
        assert!(tools::CreateNote.call(&ctx, &json!({"title": "x", "body": "y", "dir": "templates/nodes"})).is_err());
        assert!(tools::CreateNote.call(&ctx, &json!({"title": "x", "body": "y", "dir": "../../etc"})).is_err());
    }
}
