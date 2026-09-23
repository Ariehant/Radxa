import { useEffect } from "react";
import { canvasCenter, useFlowStore } from "../../state/flowStore";
import { Skeleton } from "../../components/Skeleton";
import { Canvas } from "./Canvas";
import { Inspector } from "./Inspector";
import { TYPE_COLOR } from "./ports";
import { TableView } from "../TableView/TableView";
import { HybridView } from "../HybridView/HybridView";
import type { FlowViewMode } from "../../state/flowStore";

const MODES: { id: FlowViewMode; label: string }[] = [
  { id: "canvas", label: "Canvas" },
  { id: "table", label: "Table" },
  { id: "hybrid", label: "Hybrid" },
];

function Palette() {
  const templates = useFlowStore((s) => s.templates);
  const addNode = useFlowStore((s) => s.addNode);
  // Drop new nodes at the centre of the visible canvas, nudged so repeated
  // adds don't stack exactly on top of each other.
  const place = () => {
    const n = useFlowStore.getState().flow?.nodes.length ?? 0;
    return { x: canvasCenter.x - 120 + (n % 5) * 24, y: canvasCenter.y - 60 + (n % 5) * 24 };
  };
  return (
    <aside className="palette">
      <div className="panel-title">Nodes</div>
      {templates.length === 0 && <div className="panel-empty">No templates in templates/nodes/</div>}
      {templates.map((t) => (
        <button
          key={t.ref}
          className="palette-item"
          title={t.description ?? t.ref}
          onClick={() => {
            const p = place();
            void addNode(t.ref, p.x, p.y);
          }}
        >
          <span className="palette-title">{t.title}</span>
          <span className="palette-ports">
            {t.inputs.map((p) => (
              <i key={"i" + p.name} style={{ background: TYPE_COLOR[p.type] }} title={`in ${p.name}: ${p.type}`} />
            ))}
            <span className="arrow">→</span>
            {t.outputs.map((p) => (
              <i key={"o" + p.name} style={{ background: TYPE_COLOR[p.type] }} title={`out ${p.name}: ${p.type}`} />
            ))}
          </span>
        </button>
      ))}
    </aside>
  );
}

export function FlowEditor({ dir }: { dir: string }) {
  const flow = useFlowStore((s) => s.flow);
  const loading = useFlowStore((s) => s.loading);
  const open = useFlowStore((s) => s.open);
  const mode = useFlowStore((s) => s.mode);
  const setMode = useFlowStore((s) => s.setMode);

  useEffect(() => {
    void open(dir);
  }, [dir, open]);

  if (!flow || flow.dir !== dir) return loading ? <Skeleton lines={10} /> : <div className="empty">Could not load flow.</div>;

  const invalid = flow.edges.filter((e) => !e.valid).length;

  return (
    <div className="flow-editor">
      <div className="note-header">
        <span className="flow-name">{flow.name || flow.dir}</span>
        <span className="note-path">{flow.path}</span>
        {(flow.errors.length > 0 || invalid > 0) && (
          <span className="flow-errors" title={[...flow.errors, ...flow.edges.filter((e) => !e.valid).map((e) => `${e.from} → ${e.to}: ${e.reason}`)].join("\n")}>
            ⚠ {flow.errors.length + invalid} issue{flow.errors.length + invalid === 1 ? "" : "s"}
          </span>
        )}
        <div className="segmented">
          {MODES.map((m) => (
            <button key={m.id} className={mode === m.id ? "active" : ""} onClick={() => setMode(m.id)}>
              {m.label}
            </button>
          ))}
        </div>
      </div>
      <div className="flow-body">
        {mode !== "table" && <Palette />}
        {mode === "canvas" && <Canvas />}
        {mode === "table" && <TableView />}
        {mode === "hybrid" && <HybridView left={<Canvas />} right={<TableView />} />}
        {mode !== "hybrid" && <Inspector />}
      </div>
    </div>
  );
}
