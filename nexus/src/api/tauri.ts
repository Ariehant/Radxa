// The only bridge to the backend. The UI never touches disk (spec §3 rule 2):
// every operation is a JSON-RPC 2.0 call through the single `rpc` command.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  FlowSummary,
  FlowTable,
  FlowView,
  IndexStatus,
  LinkRef,
  NoteContent,
  OutLink,
  Position,
  Priority,
  Saved,
  SearchHit,
  TemplateSummary,
  TreeEntry,
  VaultInfo,
} from "./types";

export * from "./types";

interface RpcResponse<T> {
  jsonrpc: "2.0";
  id: number;
  result?: T;
  error?: { code: number; message: string };
}

export class RpcError extends Error {
  constructor(
    public code: number,
    message: string,
    public method: string,
  ) {
    super(message);
  }
}

let nextId = 1;

export async function call<T>(method: string, params: object = {}): Promise<T> {
  const id = nextId++;
  const res = await invoke<RpcResponse<T>>("rpc", {
    request: { jsonrpc: "2.0", id, method, params },
  });
  if (res.error) throw new RpcError(res.error.code, res.error.message, method);
  return res.result as T;
}

export function onEvent<T>(name: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(name, (e) => cb(e.payload));
}

export const api = {
  vault: {
    open: (path: string) => call<VaultInfo>("vault.open", { path }),
    create: (path: string, git = true) => call<VaultInfo>("vault.create", { path, git }),
    close: () => call<null>("vault.close"),
    info: () => call<VaultInfo | null>("vault.info"),
    tree: () => call<TreeEntry[]>("vault.tree"),
  },
  note: {
    read: (path: string) => call<NoteContent>("note.read", { path }),
    write: (path: string, content: string, base_hash?: string) => call<Saved>("note.write", { path, content, base_hash }),
    create: (title: string, body = "", dir = "notes") => call<Saved>("note.create", { title, body, dir }),
    backlinks: (path: string) => call<LinkRef[]>("note.backlinks", { path }),
    outlinks: (path: string) => call<OutLink[]>("note.outlinks", { path }),
  },
  link: {
    resolve: (targets: string[]) => call<(string | null)[]>("link.resolve", { targets }),
    exists: (targets: string[]) => call<boolean[]>("link.exists", { targets }),
  },
  file: {
    delete: (path: string) => call<null>("file.delete", { path }),
    rename: (from: string, to: string) => call<string>("file.rename", { from, to }),
  },
  search: (query: string, limit = 50) => call<SearchHit[]>("search.query", { query, limit }),
  flow: {
    list: () => call<FlowSummary[]>("flow.list"),
    load: (dir: string) => call<FlowView>("flow.load", { dir }),
    create: (name: string) => call<FlowView>("flow.create", { name }),
    saveLayout: (dir: string, positions: Position[]) => call<null>("flow.saveLayout", { dir, positions }),
    connect: (dir: string, from: string, to: string) => call<FlowView>("flow.connect", { dir, from, to }),
    disconnect: (dir: string, from: string, to: string) => call<FlowView>("flow.disconnect", { dir, from, to }),
    addNode: (dir: string, ref: string, x: number, y: number) => call<FlowView>("flow.addNode", { dir, ref, x, y }),
    removeNode: (dir: string, id: string) => call<FlowView>("flow.removeNode", { dir, id }),
    updateNode: (dir: string, id: string, patch: { config?: Record<string, unknown>; title?: string }) =>
      call<FlowView>("flow.updateNode", { dir, id, ...patch }),
  },
  templates: () => call<TemplateSummary[]>("template.list"),
  view: {
    flowTable: (dir: string) => call<FlowTable>("view.flowTable", { dir }),
  },
  index: {
    status: () => call<IndexStatus>("index.status"),
    rebuild: () => call<null>("index.rebuild"),
    prioritize: (paths: string[], priority: Priority) => call<null>("index.prioritize", { paths, priority }),
  },
};
