import { useEffect, useState } from "react";
import { useFlowStore } from "../../state/flowStore";
import { TYPE_COLOR } from "./ports";
import type { NodeRun } from "../../api/tauri";

function NodeOutput({ run }: { run: NodeRun | null }) {
  if (!run) return null;
  return (
    <div className="field">
      <span>
        Last run · {run.status} · {run.ms} ms
      </span>
      {run.error && <em className="field-error">{run.error}</em>}
      {run.log.length > 0 && (
        <ul className="run-log">
          {run.log.map((l, i) => (
            <li key={i}>{l}</li>
          ))}
        </ul>
      )}
      {Object.entries(run.outputs).map(([port, v]) => (
        <div key={port} className="output">
          <b>{port}</b>
          <pre>{typeof v === "string" ? v : JSON.stringify(v, null, 2)}</pre>
        </div>
      ))}
    </div>
  );
}

export function Inspector() {
  const flow = useFlowStore((s) => s.flow);
  const id = useFlowStore((s) => s.selectedNode);
  const updateNode = useFlowStore((s) => s.updateNode);
  const removeNode = useFlowStore((s) => s.removeNode);
  const lastRun = useFlowStore((s) => s.lastRun);
  const node = flow?.nodes.find((n) => n.id === id);
  const [draft, setDraft] = useState("");
  const [bad, setBad] = useState<string | null>(null);

  useEffect(() => {
    setDraft(JSON.stringify(node?.config ?? {}, null, 2));
    setBad(null);
  }, [node?.id, node?.config]);

  if (!node) return null;

  const applyConfig = () => {
    try {
      const parsed = JSON.parse(draft || "{}");
      if (typeof parsed !== "object" || Array.isArray(parsed) || parsed === null) throw new Error("config must be an object");
      setBad(null);
      if (JSON.stringify(parsed) !== JSON.stringify(node.config ?? {})) void updateNode(node.id, { config: parsed });
    } catch (e) {
      setBad(String(e));
    }
  };

  return (
    <aside className="inspector">
      <div className="panel-title">Node {node.id}</div>
      <label className="field">
        <span>Title</span>
        <input
          key={node.id}
          defaultValue={node.title}
          onBlur={(e) => e.target.value !== node.title && void updateNode(node.id, { title: e.target.value })}
        />
      </label>
      <div className="field">
        <span>Template</span>
        <code>{node.ref}</code>
      </div>
      <div className="field">
        <span>Kind</span>
        <code>{node.kind || "—"}</code>
      </div>
      <div className="field">
        <span>Ports</span>
        <ul className="port-list">
          {node.inputs.map((p) => (
            <li key={"i" + p.name}>
              <i style={{ background: TYPE_COLOR[p.type] }} /> in <b>{p.name}</b> <code>{p.type}</code>
            </li>
          ))}
          {node.outputs.map((p) => (
            <li key={"o" + p.name}>
              <i style={{ background: TYPE_COLOR[p.type] }} /> out <b>{p.name}</b> <code>{p.type}</code>
            </li>
          ))}
        </ul>
      </div>
      <label className="field">
        <span>Config (JSON)</span>
        <textarea spellCheck={false} rows={10} value={draft} onChange={(e) => setDraft(e.target.value)} onBlur={applyConfig} />
        {bad && <em className="field-error">{bad}</em>}
      </label>
      <NodeOutput run={lastRun?.nodes.find((r) => r.node === node.id) ?? null} />
      <button className="danger" onClick={() => void removeNode(node.id)}>
        Delete node
      </button>
    </aside>
  );
}
