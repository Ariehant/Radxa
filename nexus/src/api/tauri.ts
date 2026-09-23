// The only bridge to the backend. The UI never touches disk (spec §3 rule 2):
// every operation is a JSON-RPC 2.0 call through the single `rpc` command.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { NoteContent, TreeEntry, VaultInfo } from "./types";

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
    create: (path: string) => call<VaultInfo>("vault.create", { path }),
    close: () => call<null>("vault.close"),
    info: () => call<VaultInfo | null>("vault.info"),
    tree: () => call<TreeEntry[]>("vault.tree"),
  },
  note: {
    read: (path: string) => call<NoteContent>("note.read", { path }),
  },
};
