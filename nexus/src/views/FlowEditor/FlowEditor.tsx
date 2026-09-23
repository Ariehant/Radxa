import { useEffect, type ReactNode } from "react";
import type { NodeView } from "../../api/tauri";
import { useStore } from "../../state/store";
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

function RunButton({ node }: { node: NodeView }) {
  const running = useFlowStore((s) => s.running);
  const run = useFlowStore((s) => s.run);
  return (
    <button
      className="ghost run-btn"
      title="Run this node (and anything upstream)"
      disabled={running || node.missing_template}
      onPointerDown={(e) => e.stopPropagation()}
      onClick={(e) => {
        e.stopPropagation();
        void run(node.id);
      }}
    >
      ▶
    </button>
  );
}

function RunBadge({ node }: { node: NodeView }) {
  const st = useFlowStore((s) => s.runStatus[node.id]);
  if (!st) return null;
  const label: Record<string, ReactNode> = {
    running: <span className="spinner" />,
    ok: `✓ ${st.ms ?? 0} ms`,
    cached: "✓ cached",
    error: "✗ error",
    skipped: "– skipped",
  };
  return (
    <div className={`node-status status-${st.status}`} title={st.error ?? ""}>
      {label[st.status]}
    </div>
  );
}

const renderActions = (n: NodeView) => <RunButton node={n} />;
const renderStatus = (n: NodeView) => <RunBadge node={n} />;

export function FlowEditor({ dir }: { dir: string }) {
  const flow = useFlowStore((s) => s.flow);
  const loading = useFlowStore((s) => s.loading);
  const open = useFlowStore((s) => s.open);
  const mode = useFlowStore((s) => s.mode);
  const setMode = useFlowStore((s) => s.setMode);
  const running = useFlowStore((s) => s.running);
  const run = useFlowStore((s) => s.run);
  const lastRun = useFlowStore((s) => s.lastRun);
  const openFile = useStore((s) => s.openFile);

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
        {lastRun && (
          <button className="ghost" title={lastRun.log_path} onClick={() => void openFile(lastRun.log_path)}>
            {lastRun.ok ? "✓" : "✗"} Last run log
          </button>
        )}
        <button className="primary" disabled={running || flow.nodes.length === 0} onClick={() => void run()}>
          {running ? "Running…" : "▶ Run flow"}
        </button>
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
        {mode === "canvas" && <Canvas renderActions={renderActions} renderStatus={renderStatus} />}
        {mode === "table" && <TableView />}
        {mode === "hybrid" && <HybridView left={<Canvas renderActions={renderActions} renderStatus={renderStatus} />} right={<TableView />} />}
        {mode !== "hybrid" && <Inspector />}
      </div>
    </div>
  );
}
