import { create } from "zustand";
import { api, type EdgeCheck, type FlowView, type NodeView, type RunProgress, type RunResult, type RunStatus, type TemplateSummary } from "../api/tauri";
import { compatible, splitRef } from "../views/FlowEditor/ports";

/** Id of a view registered in src/views/registry.tsx. */
export type FlowViewMode = string;

/** World-space centre of the visible canvas; kept current by <Canvas> without re-rendering. */
export const canvasCenter = { x: 400, y: 240 };

interface FlowState {
  dir: string | null;
  flow: FlowView | null;
  templates: TemplateSummary[];
  /** Node kinds with a registered executor (backend NodeRegistry). */
  kinds: string[];
  loading: boolean;
  error: string | null;
  mode: FlowViewMode;
  selectedNode: string | null;
  selectedEdge: string | null; // "from->to"
  /** True while the user drags a node; external reloads wait. */
  dragging: boolean;
  running: boolean;
  runStatus: Record<string, { status: RunStatus; ms?: number; error?: string | null }>;
  lastRun: RunResult | null;

  open: (dir: string) => Promise<void>;
  reload: () => Promise<void>;
  setMode: (m: FlowViewMode) => void;
  select: (node: string | null, edge?: string | null) => void;
  moveNode: (id: string, x: number, y: number) => void;
  commitLayout: (ids: string[]) => Promise<void>;
  connect: (from: string, to: string) => Promise<boolean>;
  disconnect: (from: string, to: string) => Promise<void>;
  addNode: (ref: string, x: number, y: number) => Promise<void>;
  removeNode: (id: string) => Promise<void>;
  updateNode: (id: string, patch: { config?: Record<string, unknown>; title?: string }) => Promise<void>;
  setError: (e: string | null) => void;
  close: () => void;
  run: (node?: string) => Promise<RunResult | null>;
  onRunProgress: (e: RunProgress) => void;
}

function loadMode(): FlowViewMode {
  try {
    return localStorage.getItem("nexus.flowMode") || "canvas";
  } catch {
    return "canvas";
  }
}

export const useFlowStore = create<FlowState>((set, get) => ({
  dir: null,
  flow: null,
  templates: [],
  kinds: [],
  loading: false,
  error: null,
  mode: loadMode(),
  selectedNode: null,
  selectedEdge: null,
  dragging: false,
  running: false,
  runStatus: {},
  lastRun: null,

  open: async (dir) => {
    const same = get().dir === dir;
    set({ dir, loading: true, flow: same ? get().flow : null, selectedNode: null, selectedEdge: null, error: null, ...(same ? {} : { runStatus: {}, lastRun: null }) });
    try {
      const [flow, templates, lastRun, kinds] = await Promise.all([
        api.flow.load(dir),
        api.templates(),
        api.exec.last(dir),
        get().kinds.length ? Promise.resolve(null) : api.nodeKinds().then((k) => k.map((x) => x.id)),
      ]);
      if (kinds) set({ kinds });
      if (get().dir === dir) {
        const runStatus = Object.fromEntries((lastRun?.nodes ?? []).map((n) => [n.node, { status: n.status, ms: n.ms, error: n.error }]));
        set({ flow, templates, lastRun, runStatus });
      }
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ loading: false });
    }
  },

  reload: async () => {
    const dir = get().dir;
    if (!dir || get().dragging) return;
    try {
      const flow = await api.flow.load(dir);
      if (get().dir === dir && !get().dragging) set({ flow });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  setMode: (mode) => {
    try {
      localStorage.setItem("nexus.flowMode", mode);
    } catch {
      /* ignore */
    }
    set({ mode });
  },

  select: (selectedNode, selectedEdge = null) => set({ selectedNode, selectedEdge }),

  // Local-only while dragging: 60 fps, no IPC.
  moveNode: (id, x, y) => {
    const flow = get().flow;
    if (!flow) return;
    set({ dragging: true, flow: { ...flow, nodes: flow.nodes.map((n) => (n.id === id ? { ...n, x, y } : n)) } });
  },

  commitLayout: async (ids) => {
    const { flow, dir } = get();
    set({ dragging: false });
    if (!flow || !dir) return;
    const positions = flow.nodes.filter((n) => ids.includes(n.id)).map((n) => ({ id: n.id, x: Math.round(n.x), y: Math.round(n.y) }));
    try {
      await api.flow.saveLayout(dir, positions);
    } catch (e) {
      set({ error: String(e) });
      void get().reload();
    }
  },

  connect: async (from, to) => {
    const { flow, dir } = get();
    if (!flow || !dir) return false;
    const a = splitRef(from);
    const b = splitRef(to);
    const src = flow.nodes.find((n) => n.id === a.node)?.outputs.find((p) => p.name === a.port);
    const dst = flow.nodes.find((n) => n.id === b.node)?.inputs.find((p) => p.name === b.port);
    if (!src || !dst) return false;
    if (!compatible(src.type, dst.type)) {
      set({ error: `Type mismatch: ${src.type} → ${dst.type}` });
      return false;
    }
    // Optimistic edge; replaced by the server's validated view.
    const optimistic: EdgeCheck = {
      index: flow.edges.length,
      from,
      to,
      from_type: src.type,
      to_type: dst.type,
      valid: true,
      reason: null,
    };
    set({ flow: { ...flow, edges: [...flow.edges, optimistic] } });
    try {
      const next = await api.flow.connect(dir, from, to);
      if (get().dir === dir) set({ flow: next });
      return true;
    } catch (e) {
      set({ error: String(e), flow: get().flow && { ...get().flow!, edges: get().flow!.edges.filter((x) => x !== optimistic) } });
      return false;
    }
  },

  disconnect: async (from, to) => {
    const { flow, dir } = get();
    if (!flow || !dir) return;
    set({ flow: { ...flow, edges: flow.edges.filter((e) => !(e.from === from && e.to === to)) }, selectedEdge: null });
    try {
      set({ flow: await api.flow.disconnect(dir, from, to) });
    } catch (e) {
      set({ error: String(e) });
      void get().reload();
    }
  },

  addNode: async (ref, x, y) => {
    const dir = get().dir;
    if (!dir) return;
    try {
      const flow = await api.flow.addNode(dir, ref, x, y);
      set({ flow, selectedNode: flow.nodes[flow.nodes.length - 1]?.id ?? null });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  removeNode: async (id) => {
    const { flow, dir } = get();
    if (!flow || !dir) return;
    const gone = (ref: string) => splitRef(ref).node === id;
    set({
      flow: { ...flow, nodes: flow.nodes.filter((n) => n.id !== id), edges: flow.edges.filter((e) => !gone(e.from) && !gone(e.to)) },
      selectedNode: null,
    });
    try {
      set({ flow: await api.flow.removeNode(dir, id) });
    } catch (e) {
      set({ error: String(e) });
      void get().reload();
    }
  },

  updateNode: async (id, patch) => {
    const { flow, dir } = get();
    if (!flow || !dir) return;
    set({
      flow: {
        ...flow,
        nodes: flow.nodes.map((n): NodeView => (n.id === id ? { ...n, ...(patch.config ? { config: patch.config } : {}), ...(patch.title ? { title: patch.title } : {}) } : n)),
      },
    });
    try {
      set({ flow: await api.flow.updateNode(dir, id, patch) });
    } catch (e) {
      set({ error: String(e) });
      void get().reload();
    }
  },

  setError: (error) => set({ error }),
  close: () => set({ dir: null, flow: null, selectedNode: null, selectedEdge: null }),

  run: async (node) => {
    const dir = get().dir;
    if (!dir || get().running) return null;
    // Optimistic: mark the planned nodes as queued immediately.
    set({ running: true, runStatus: node ? { ...get().runStatus, [node]: { status: "running" } } : {} });
    try {
      const r = await api.exec.run(dir, node);
      if (get().dir === dir) {
        const runStatus = { ...get().runStatus };
        for (const n of r.nodes) runStatus[n.node] = { status: n.status, ms: n.ms, error: n.error };
        set({ lastRun: r, runStatus });
        if (!r.ok) set({ error: r.nodes.find((n) => n.error)?.error ?? "Run failed" });
      }
      return r;
    } catch (e) {
      set({ error: String(e), runStatus: node ? { ...get().runStatus, [node]: { status: "error", error: String(e) } } : get().runStatus });
      return null;
    } finally {
      set({ running: false });
    }
  },

  onRunProgress: (e) => {
    if (e.flow !== get().dir) return;
    set({ runStatus: { ...get().runStatus, [e.node]: { status: e.status, ms: e.ms, error: e.error } } });
  },
}));
