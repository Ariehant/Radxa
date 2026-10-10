import { useStore } from "../../state/store";
import { VirtualList } from "../../components/VirtualList";
import type { LinkRef } from "../../api/tauri";

const ROW = 44;

export function BacklinksPanel() {
  const backlinks = useStore((s) => s.backlinks);
  const openFile = useStore((s) => s.openFile);

  return (
    <aside className="backlinks">
      <div className="panel-title">
        Backlinks <span className="count">{backlinks.length}</span>
      </div>
      {backlinks.length === 0 ? (
        <div className="panel-empty">No notes link here yet.</div>
      ) : (
        <VirtualList<LinkRef>
          items={backlinks}
          rowHeight={ROW}
          getKey={(b) => `${b.path}:${b.kind}:${b.port}`}
          renderRow={(b) => (
            <div className="backlink" onClick={() => openFile(b.path)} title={b.path}>
              <div className="backlink-title">{b.title}</div>
              <div className="backlink-meta">
                {b.kind === "relation" ? `${b.port} →` : b.kind} · {b.path}
              </div>
            </div>
          )}
        />
      )}
    </aside>
  );
}
