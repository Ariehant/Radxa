import type { PointerEvent } from "react";
import type { PortDef, PortType } from "../../api/tauri";
import { TYPE_COLOR, compatible } from "./ports";

interface Props {
  nodeId: string;
  port: PortDef;
  dir: "in" | "out";
  /** Type being dragged from an output, to highlight valid drop targets. */
  dragType: PortType | null;
  connected: boolean;
  onStartLink?: (e: PointerEvent, ref: string, type: PortType) => void;
}

export function Port({ nodeId, port, dir, dragType, connected, onStartLink }: Props) {
  const ref = `${nodeId}.${port.name}`;
  const hint = dir === "in" && dragType ? (compatible(dragType, port.type) ? " ok" : " bad") : "";
  return (
    <div className={`port port-${dir}${hint}`} title={`${port.name}: ${port.type}${port.description ? ` — ${port.description}` : ""}`}>
      {dir === "out" && <span className="port-label">{port.name}</span>}
      <span
        className={"port-dot" + (connected ? " connected" : "")}
        data-port={ref}
        data-port-dir={dir}
        data-port-type={port.type}
        style={{ borderColor: TYPE_COLOR[port.type], background: connected ? TYPE_COLOR[port.type] : undefined }}
        onPointerDown={
          dir === "out" && onStartLink
            ? (e) => {
                e.stopPropagation();
                onStartLink(e, ref, port.type);
              }
            : (e) => e.stopPropagation()
        }
      />
      {dir === "in" && (
        <span className="port-label">
          {port.name}
          <span className="port-type">{port.type}</span>
        </span>
      )}
    </div>
  );
}
