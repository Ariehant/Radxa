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
}
