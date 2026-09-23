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
