import type { EdgeCheck, NodeView } from "../../api/tauri";
import { TYPE_COLOR, bezier, intersects, portAnchor, splitRef, type Rect } from "./ports";

interface Props {
  edges: EdgeCheck[];
  nodes: Map<string, NodeView>;
  viewport: Rect;
  selected: string | null;
  onSelect: (key: string) => void;
  pending: { a: { x: number; y: number }; b: { x: number; y: number }; bad: boolean } | null;
}

export const edgeKey = (e: { from: string; to: string }) => `${e.from}->${e.to}`;

/** Only edges whose bounding box touches the viewport are rendered (spec §9.5). */
export function EdgeLayer({ edges, nodes, viewport, selected, onSelect, pending }: Props) {
  const paths = [];
  for (const e of edges) {
    const f = splitRef(e.from);
    const t = splitRef(e.to);
    const fn = nodes.get(f.node);
    const tn = nodes.get(t.node);
    if (!fn || !tn) continue;
    const a = portAnchor(fn, f.port, "out");
    const b = portAnchor(tn, t.port, "in");
    if (!a || !b) continue;
    const box = { x: Math.min(a.x, b.x) - 60, y: Math.min(a.y, b.y) - 60, w: Math.abs(a.x - b.x) + 120, h: Math.abs(a.y - b.y) + 120 };
    if (!intersects(box, viewport)) continue;
    const key = edgeKey(e);
    const d = bezier(a, b);
    const color = !e.valid ? "var(--danger)" : e.from_type ? TYPE_COLOR[e.from_type] : "var(--fg-3)";
    paths.push(
      <g key={key} className={"edge" + (e.valid ? "" : " invalid") + (selected === key ? " selected" : "")}>
        <path d={d} className="edge-hit" onPointerDown={(ev) => { ev.stopPropagation(); onSelect(key); }}>
          <title>{e.valid ? `${e.from} → ${e.to} (${e.from_type})` : `${e.from} → ${e.to}: ${e.reason}`}</title>
        </path>
        <path d={d} className="edge-line" stroke={color} />
      </g>,
    );
  }
  return (
    <svg className="edge-layer" width="1" height="1">
      {paths}
      {pending && <path d={bezier(pending.a, pending.b)} className={"edge-line pending" + (pending.bad ? " bad" : "")} />}
    </svg>
  );
}
