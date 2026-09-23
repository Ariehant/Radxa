export interface VaultInfo {
  root: string;
  name: string;
}

export interface TreeEntry {
  path: string;
  name: string;
  is_dir: boolean;
  depth: number;
}

export interface NoteContent {
  path: string;
  content: string;
  hash: string;
}

export interface Saved {
  path: string;
  hash: string;
}

export interface LinkRef {
  path: string;
  title: string;
  kind: "link" | "embed" | "relation";
  port: string;
}

export interface OutLink {
  target: string;
  kind: string;
  port: string;
  resolved: string | null;
}

export interface SearchHit {
  path: string;
  title: string;
  snippet: string;
}

export interface IndexStatus {
  pending: number;
  scanning: boolean;
  hot: number;
  stats: { files: number; nodes: number; edges: number };
}

export interface IndexUpdated {
  changed: string[];
  removed: string[];
  structural: boolean;
  pending: number;
  ms: number;
}

export type Priority = "open" | "viewport" | "recent" | "idle";

export type PortType =
  | "string"
  | "number"
  | "json"
  | "document"
  | "document[]"
  | "note_ref"
  | "note_ref[]"
  | "card"
  | "any";

export interface PortDef {
  name: string;
  type: PortType;
  description?: string;
}

export interface NodeView {
  id: string;
  ref: string;
  title: string;
  kind: string;
  config: Record<string, unknown> | null;
  inputs: PortDef[];
  outputs: PortDef[];
  x: number;
  y: number;
  w: number;
  h: number;
  missing_template: boolean;
}

export interface EdgeCheck {
  index: number;
  from: string;
  to: string;
  from_type: PortType | null;
  to_type: PortType | null;
  valid: boolean;
  reason: string | null;
}

export interface FlowView {
  dir: string;
  path: string;
  canvas_path: string;
  id: string | null;
  name: string;
  entry: string | null;
  description: string;
  nodes: NodeView[];
  edges: EdgeCheck[];
  errors: string[];
  order: string[] | null;
}

export interface FlowSummary {
  dir: string;
  name: string;
  nodes: number;
}

export interface TemplateSummary {
  ref: string;
  title: string;
  kind: string;
  description: string | null;
  inputs: PortDef[];
  outputs: PortDef[];
}

export interface Position {
  id: string;
  x: number;
  y: number;
  w?: number;
  h?: number;
}

export interface FlowNodeRow {
  id: string;
  title: string | null;
  ref: string;
  template_title: string | null;
  kind: string | null;
  config: Record<string, unknown> | null;
  x: number | null;
  y: number | null;
  inputs: number;
  outputs: number;
}

export interface FlowEdgeRow {
  from: string;
  to: string;
  data_type: string | null;
}

export interface FlowTable {
  nodes: FlowNodeRow[];
  edges: FlowEdgeRow[];
}
