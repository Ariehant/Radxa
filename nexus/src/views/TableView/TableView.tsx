import { useEffect, useMemo, useState } from "react";
import { api, type FlowTable } from "../../api/tauri";
import { useFlowStore } from "../../state/flowStore";
import { VirtualList } from "../../components/VirtualList";
import { Skeleton } from "../../components/Skeleton";
import { TYPE_COLOR } from "../FlowEditor/ports";
import { edgeKey } from "../FlowEditor/EdgeLayer";
import type { PortType } from "../../api/tauri";

const ROW = 30;

/**
 * Nodes and edges of the open flow as rows. Data comes from the SQLite index
 * (`view.flowTable`); selection and in-flight edits are the same flow store
 * the canvas uses, so switching views never reloads anything.
 */
export function TableView() {
  const dir = useFlowStore((s) => s.dir);
  const flow = useFlowStore((s) => s.flow);
  const selectedNode = useFlowStore((s) => s.selectedNode);
  const selectedEdge = useFlowStore((s) => s.selectedEdge);
  const { select, updateNode, disconnect } = useFlowStore.getState();
  const [table, setTable] = useState<FlowTable | null>(null);

  // Every committed flow change is indexed synchronously before the RPC
  // returns, so re-querying on each store update is always consistent.
  useEffect(() => {
    if (!dir || useFlowStore.getState().dragging) return;
    let live = true;
    api.view.flowTable(dir).then((t) => live && setTable(t)).catch(() => {});
    return () => {
      live = false;
    };
  }, [dir, flow]);

  const validity = useMemo(() => new Map((flow?.edges ?? []).map((e) => [edgeKey(e), e])), [flow?.edges]);
  // While a drag is in flight the store is ahead of the index; prefer it.
  const livePos = useMemo(() => new Map((flow?.nodes ?? []).map((n) => [n.id, n])), [flow?.nodes]);

  if (!table) return <Skeleton lines={8} />;

  return (
    <div className="table-view">
      <section className="table-section">
        <div className="panel-title">
          Nodes <span className="count">{table.nodes.length}</span>
        </div>
        <div className="grid-row grid-head nodes-grid">
          <span>id</span>
          <span>title</span>
          <span>kind</span>
          <span>template</span>
          <span>ports</span>
          <span>position</span>
          <span>config</span>
        </div>
        <div className="grid-body" style={{ height: Math.min(table.nodes.length, 12) * ROW + 2 }}>
          <VirtualList
            items={table.nodes}
            rowHeight={ROW}
            getKey={(n) => n.id}
            renderRow={(n) => {
              const live = livePos.get(n.id);
              return (
                <div className={"grid-row nodes-grid" + (n.id === selectedNode ? " selected" : "")} onClick={() => select(n.id)}>
                  <code>{n.id}</code>
                  <input
                    key={`${n.id}:${n.title ?? ""}`}
                    className="cell-input"
                    defaultValue={n.title ?? n.template_title ?? ""}
                    onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                    onBlur={(e) => {
                      const v = e.target.value;
                      if (v !== (n.title ?? n.template_title ?? "")) void updateNode(n.id, { title: v });
                    }}
                  />
                  <span className="kind-badge">{n.kind ?? "?"}</span>
                  <span className="muted" title={n.ref}>
                    {n.template_title ?? n.ref}
                  </span>
                  <span className="muted">
                    {n.inputs} in · {n.outputs} out
                  </span>
                  <span className="muted mono">
                    {Math.round(live?.x ?? n.x ?? 0)}, {Math.round(live?.y ?? n.y ?? 0)}
                  </span>
                  <code className="muted ellipsis" title={JSON.stringify(n.config)}>
                    {JSON.stringify(n.config ?? {})}
                  </code>
                </div>
              );
            }}
          />
        </div>
      </section>
      <section className="table-section">
        <div className="panel-title">
          Edges <span className="count">{table.edges.length}</span>
        </div>
        <div className="grid-row grid-head edges-grid">
          <span>from</span>
          <span>to</span>
          <span>type</span>
          <span>status</span>
          <span />
        </div>
        <div className="grid-body" style={{ height: Math.max(1, Math.min(table.edges.length, 12)) * ROW + 2 }}>
          <VirtualList
            items={table.edges}
            rowHeight={ROW}
            getKey={(e) => edgeKey(e)}
            renderRow={(e) => {
              const v = validity.get(edgeKey(e));
              const key = edgeKey(e);
              return (
                <div className={"grid-row edges-grid" + (key === selectedEdge ? " selected" : "")} onClick={() => select(null, key)}>
                  <code>{e.from}</code>
                  <code>{e.to}</code>
                  <span className="type-chip" style={{ borderColor: e.data_type ? TYPE_COLOR[e.data_type as PortType] : undefined }}>
                    {e.data_type ?? "?"}
                  </span>
                  <span className={v && !v.valid ? "bad" : "ok"} title={v?.reason ?? ""}>
                    {v ? (v.valid ? "valid" : v.reason) : "…"}
                  </span>
                  <button
                    className="ghost"
                    title="Remove edge"
                    onClick={(ev) => {
                      ev.stopPropagation();
                      void disconnect(e.from, e.to);
                    }}
                  >
                    ✕
                  </button>
                </div>
              );
            }}
          />
        </div>
      </section>
      {flow?.order && (
        <section className="table-section">
          <div className="panel-title">Execution order</div>
          <div className="order">{flow.order.join("  →  ")}</div>
        </section>
      )}
    </div>
  );
}
