import type { NodeView, PortType } from "../../api/tauri";

/** Mirrors `PortType::connects_to` in flow/types.rs (backend re-validates). */
export function compatible(from: PortType, to: PortType): boolean {
  return from === "any" || to === "any" || from === to || (from === "document" && to === "document[]");
}

export const TYPE_COLOR: Record<PortType, string> = {
  string: "#5b9dff",
  number: "#f5a524",
  json: "#a77bff",
  document: "#30a46c",
  "document[]": "#1f8a57",
  note_ref: "#e5484d",
  "note_ref[]": "#b8323a",
  card: "#12a5b8",
  any: "#8a90a0",
};

export const HEADER_H = 34;
export const ROW_H = 24;
export const PAD_B = 10;

export function nodeHeight(n: NodeView): number {
  return Math.max(n.h, HEADER_H + Math.max(n.inputs.length, n.outputs.length, 1) * ROW_H + PAD_B);
}

export function splitRef(ref: string): { node: string; port: string } {
  const i = ref.lastIndexOf(".");
  return { node: ref.slice(0, i), port: ref.slice(i + 1) };
}

/** World-space anchor of a port. */
export function portAnchor(n: NodeView, port: string, dir: "in" | "out"): { x: number; y: number } | null {
  const list = dir === "in" ? n.inputs : n.outputs;
  const i = list.findIndex((p) => p.name === port);
  if (i < 0) return null;
  return { x: dir === "in" ? n.x : n.x + n.w, y: n.y + HEADER_H + i * ROW_H + ROW_H / 2 };
}

export function bezier(a: { x: number; y: number }, b: { x: number; y: number }): string {
  const dx = Math.max(40, Math.abs(b.x - a.x) * 0.5);
  return `M ${a.x} ${a.y} C ${a.x + dx} ${a.y}, ${b.x - dx} ${b.y}, ${b.x} ${b.y}`;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function intersects(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}
