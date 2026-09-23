import { create } from "zustand";
import { api, RpcError, type IndexStatus, type IndexUpdated, type LinkRef, type TreeEntry, type VaultInfo } from "../api/tauri";

export type EditorMode = "rich" | "source";

export interface OpenDoc {
  path: string;
  /** Live editor content — updated optimistically on every keystroke. */
  content: string;
  /** Hash of the version on disk that `content` descends from. */
  hash: string;
  /** Bumped when content is replaced from outside the editor (reload). */
  rev: number;
  dirty: boolean;
  saving: boolean;
  conflict: boolean;
}

interface NexusState {
  vault: VaultInfo | null;
  tree: TreeEntry[];
  treeLoading: boolean;
  expanded: Record<string, boolean>;
  doc: OpenDoc | null;
  docLoading: boolean;
  editorMode: EditorMode;
  error: string | null;
  backlinks: LinkRef[];
  indexStatus: IndexStatus | null;

  openVault: (path: string, create?: boolean) => Promise<void>;
  closeVault: () => Promise<void>;
  refreshTree: () => Promise<void>;
  toggleDir: (path: string) => void;
  openFile: (path: string) => Promise<void>;
  editDoc: (content: string) => void;
  saveDoc: (force?: boolean) => Promise<void>;
  reloadDoc: () => Promise<void>;
  setEditorMode: (m: EditorMode) => void;
  createNote: (title: string) => Promise<void>;
  setError: (e: string | null) => void;
  loadBacklinks: () => Promise<void>;
  openLink: (target: string) => Promise<void>;
  onIndexUpdated: (e: IndexUpdated) => void;
  refreshIndexStatus: () => Promise<void>;
}

const DEFAULT_EXPANDED = { notes: true, flows: true };
const AUTOSAVE_MS = 800;
let autosaveTimer: ReturnType<typeof setTimeout> | undefined;

function loadMode(): EditorMode {
  try {
    return localStorage.getItem("nexus.editorMode") === "source" ? "source" : "rich";
  } catch {
    return "rich";
  }
}

export const useStore = create<NexusState>((set, get) => ({
  vault: null,
  tree: [],
  treeLoading: false,
  expanded: DEFAULT_EXPANDED,
  doc: null,
  docLoading: false,
  editorMode: loadMode(),
  error: null,
  backlinks: [],
  indexStatus: null,

  openVault: async (path, create = false) => {
    try {
      const vault = create ? await api.vault.create(path) : await api.vault.open(path);
      set({ vault, doc: null, expanded: DEFAULT_EXPANDED, error: null });
      await get().refreshTree();
      void get().refreshIndexStatus();
    } catch (e) {
      set({ error: String(e) });
    }
  },

  closeVault: async () => {
    await get().saveDoc();
    await api.vault.close();
    set({ vault: null, tree: [], doc: null, backlinks: [] });
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
    if (get().doc?.dirty) await get().saveDoc();
    set({ docLoading: true });
    try {
      const note = await api.note.read(path);
      set({
        doc: { path: note.path, content: note.content, hash: note.hash, rev: 0, dirty: false, saving: false, conflict: false },
        backlinks: [],
      });
      void get().loadBacklinks();
    } catch (e) {
      set({ error: String(e) });
    } finally {
      set({ docLoading: false });
    }
  },

  // Optimistic: the store (and so every view) updates synchronously; the
  // disk write follows on a short debounce.
  editDoc: (content) => {
    const doc = get().doc;
    if (!doc || doc.content === content) return;
    set({ doc: { ...doc, content, dirty: true } });
    clearTimeout(autosaveTimer);
    autosaveTimer = setTimeout(() => void get().saveDoc(), AUTOSAVE_MS);
  },

  saveDoc: async (force = false) => {
    clearTimeout(autosaveTimer);
    const doc = get().doc;
    if (!doc || !doc.dirty || doc.saving || (doc.conflict && !force)) return;
    const { path, content } = doc;
    set({ doc: { ...doc, saving: true } });
    try {
      const saved = await api.note.write(path, content, force ? undefined : doc.hash);
      const cur = get().doc;
      if (cur?.path === path) {
        set({ doc: { ...cur, hash: saved.hash, saving: false, conflict: false, dirty: cur.content !== content } });
        if (cur.content !== content) void get().saveDoc();
      }
    } catch (e) {
      const cur = get().doc;
      const conflict = e instanceof RpcError && e.code === -32009;
      if (cur?.path === path) set({ doc: { ...cur, saving: false, conflict } });
      if (!conflict) set({ error: String(e) });
    }
  },

  reloadDoc: async () => {
    const doc = get().doc;
    if (!doc) return;
    const note = await api.note.read(doc.path);
    const cur = get().doc;
    if (cur?.path !== doc.path) return;
    set({ doc: { ...cur, content: note.content, hash: note.hash, rev: cur.rev + 1, dirty: false, conflict: false } });
  },

  setEditorMode: (editorMode) => {
    try {
      localStorage.setItem("nexus.editorMode", editorMode);
    } catch {
      /* private mode */
    }
    const doc = get().doc;
    set({ editorMode, doc: doc ? { ...doc, rev: doc.rev + 1 } : doc });
  },

  createNote: async (title) => {
    try {
      const saved = await api.note.create(title);
      await get().openFile(saved.path);
    } catch (e) {
      set({ error: String(e) });
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
    else await get().createNote(target);
  },

  onIndexUpdated: (e) => {
    if (e.structural) void get().refreshTree();
    const doc = get().doc;
    if (doc && e.changed.some((p) => p.toLowerCase() === doc.path.toLowerCase())) {
      // Changed on disk. If it wasn't our own save, pull it in (or flag a
      // conflict when there are unsaved local edits).
      void api.note.read(doc.path).then((note) => {
        const cur = get().doc;
        if (!cur || cur.path !== doc.path || note.hash === cur.hash || cur.saving) return;
        if (cur.dirty) set({ doc: { ...cur, conflict: true } });
        else set({ doc: { ...cur, content: note.content, hash: note.hash, rev: cur.rev + 1 } });
      });
    }
    if (doc && e.removed.some((p) => p.toLowerCase() === doc.path.toLowerCase()) && !doc.dirty) {
      set({ doc: null, backlinks: [] });
    }
    if (get().doc && (e.changed.length > 0 || e.removed.length > 0)) void get().loadBacklinks();
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
