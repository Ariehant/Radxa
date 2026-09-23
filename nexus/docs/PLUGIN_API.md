# Nexus Plugin API (specification — Phase 1.5)

> **Status:** Phase 1 ships the *manifest schema and validator* only
> (`src-tauri/src/registry/plugin.rs`). The WASM host, IPC bridge and
> credential vault described here are **not implemented yet**; this document is
> the contract they will be built against.

Plugins extend Nexus through the same registries the core uses:

| Registry | Core file | Plugin contributes |
|---|---|---|
| `NodeRegistry` | `registry/nodes.rs` | flow node kinds (`[contributes.nodes]`) |
| `ProviderRegistry` | `registry/providers.rs` | LLM providers (`[contributes.providers]`) |
| `ViewRegistry` | `registry/views.rs` + `src/views/registry.tsx` | view queries + renderers (`[contributes.views]`) |
| RPC `Router` | `rpc.rs` | namespaced methods `plugin.<id>.<method>` |

Built-in features are registered exactly like future plugins, so a plugin is
never a second-class citizen.

---

## 1. Layout

```
vault/.nexus/plugins/com.nexus.github/
  plugin.toml      # manifest (required)
  plugin.wasm      # WASI module (optional)
  ui.js            # UI bundle, ES module (optional)
  README.md
```

Plugins are per-vault. Distribution (Phase 2) is git-based: a plugin is a git
repository with a `plugin.toml` at its root and a signed release tag
(Phase 2.5).

## 2. Manifest — `plugin.toml`

The canonical example is [`examples/plugin.toml`](examples/plugin.toml); it is
parsed and validated in CI (`registry::plugin::tests`).

| Table | Key | Type | Required | Notes |
|---|---|---|---|---|
| `[plugin]` | `id` | string | ✓ | reverse-DNS, lowercase: `com.example.tool` |
| | `name` | string | ✓ | |
| | `version` | semver | ✓ | |
| | `author`, `description` | string | | |
| | `min_nexus_version` | semver | ✓ | host refuses to load older-incompatible plugins |
| `[entry]` | `wasm` | path | one of | WASI module, relative, must end in `.wasm` |
| | `ui` | path | one of | ES module rendered in a sandboxed iframe |
| | `sidecar` | path | one of | external process speaking JSON-RPC over stdin/stdout (Tier 7) |
| `[connection]` | `type` | `http` \| `websocket` \| `unix` \| `pipe` \| `tcp` \| `stdio` | | |
| | `endpoint` | URL | for http/ws | host must be in `permissions.network` (localhost exempt) |
| `[auth]` | `method` | `none` \| `api_key` \| `oauth2` \| `basic` | | secrets go to the OS keychain, never to files |
| | `scopes` | string[] | for oauth2 | |
| `[permissions]` | `fs_read`, `fs_write` | glob[] | | must start with `vault/`; `vault/.nexus/**` is always denied |
| | `network` | host[] | | bare host names; empty = no network |
| | `llm_call` | bool | | may call the configured LLM provider |
| | `git` | bool | | may commit via the host's git layer |
| | `max_memory_mb`, `max_cpu_ms`, `max_disk_mb` | int | | resource limits |
| `[socket]` | `path` / `pipe` / `tcp_port` | | | where the IPC bridge listens for this plugin |
| `[capabilities]` | `read`, `write`, `events` | string[] | | shown to the user at install |
| `[contributes.nodes]` | `<kind> = "<export>"` | map | | node kind id → exported WASM function |
| `[contributes.providers]` | `<id> = "<export>"` | map | | |
| `[contributes.views]` | `<id> = "<export>"` | map | | |

Unknown keys are rejected. `PluginManifest::validate()` reports **all**
problems at once.

## 3. Lifecycle (Rust guest API)

```rust
#[nexus_plugin]
trait Plugin {
    fn on_load(&mut self, ctx: &Context) -> Result<()>;
    fn on_vault_open(&mut self, ctx: &Context) -> Result<()>;
    fn on_file_change(&mut self, path: &Path) -> Result<()> { Ok(()) }
    fn on_node_execute(&mut self, node: &Node) -> Result<Output> {
        Err(Error::NotSupported)
    }
}
```

- `on_load` — once per process, before any vault. Register contributions.
- `on_vault_open` — each time a vault opens. `Context` exposes vault-relative
  path helpers and the granted capabilities.
- `on_file_change` — debounced (400 ms), vault-relative, forward-slash paths,
  only for paths matching `fs_read`.
- `on_node_execute` — called by the flow engine for a contributed node kind.
  `Node` carries `id`, `kind`, merged `config` (template defaults ⊕ instance),
  and `inputs` keyed by port name, already type-coerced
  (`document` → `document[]`). Return outputs keyed by port name.

Host functions available to the guest (all capability-checked):

| Function | Capability |
|---|---|
| `read_note(path) -> String` | `fs_read` |
| `write_note(path, content)` | `fs_write` (atomic, indexed, auto-committed like any save) |
| `query_graph({backlinks_of \| links_of \| search}) -> Json` | `fs_read` |
| `http(request) -> response` | `network` allowlist |
| `llm_chat(request) -> response` | `llm_call` |
| `git_commit(message) -> sha` | `git` |
| `log(level, message)` | always; written to `.nexus/audit/` |

## 4. Types

Ports use the core `PortType` set: `string`, `number`, `json`, `document`,
`document[]`, `note_ref`, `note_ref[]`, `card`, `any`. Values are JSON:

```jsonc
// document
{ "path": "notes/a.md", "title": "A", "content": "…", "frontmatter": {} }
// note_ref
{ "path": "notes/a.md", "title": "A" }
```

Compatibility: `any` ↔ everything; `document` → `document[]` auto-wraps;
otherwise exact match. Incompatible edges are rejected by the host before a
plugin ever sees them.

## 5. IPC bridge (external apps)

JSON-RPC 2.0, line-delimited JSON, over the transport in `[socket]`
(Unix socket on Linux, named pipe on Windows, TCP localhost fallback, or
stdin/stdout for sidecars). The in-app transport is the same protocol: the
Tauri frontend already talks to the core exclusively through one `rpc`
command carrying JSON-RPC 2.0 requests (`src-tauri/src/rpc.rs`), and
`examples/dev_bridge.rs` serves that router over localhost HTTP for tests.

```json
{"jsonrpc":"2.0","id":1,"method":"note.read","params":{"path":"notes/a.md"}}
{"jsonrpc":"2.0","id":1,"result":{"path":"notes/a.md","content":"…","hash":"…"}}
```

Error codes: `-32600` invalid request, `-32601` unknown method, `-32602`
invalid params / validation, `-32001` no vault, `-32004` not found,
`-32009` write conflict, `-32000` other.

## 6. Security model

1. Capability-based permissions in the manifest; the user approves them.
2. WASI sandbox — only declared `fs_*` paths are preopened; `.nexus/` never is.
3. No network by default — explicit host allowlist.
4. Signed plugins (Phase 2.5).
5. Kill switch — disabling a plugin unloads it immediately.
6. Resource limits — memory, CPU time (fuel), disk quota.
7. Credentials in the OS keychain (Windows Credential Manager / Secret
   Service). Until then, Phase 1 keeps the single LLM API key in
   `.nexus/credentials.toml` (0600, gitignored, never auto-committed).
8. Audit log — every external call is appended to `.nexus/audit/`.

## 7. Adding a built-in node kind (today)

One file, no core edit: create `src-tauri/src/nodes/<kind>.rs` with

```rust
use crate::error::Result;
use crate::flow::engine::{single, NodeCtx, Outputs};
use crate::registry::nodes::{NodeKind, NodeRegistry};

pub fn register(r: &mut NodeRegistry) {
    r.add(NodeKind { id: "<kind>", label: "…", description: "…", cacheable: true, run });
}

fn run(ctx: &NodeCtx) -> Result<Outputs> {
    Ok(single(ctx.out_port(), serde_json::json!("…")))
}
```

`build.rs` discovers every file in `src/nodes/` and generates the
registration list. Then describe its ports in a vault template
(`templates/nodes/<name>.md`, `kind: <kind>`). See
`src/nodes/text_template.rs` for a complete example.
