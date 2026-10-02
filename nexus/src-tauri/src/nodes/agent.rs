//! Node kind `agent`.

use crate::error::Result;
use crate::flow::engine::{single, NodeCtx, Outputs};
use crate::registry::nodes::{NodeKind, NodeRegistry};
use crate::agent::tools::{self, Tool};
use crate::flow::engine::{render, render_value};
use serde_json::Value;

pub fn register(r: &mut NodeRegistry) {
    r.add(NodeKind {
        id: "agent",
        label: "Agent",
        description: "Run an LLM over the rendered template prompt, with vault tools.",
        cacheable: true,
        run,
    });
}

/// `agent`: render the template prompt with inputs, run the LLM with the
/// template's allowed tools.
fn run(ctx: &NodeCtx) -> Result<Outputs> {
    let cfg = ctx.vault.config();
    let model = ctx.cfg_str("model").or_else(|| ctx.template.meta.model.clone()).unwrap_or_else(|| cfg.llm.model.clone());
    let provider = crate::agent::provider_for(&cfg.llm)?;
    let mut prompt = render(&ctx.template.body, ctx);
    // Inputs the template didn't reference still reach the model.
    for (k, v) in ctx.inputs {
        if !ctx.template.body.contains(&format!("{{{{{k}}}}}")) {
            prompt.push_str(&format!("\n\n# Input `{k}`\n\n{}", render_value(v)));
        }
    }
    let system = ctx.cfg_str("system").unwrap_or_else(|| {
        "You are an agent inside Nexus, a Markdown knowledge base. Use the tools to read and query notes when helpful. \
         Reply with the requested output only."
            .into()
    });
    let all = tools::builtin();
    let allowed: Vec<&dyn Tool> = all
        .iter()
        .map(|t| t.as_ref())
        .filter(|t| ctx.template.meta.tools.iter().any(|n| *n == t.spec().name))
        .collect();
    ctx.note(format!("provider `{}`, model `{model}`, tools [{}]", provider.id(), allowed.iter().map(|t| t.spec().name).collect::<Vec<_>>().join(", ")));
    let out = crate::agent::run(provider.as_ref(), &model, &system, &prompt, &allowed, ctx.tools, cfg.llm.max_tool_steps)?;
    for t in &out.tools {
        ctx.note(format!("tool `{}` {} {}", t.name, t.arguments, if t.ok { "→ ok" } else { "→ error" }));
    }
    ctx.note(format!("{} step(s)", out.steps));
    let port = ctx.out_port();
    let ty = ctx.template.output(&port).map(|p| p.ty);
    let value = match ty {
        Some(crate::flow::types::PortType::Json) => serde_json::from_str(&out.text).unwrap_or(Value::String(out.text)),
        _ => Value::String(out.text),
    };
    Ok(single(port, value))
}
