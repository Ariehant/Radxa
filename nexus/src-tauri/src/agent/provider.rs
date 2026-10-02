//! LLM provider abstraction. Providers are synchronous (they run on the
//! blocking RPC pool) and speak a minimal chat + tool-calling protocol.

use crate::config::LlmConfig;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessage {
    /// `system` | `user` | `assistant` | `tool`
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// For `tool` messages: which tool produced this result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        ChatMessage { role: role.into(), content: content.into(), tool_calls: vec![], tool_name: None }
    }
    pub fn tool(name: &str, content: impl Into<String>) -> Self {
        ChatMessage { role: "tool".into(), content: content.into(), tool_calls: vec![], tool_name: Some(name.into()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema for the arguments object.
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: ChatMessage,
}

pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse>;
    fn models(&self) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

/// Factory registered in the provider registry.
pub type ProviderFactory = fn(&LlmConfig) -> Box<dyn LlmProvider>;

/// Offline provider: echoes a digest of the prompt. Useful for trying flows
/// without a model and for deterministic tests. Never calls tools.
pub struct EchoProvider;

impl LlmProvider for EchoProvider {
    fn id(&self) -> &str {
        "echo"
    }

    fn chat(&self, req: &ChatRequest) -> Result<ChatResponse> {
        let last = req.messages.iter().rev().find(|m| m.role == "user").map(|m| m.content.as_str()).unwrap_or("");
        let words = last.split_whitespace().count();
        let preview: String = last.chars().take(280).collect();
        let text = format!("[echo:{}] {words} words received.\n\n{preview}{}", req.model, if last.chars().count() > 280 { "…" } else { "" });
        Ok(ChatResponse { message: ChatMessage::new("assistant", text) })
    }

    fn models(&self) -> Result<Vec<String>> {
        Ok(vec!["echo".into()])
    }
}

pub fn echo_factory(_: &LlmConfig) -> Box<dyn LlmProvider> {
    Box::new(EchoProvider)
}
