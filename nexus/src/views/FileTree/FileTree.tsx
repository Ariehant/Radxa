import { useCallback, useMemo, useRef } from "react";
import { api } from "../../api/tauri";
import type { TreeEntry } from "../../api/tauri";
import { VirtualList } from "../../components/VirtualList";
import { Skeleton } from "../../components/Skeleton";
import { useStore } from "../../state/store";

const ROW = 24;

/** Entries arrive depth-first; hide descendants of collapsed directories. */
export function visibleEntries(tree: TreeEntry[], expanded: Record<string, boolean>): TreeEntry[] {
  const out: TreeEntry[] = [];
  let hideBelow = Infinity;
  for (const e of tree) {
    if (e.depth > hideBelow) continue;
    hideBelow = Infinity;
    out.push(e);
    if (e.is_dir && !expanded[e.path]) hideBelow = e.depth;
  }
  return out;
}

export function FileTree() {
  const tree = useStore((s) => s.tree);
  const loading = useStore((s) => s.treeLoading);
  const expanded = useStore((s) => s.expanded);
  const toggleDir = useStore((s) => s.toggleDir);
  const openFile = useStore((s) => s.openFile);
  const current = useStore((s) => s.doc?.path ?? (s.flowDir ? `${s.flowDir}/flow.md` : undefined));

  const rows = useMemo(() => visibleEntries(tree, expanded), [tree, expanded]);

  // While the initial scan runs, files scrolled into view jump the queue
  // (priority: open → viewport → recent → idle).
  const timer = useRef<ReturnType<typeof setTimeout>>();
  const onRange = useCallback(
    (start: number, end: number) => {
      clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        const st = useStore.getState().indexStatus;
        if (st && !st.scanning && st.pending === 0) return;
        const paths = rows.slice(start, end).filter((e) => !e.is_dir).map((e) => e.path);
        if (paths.length) void api.index.prioritize(paths, "viewport").catch(() => {});
      }, 250);
    },
    [rows],
  );

  if (loading && tree.length === 0) return <Skeleton lines={12} />;

  return (
    <VirtualList
      className="file-tree"
      items={rows}
      rowHeight={ROW}
      getKey={(e) => e.path}
      onRangeChange={onRange}
      renderRow={(e) => (
        <div
          className={"tree-row" + (e.path === current ? " active" : "")}
          style={{ paddingLeft: 8 + e.depth * 14 }}
          title={e.path}
          onClick={() => (e.is_dir ? toggleDir(e.path) : openFile(e.path))}
        >
          <span className="tree-icon">{e.is_dir ? (expanded[e.path] ? "▾" : "▸") : fileIcon(e.name)}</span>
          <span className="tree-name">{e.name}</span>
        </div>
      )}
    />
  );
}

function fileIcon(name: string): string {
  if (name.endsWith(".canvas")) return "◇";
  if (name.endsWith(".db.md")) return "▦";
  if (name.endsWith(".view.md")) return "▤";
  if (name.endsWith(".md")) return "·";
  return "○";
}
