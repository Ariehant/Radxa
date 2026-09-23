//! Node kind `text_template`: render `config.template` with the inputs.
//! (Also the worked example for "adding a node type = 1 file".)

use crate::error::{NexusError, Result};
use crate::flow::engine::{render, single, NodeCtx, Outputs};
use crate::registry::nodes::{NodeKind, NodeRegistry};
use serde_json::Value;

pub fn register(r: &mut NodeRegistry) {
    r.add(NodeKind {
        id: "text_template",
        label: "Text Template",
        description: "Render `config.template` with {{input}} placeholders into a string.",
        cacheable: true,
        run,
    });
}

fn run(ctx: &NodeCtx) -> Result<Outputs> {
    let tpl = ctx
        .config
        .get("template")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| (!ctx.template.body.is_empty()).then(|| ctx.template.body.clone()))
        .ok_or_else(|| NexusError::invalid("text_template: set config.template"))?;
    Ok(single(ctx.out_port(), Value::String(render(&tpl, ctx))))
}
