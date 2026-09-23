//! Default content written into a freshly created vault. Never overwrites.

use crate::error::Result;
use crate::fs::atomic_write;
use std::path::Path;

const DEFAULTS: &[(&str, &str)] = &[
    (".gitignore", ".nexus/index.db*\n"),
    (".gitattributes", "* text=auto eol=lf\n"),
    (
        "notes/welcome.md",
        "---\ntype: note\ntitle: Welcome to Nexus\ntags: [nexus]\n---\n\n# Welcome to Nexus\n\nEverything in this vault is plain Markdown and JSON Canvas.\nLink notes with wikilinks like [[Getting Started]].\n",
    ),
    (
        "notes/getting-started.md",
        "---\ntype: note\ntitle: Getting Started\n---\n\n# Getting Started\n\nBack to [[Welcome to Nexus]].\n",
    ),
];

pub fn write_defaults(root: &Path) -> Result<()> {
    for (rel, body) in DEFAULTS {
        let p = root.join(rel);
        if !p.exists() {
            atomic_write(&p, body.as_bytes())?;
        }
    }
    Ok(())
}
