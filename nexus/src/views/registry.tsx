import type { ReactNode } from "react";
import type { NodeView } from "../api/tauri";

/**
 * ViewRegistry (frontend half): renderers for a flow. The backend half
 * (src-tauri/src/registry/views.rs) registers the queries views read from.
 * Adding a layout = one `registerFlowView` call; FlowEditor has no per-view code.
 */
export interface FlowViewContext {
  renderActions: (n: NodeView) => ReactNode;
  renderStatus: (n: NodeView) => ReactNode;
}

export interface FlowViewDef {
  id: string;
  label: string;
  order: number;
  /** Show the node palette / inspector side panels around this view. */
  palette: boolean;
  inspector: boolean;
  render: (ctx: FlowViewContext) => ReactNode;
}

const flowViews = new Map<string, FlowViewDef>();

export function registerFlowView(def: FlowViewDef) {
  if (flowViews.has(def.id)) throw new Error(`flow view ${def.id} registered twice`);
  flowViews.set(def.id, def);
}

export function listFlowViews(): FlowViewDef[] {
  return [...flowViews.values()].sort((a, b) => a.order - b.order);
}

export function getFlowView(id: string): FlowViewDef | undefined {
  return flowViews.get(id);
}
