# Nexus — Knowledge + Workflow OS

Local-first desktop app (Tauri 2 · Rust · React 18 · TypeScript) that unifies
an Obsidian-style knowledge graph with a typed, executable workflow canvas.
All user data is **Markdown + JSON Canvas on disk**; SQLite is a derived,
disposable index. Running a flow produces knowledge: agent and write-note
nodes create `.md` files that immediately appear in backlinks and search,
and every run is logged to `runs/<flow>/<timestamp>.md`.

This directory implements **Phase 1** of the build spec (milestones 1–8).

## Quick start

Prerequisites: Rust ≥ 1.80, Node ≥ 20, and on Linux the WebKitGTK 4.1 stack:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev \
  libayatana-appindicator3-dev librsvg2-dev build-essential pkg-config
```

Windows 11 needs only the MSVC toolchain; WebView2 is fetched by the installer.

```bash
npm install
npm run tauri dev          # run the app
npm run tauri build        # deb + AppImage (Linux), msi + nsis (Windows)
```

Create a vault from the welcome screen: it scaffolds `notes/`, `flows/`,
`templates/nodes/`, a sample `deploy-pipeline` flow, and (optionally) `git init`.

For agent nodes, run [Ollama](https://ollama.com) locally (`ollama serve`,
`ollama pull llama3.2`) — no API key needed. Without a model, choose the
**Echo** provider in Settings to exercise flows offline.

## Tests

| Command | What |
|---|---|
| `cargo test --manifest-path src-tauri/Cargo.toml --lib` | 56 Rust unit/integration tests (index, parser, flows, engine, agent loop, fake-Ollama wire test, git, RPC) |
| `cargo test --release --manifest-path src-tauri/Cargo.toml --lib perf_ -- --ignored --nocapture` | 10k-file perf checks (spec §9) |
| `npm test` | Vitest: frontmatter + TipTap wikilink roundtrip |
| `npm run e2e` | Real Rust backend (`examples/dev_bridge.rs`) + real UI in headless Chromium, milestones 1–7 |
| `npm run e2e:prod` | Same against release backend + production bundle; perf targets asserted |

## Architecture

```
React UI ──(single Tauri command `rpc`, JSON-RPC 2.0)──► Router (commands/*)
                                                              │
   vault.rs ── fs/atomic.rs (tmp → fsync → rename)            │
   index/  ── worker thread (only writer, priority queue, xxh3 skip)
           ── watcher (notify, 400 ms debounce) · read-only pool (WAL)
           ── FTS5 (lazy) · bloom filter · 500-file hot cache
   flow/   ── model (flow.md + flow.canvas) · types · validate · engine
   agent/  ── LlmProvider (Ollama, Echo) · tools (read_note, query_graph, create_note)
   registry/ ─ NodeRegistry (src/nodes/*.rs, auto-discovered) · ProviderRegistry · ViewRegistry · plugin manifest
   git.rs  ── git2 auto-commit, coalesced
```

Three rules hold throughout: SQLite is never truth (delete `.nexus/index.db`,
reopen, everything returns); the UI never touches disk (every operation is an
RPC); views never store data (they are registered queries over the index).

Key files: [`src-tauri/src/index/worker.rs`](src-tauri/src/index/worker.rs),
[`src-tauri/src/flow/engine.rs`](src-tauri/src/flow/engine.rs),
[`src/views/FlowEditor/Canvas.tsx`](src/views/FlowEditor/Canvas.tsx),
[`docs/PLUGIN_API.md`](docs/PLUGIN_API.md).

### Extending

- **New node kind:** add one file `src-tauri/src/nodes/<kind>.rs` (see
  `text_template.rs`); `build.rs` registers it. Describe its ports in a vault
  template `templates/nodes/<name>.md` with `kind: <kind>`.
- **New LLM provider:** implement `agent::provider::LlmProvider` and add it to
  `ProviderRegistry::builtin()`.
- **New flow view:** `registerFlowView({...})` in `src/views/builtinViews.tsx`;
  back it with a query in `ViewRegistry::builtin()` if it needs data.

## Measured performance (release build, this repo's CI container)

| Operation | Target | Measured |
|---|---|---|
| Vault open (10k files) → first paint | < 3 s | ~55 ms (open + tree); full index continues in background, ~2 s |
| File save → UI update | < 16 ms | synchronous optimistic store update |
| File save → SQLite updated | < 200 ms | < 1 ms (`index_now`) |
| Search query | < 50 ms | ~11 ms over 10k notes |
| Relation (backlinks) query | < 5 ms | ~0.2 ms |
| View render (500 rows) | < 100 ms | ~40–55 ms query → paint |
| Flow canvas open (500 nodes) | < 200 ms | ~80–110 ms click → paint |
| Full vault reindex (10k files) | < 30 s | ~2 s (+0.4 s lazy FTS) |

Rollup and formula targets belong to Phase 1.5 (databases) and are not
implemented yet.

## Phase 1 scope notes

- Implemented: vault open/create, virtualised file tree, TipTap ⇄ CodeMirror
  editing with byte-for-byte wikilink roundtrip, backlinks, search, flow
  canvas with typed ports and blocked incompatible edges, Canvas / Table /
  Hybrid views, per-node and whole-flow execution with caching, Ollama agent
  with vault tools, run logs, git auto-commit, settings, registries, plugin
  manifest schema.
- Not in Phase 1 (per spec): database properties/relations/rollups/formulas
  and their views, WASM plugin host, IPC sockets, OS keychain, graph view.
- Testing gaps: the Tauri shell was smoke-launched under Xvfb on Ubuntu
  24.04 (the target is 26.04). Windows 11 is covered by the CI workflow but
  hasn't been run by hand. Ollama itself isn't in the test environment; its
  wire protocol is tested against a fake server.
