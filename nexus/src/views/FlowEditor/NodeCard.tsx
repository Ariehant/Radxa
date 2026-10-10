import { memo, type PointerEvent, type ReactNode } from "react";
import type { NodeView, PortType } from "../../api/tauri";
import { Port } from "./Port";
import { HEADER_H, ROW_H, nodeHeight } from "./ports";

interface Props {
  node: NodeView;
  /** Zoomed far out: draw a lightweight box instead of full ports (level of detail). */
  compact?: boolean;
  selected: boolean;
  isEntry: boolean;
  dragType: PortType | null;
  connected: Set<string>;
  status?: ReactNode;
  actions?: ReactNode;
  onStartDrag: (e: PointerEvent, node: NodeView) => void;
  onStartLink: (e: PointerEvent, ref: string, type: PortType) => void;
}

function NodeCardImpl({ node, compact, selected, isEntry, dragType, connected, status, actions, onStartDrag, onStartLink }: Props) {
  const rows = Math.max(node.inputs.length, node.outputs.length);
  if (compact) {
    return (
      <div
        className={"node-card compact" + (selected ? " selected" : "") + (node.missing_template ? " missing" : "")}
        style={{ transform: `translate(${node.x}px, ${node.y}px)`, width: node.w, height: nodeHeight(node) }}
        data-node={node.id}
        onPointerDown={(e) => onStartDrag(e, node)}
      >
        <span className="node-title">{node.title}</span>
      </div>
    );
  }
  return (
    <div
      className={"node-card" + (selected ? " selected" : "") + (node.missing_template ? " missing" : "")}
      style={{ transform: `translate(${node.x}px, ${node.y}px)`, width: node.w, height: nodeHeight(node) }}
      data-node={node.id}
      onPointerDown={(e) => onStartDrag(e, node)}
    >
      <div className="node-header" style={{ height: HEADER_H }}>
        <span className="node-title" title={node.ref}>
          {isEntry && <span className="entry-mark" title="Entry node">▶</span>}
          {node.title}
        </span>
        <span className="node-kind">{node.missing_template ? "missing" : node.kind}</span>
        {actions}
      </div>
      <div className="node-ports">
        {Array.from({ length: rows }, (_, i) => (
          <div className="port-row" key={i} style={{ height: ROW_H }}>
            <div className="port-cell">
              {node.inputs[i] && (
                <Port nodeId={node.id} port={node.inputs[i]} dir="in" dragType={dragType} connected={connected.has(`${node.id}.${node.inputs[i].name}`)} />
              )}
            </div>
            <div className="port-cell right">
              {node.outputs[i] && (
                <Port
                  nodeId={node.id}
                  port={node.outputs[i]}
                  dir="out"
                  dragType={null}
                  connected={connected.has(`${node.id}.${node.outputs[i].name}`)}
                  onStartLink={onStartLink}
                />
              )}
            </div>
          </div>
        ))}
      </div>
      {status}
    </div>
  );
}

export const NodeCard = memo(NodeCardImpl);
