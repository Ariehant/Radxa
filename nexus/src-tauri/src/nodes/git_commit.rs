//! Node kind `git_commit`.

use crate::error::{NexusError, Result};
use crate::flow::engine::{single, NodeCtx, Outputs};
use crate::registry::nodes::{NodeKind, NodeRegistry};
use crate::flow::engine::render_value;
use serde_json::Value;

pub fn register(r: &mut NodeRegistry) {
    r.add(NodeKind {
        id: "git_commit",
        label: "Git Commit",
        description: "Write `content` to `config.path` and commit it.",
        cacheable: true,
        run,
    });
}

/// `git_commit`: write input `content` to `config.path`, then commit now.
fn run(ctx: &NodeCtx) -> Result<Outputs> {
    let git = ctx.vault.git.as_ref().ok_or_else(|| NexusError::invalid("git_commit: the vault is not a git repository"))?;
    let content = ctx.inputs.get("content").map(render_value).ok_or_else(|| NexusError::invalid("git_commit: input `content` is not connected"))?;
    let path = ctx.cfg_str("path").unwrap_or_else(|| format!("notes/generated/{}.md", crate::vault::slugify(&ctx.loaded.flow.meta.name)));
    ctx.vault.write_text(&path, &content, None)?;
    let sha = git.flush()?.unwrap_or_default();
    ctx.note(format!("committed {path} {}", if sha.is_empty() { "(no changes)".to_owned() } else { sha[..8.min(sha.len())].to_owned() }));
    Ok(single(ctx.out_port(), Value::String(sha)))
}
