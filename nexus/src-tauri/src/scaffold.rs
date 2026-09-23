//! Default content written into a freshly created vault. Never overwrites.

use crate::error::Result;
use crate::fs::atomic_write;
use std::path::Path;

const DEFAULTS: &[(&str, &str)] = &[
    (".gitignore", ".nexus/index.db*\n.nexus/*.tmp\n"),
    (".gitattributes", "* text=auto eol=lf\n"),
    (
        "notes/welcome.md",
        "---\ntype: note\ntitle: Welcome to Nexus\ntags: [nexus]\n---\n\n# Welcome to Nexus\n\nEverything in this vault is plain Markdown and JSON Canvas.\nLink notes with wikilinks like [[Getting Started]].\n\nOpen `flows/deploy-pipeline/flow.md` to see a workflow whose agent writes\nits results back into this knowledge base.\n",
    ),
    (
        "notes/getting-started.md",
        "---\ntype: note\ntitle: Getting Started\n---\n\n# Getting Started\n\nBack to [[Welcome to Nexus]].\n\n- Toggle **Rich / Source** in the editor header.\n- Ctrl/Cmd-click a wikilink in source mode to follow it.\n- Every save is atomic and, if the vault is a git repo, auto-committed.\n",
    ),
    (
        "notes/release-notes.md",
        "---\ntype: note\ntitle: Release Notes\n---\n\n# Release Notes\n\n## 0.1.0\n\n- Vault open/create, file tree, wikilinks and backlinks.\n- Rich (TipTap) and source (CodeMirror) editing.\n- Flow canvas with typed ports; agent nodes run on Ollama.\n",
    ),
    (
        "templates/nodes/fetch-data.md",
        "---\nkind: fetch\ntitle: Fetch Data\ndescription: Load notes as documents. `source` is a vault path or glob.\ninputs: []\noutputs:\n  - { name: out, type: \"document[]\" }\nconfig:\n  source: \"notes/**/*.md\"\n---\n\nReads matching notes and emits them as a document list.\n",
    ),
    (
        "templates/nodes/llm-summarize.md",
        "---\nkind: agent\ntitle: LLM Summarize\ndescription: Summarise documents with the configured LLM provider.\nmodel: llama3.2\ninputs:\n  - { name: in, type: \"document[]\" }\noutputs:\n  - { name: out, type: string }\ntools: [read_note, query_graph]\n---\n\nYou are summarizing documents. Input: {{in}}\n\nReturn a 3-paragraph summary.\n",
    ),
    (
        "templates/nodes/write-note.md",
        "---\nkind: write_note\ntitle: Write Note\ndescription: Write the input text into a new note in the knowledge base.\ninputs:\n  - { name: content, type: string }\noutputs:\n  - { name: note, type: note_ref }\nconfig:\n  dir: notes/generated\n  title: \"{{flow}} output\"\n---\n\nCreates a note; the note shows up in the graph and backlinks immediately.\n",
    ),
    (
        "templates/nodes/git-commit.md",
        "---\nkind: git_commit\ntitle: Git Commit\ndescription: Write content to a file and commit it (requires a git vault).\ninputs:\n  - { name: content, type: string }\noutputs:\n  - { name: sha, type: string }\nconfig:\n  path: \"notes/generated/{{flow}}.md\"\n  message: \"nexus: flow output\"\n---\n",
    ),
    (
        "templates/nodes/query-graph.md",
        "---\nkind: query_graph\ntitle: Query Graph\ndescription: Notes linking to `target` (backlinks) or matching `search`.\ninputs: []\noutputs:\n  - { name: out, type: \"note_ref[]\" }\nconfig:\n  target: Welcome to Nexus\n---\n",
    ),
    (
        "flows/deploy-pipeline/flow.md",
        "---\ntype: flow\nname: deploy-pipeline\nentry: n1\nnodes:\n  - id: n1\n    ref: templates/nodes/fetch-data.md\n    config: { source: notes/release-notes.md }\n  - id: n2\n    ref: templates/nodes/llm-summarize.md\n    config: { model: llama3.2 }\n  - id: n3\n    ref: templates/nodes/write-note.md\n    config: { title: Release Summary, dir: notes/generated }\nedges:\n  - { from: n1.out, to: n2.in, type: \"document[]\" }\n  - { from: n2.out, to: n3.content, type: string }\n---\n\n# deploy-pipeline\n\nFetch the release notes, summarise them with a local LLM and write the\nsummary back into the vault as a new note.\n",
    ),
    (
        "flows/deploy-pipeline/flow.canvas",
        "{\n  \"nodes\": [\n    { \"id\": \"n1\", \"type\": \"text\", \"x\": 100, \"y\": 100, \"width\": 240, \"height\": 120, \"text\": \"\" },\n    { \"id\": \"n2\", \"type\": \"text\", \"x\": 480, \"y\": 100, \"width\": 240, \"height\": 120, \"text\": \"\" },\n    { \"id\": \"n3\", \"type\": \"text\", \"x\": 860, \"y\": 100, \"width\": 240, \"height\": 120, \"text\": \"\" }\n  ],\n  \"edges\": []\n}\n",
    ),
];

pub fn write_defaults(root: &Path) -> Result<()> {
    for (rel, body) in DEFAULTS {
        let p = crate::fs::resolve(root, rel)?;
        if !p.exists() {
            atomic_write(&p, body.as_bytes())?;
        }
    }
    Ok(())
}
