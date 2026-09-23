import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent, type ReactNode, type WheelEvent } from "react";
import type { NodeView, PortType } from "../../api/tauri";
import { canvasCenter, useFlowStore } from "../../state/flowStore";
import { EdgeLayer } from "./EdgeLayer";
import { NodeCard } from "./NodeCard";
import { compatible, intersects, nodeHeight, portAnchor, splitRef, type Rect } from "./ports";

type Gesture =
  | { kind: "pan"; sx: number; sy: number; vx: number; vy: number }
  | { kind: "drag"; id: string; sx: number; sy: number; nx: number; ny: number; moved: boolean }
  | { kind: "link"; from: string; type: PortType };

interface Props {
  /** Extra header controls per node (e.g. run button). */
  renderActions?: (n: NodeView) => ReactNode;
  renderStatus?: (n: NodeView) => ReactNode;
}

const MARGIN = 300;
/** Below this zoom, node cards and edges render in low detail. */
const LOD_ZOOM = 0.45;

export function Canvas({ renderActions, renderStatus }: Props) {
  const flow = useFlowStore((s) => s.flow);
  const selectedNode = useFlowStore((s) => s.selectedNode);
  const selectedEdge = useFlowStore((s) => s.selectedEdge);
  const { select, moveNode, commitLayout, connect, disconnect, removeNode } = useFlowStore.getState();

  const root = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ x: 40, y: 40, zoom: 1 });
  const [size, setSize] = useState({ w: 1000, h: 700 });
  const [link, setLink] = useState<{ from: string; type: PortType; a: { x: number; y: number }; b: { x: number; y: number }; bad: boolean } | null>(null);
  const gesture = useRef<Gesture | null>(null);
  const viewRef = useRef(view);
  viewRef.current = view;

  useEffect(() => {
    const el = root.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const fit = useCallback(() => {
    const f = useFlowStore.getState().flow;
    const el = root.current;
    if (!f || !el) return;
    if (f.nodes.length === 0) return setView({ x: 40, y: 40, zoom: 1 });
    const minX = Math.min(...f.nodes.map((n) => n.x));
    const minY = Math.min(...f.nodes.map((n) => n.y));
    const maxX = Math.max(...f.nodes.map((n) => n.x + n.w));
    const maxY = Math.max(...f.nodes.map((n) => n.y + nodeHeight(n)));
    const zoom = Math.min(1, Math.max(0.15, Math.min((el.clientWidth - 80) / (maxX - minX), (el.clientHeight - 80) / (maxY - minY))));
    setView({ zoom, x: (el.clientWidth - (maxX - minX) * zoom) / 2 - minX * zoom, y: (el.clientHeight - (maxY - minY) * zoom) / 2 - minY * zoom });
  }, []);

  // Fit the graph once per opened flow.
  const fitted = useRef<string | null>(null);
  useEffect(() => {
    if (!flow || fitted.current === flow.dir) return;
    fitted.current = flow.dir;
    fit();
  }, [flow, fit]);

  const toWorld = useCallback((clientX: number, clientY: number) => {
    const r = root.current!.getBoundingClientRect();
    const v = viewRef.current;
    return { x: (clientX - r.left - v.x) / v.zoom, y: (clientY - r.top - v.y) / v.zoom };
  }, []);

  const nodeMap = useMemo(() => new Map((flow?.nodes ?? []).map((n) => [n.id, n])), [flow?.nodes]);
  const connected = useMemo(() => {
    const s = new Set<string>();
    for (const e of flow?.edges ?? []) {
      s.add(e.from);
      s.add(e.to);
    }
    return s;
  }, [flow?.edges]);

  useEffect(() => {
    canvasCenter.x = (size.w / 2 - view.x) / view.zoom;
    canvasCenter.y = (size.h / 2 - view.y) / view.zoom;
  }, [view, size]);

  const viewport: Rect = {
    x: -view.x / view.zoom - MARGIN,
    y: -view.y / view.zoom - MARGIN,
    w: size.w / view.zoom + 2 * MARGIN,
    h: size.h / view.zoom + 2 * MARGIN,
  };

  const onStartDrag = useCallback((e: PointerEvent, n: NodeView) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    root.current?.setPointerCapture(e.pointerId);
    const w = toWorld(e.clientX, e.clientY);
    gesture.current = { kind: "drag", id: n.id, sx: w.x, sy: w.y, nx: n.x, ny: n.y, moved: false };
    useFlowStore.getState().select(n.id);
  }, [toWorld]);

  const onStartLink = useCallback((e: PointerEvent, from: string, type: PortType) => {
    if (e.button !== 0) return;
    root.current?.setPointerCapture(e.pointerId);
    const n = useFlowStore.getState().flow?.nodes.find((x) => x.id === splitRef(from).node);
    const a = n && portAnchor(n, splitRef(from).port, "out");
    if (!a) return;
    gesture.current = { kind: "link", from, type };
    setLink({ from, type, a, b: toWorld(e.clientX, e.clientY), bad: false });
  }, [toWorld]);

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0 && e.button !== 1) return;
    root.current?.focus();
    root.current?.setPointerCapture(e.pointerId);
    gesture.current = { kind: "pan", sx: e.clientX, sy: e.clientY, vx: view.x, vy: view.y };
    select(null);
  };

  const onPointerMove = (e: PointerEvent) => {
    const g = gesture.current;
    if (!g) return;
    if (g.kind === "pan") {
      setView((v) => ({ ...v, x: g.vx + e.clientX - g.sx, y: g.vy + e.clientY - g.sy }));
    } else if (g.kind === "drag") {
      const w = toWorld(e.clientX, e.clientY);
      g.moved = true;
      moveNode(g.id, g.nx + w.x - g.sx, g.ny + w.y - g.sy);
    } else if (g.kind === "link") {
      const target = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest("[data-port-dir='in']");
      const bad = !!target && !compatible(g.type, target.getAttribute("data-port-type") as PortType);
      setLink((l) => l && { ...l, b: toWorld(e.clientX, e.clientY), bad });
    }
  };

  const onPointerUp = async (e: PointerEvent) => {
    const g = gesture.current;
    gesture.current = null;
    root.current?.releasePointerCapture(e.pointerId);
    if (!g) return;
    if (g.kind === "drag" && g.moved) {
      await commitLayout([g.id]);
    } else if (g.kind === "link") {
      const target = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest("[data-port-dir='in']");
      const to = target?.getAttribute("data-port");
      if (to && splitRef(to).node !== splitRef(g.from).node) {
        const ok = await connect(g.from, to);
        if (!ok) {
          // Blocked: flash the rejected edge red, then drop it.
          setLink((l) => l && { ...l, bad: true });
          setTimeout(() => setLink(null), 450);
          return;
        }
      }
      setLink(null);
    }
  };

  const onWheel = (e: WheelEvent) => {
    if (e.ctrlKey || e.metaKey) {
      const r = root.current!.getBoundingClientRect();
      const px = e.clientX - r.left;
      const py = e.clientY - r.top;
      setView((v) => {
        const zoom = Math.min(2.5, Math.max(0.15, v.zoom * Math.exp(-e.deltaY * 0.0015)));
        return { zoom, x: px - ((px - v.x) * zoom) / v.zoom, y: py - ((py - v.y) * zoom) / v.zoom };
      });
    } else {
      setView((v) => ({ ...v, x: v.x - e.deltaX, y: v.y - e.deltaY }));
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "Delete" && e.key !== "Backspace") return;
    if (selectedEdge) {
      const [from, to] = selectedEdge.split("->");
      void disconnect(from, to);
    } else if (selectedNode) {
      void removeNode(selectedNode);
    }
  };

  if (!flow) return null;
  const compact = view.zoom < LOD_ZOOM && !link;
  const visibleNodes = flow.nodes.filter((n) => intersects({ x: n.x, y: n.y, w: n.w, h: nodeHeight(n) }, viewport));

  return (
    <div
      ref={root}
      className="flow-canvas"
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onWheel={onWheel}
      onKeyDown={onKeyDown}
      style={{ backgroundPosition: `${view.x}px ${view.y}px`, backgroundSize: `${24 * view.zoom}px ${24 * view.zoom}px` }}
    >
      <div className="flow-world" style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})` }}>
        <EdgeLayer
          edges={flow.edges}
          nodes={nodeMap}
          viewport={viewport}
          selected={selectedEdge}
          onSelect={(k) => select(null, k)}
          pending={link && { a: link.a, b: link.b, bad: link.bad }}
          compact={compact}
        />
        {visibleNodes.map((n) => (
          <NodeCard
            key={n.id}
            node={n}
            compact={compact}
            selected={n.id === selectedNode}
            isEntry={flow.entry === n.id}
            dragType={link?.type ?? null}
            connected={connected}
            actions={renderActions?.(n)}
            status={renderStatus?.(n)}
            onStartDrag={onStartDrag}
            onStartLink={onStartLink}
          />
        ))}
      </div>
      <div className="flow-zoom" onPointerDown={(e) => e.stopPropagation()}>
        <button className="ghost" onClick={() => setView((v) => ({ ...v, zoom: Math.min(2.5, v.zoom * 1.2) }))}>
          ＋
        </button>
        <span>{Math.round(view.zoom * 100)}%</span>
        <button className="ghost" onClick={() => setView((v) => ({ ...v, zoom: Math.max(0.15, v.zoom / 1.2) }))}>
          −
        </button>
        <button className="ghost" onClick={fit} title="Fit to content">
          ⤢
        </button>
      </div>
      {flow.nodes.length === 0 && <div className="flow-empty">Add a node from the palette to start.</div>}
      {selectedEdge && <div className="flow-hint">Edge {selectedEdge.replace("->", " → ")} — press Delete to remove</div>}
    </div>
  );
}
