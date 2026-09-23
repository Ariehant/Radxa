import { create } from "zustand";
import { api, type IndexStatus, type IndexUpdated, type LinkRef, type TreeEntry, type VaultInfo } from "../api/tauri";

export interface OpenDoc {
  path: string;
  content: string;
}

interface NexusState {
  vault: VaultInfo | null;
  tree: TreeEntry[];
  treeLoading: boolean;
  expanded: Record<string, boolean>;
  doc: OpenDoc | null;
  docLoading: boolean;
  error: string | null;
  backlinks: LinkRef[];
  indexStatus: IndexStatus | null;

  openVault: (path: string, create?: boolean) => Promise<void>;
  closeVault: () => Promise<void>;
  refreshTree: () => Promise<void>;
  toggleDir: (path: string) => void;
  openFile: (path: string) => Promise<void>;
  setError: (e: string | null) => void;
  loadBacklinks: () => Promise<void>;
  openLink: (target: string) => Promise<void>;
  onIndexUpdated: (e: IndexUpdated) => void;
  refreshIndexStatus: () => Promise<void>;
}

const DEFAULT_EXPANDED = { notes: true, flows: true };

export const useStore = create<NexusState>((set, get) => ({
  vault: null,
  tree: [],
  treeLoading: false,
  expanded: DEFAULT_EXPANDED,
  doc: null,
  docLoading: false,
  error: null,
  backlinks: [],
  indexStatus: null,

  openVault: async (path, create = false) => {
    try {
      const vault = create ? await api.vault.create(path) : await api.vault.open(path);
      set({ vault, doc: null, expanded: DEFAULT_EXPANDED, error: null });
      await get().refreshTree();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  closeVault: async () => {
    await api.vault.close();
    set({ vault: null, tree: [], doc: null });
  },

  refreshTree: async () => {
    set({ treeLoading: true });
    try {
      set({ tree: await api.vault.tree() });
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ treeLoading: false });
    }
  },

  toggleDir: (path) => set((s) => ({ expanded: { ...s.expanded, [path]: !s.expanded[path] } })),

  openFile: async (path) => {
    set({ docLoading: true });
    try {
      const note = await api.note.read(path);
      set({ doc: { path: note.path, content: note.content }, backlinks: [] });
      void get().loadBacklinks();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ docLoading: false });
    }
  },

  setError: (error) => set({ error }),

  loadBacklinks: async () => {
    const path = get().doc?.path;
    if (!path) return;
    try {
      const backlinks = await api.note.backlinks(path);
      if (get().doc?.path === path) set({ backlinks });
    } catch (e) {
      set({ error: String(e) });
    }
  },

  openLink: async (target) => {
    const [resolved] = await api.link.resolve([target]);
    if (resolved) await get().openFile(resolved);
    else set({ error: `No note named “${target}”` });
  },

  onIndexUpdated: (e) => {
    if (e.structural) void get().refreshTree();
    const current = get().doc?.path;
    // Any change can add/remove a backlink to the open note; the query is cheap.
    if (current && (e.changed.length > 0 || e.removed.length > 0)) void get().loadBacklinks();
    void get().refreshIndexStatus();
  },

  refreshIndexStatus: async () => {
    try {
      set({ indexStatus: await api.index.status() });
    } catch {
      /* vault closed */
    }
  },
}));
