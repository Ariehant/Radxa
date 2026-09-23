/** Split `---\nyaml\n---\nbody`. Mirrors `index::parser::split_frontmatter`. */
export function splitFrontmatter(src: string): { frontmatter: string | null; body: string } {
  const s = src.startsWith("﻿") ? src.slice(1) : src;
  if (!s.startsWith("---\n")) return { frontmatter: null, body: s };
  const rest = s.slice(4);
  let offset = 0;
  for (const line of rest.split(/(?<=\n)/)) {
    const t = line.replace(/\r?\n$/, "");
    if (t === "---" || t === "...") {
      return { frontmatter: rest.slice(0, offset), body: rest.slice(offset + line.length) };
    }
    offset += line.length;
  }
  return { frontmatter: null, body: s };
}

export function joinFrontmatter(frontmatter: string | null, body: string): string {
  if (frontmatter === null) return body;
  const fm = frontmatter.endsWith("\n") || frontmatter === "" ? frontmatter : frontmatter + "\n";
  return `---\n${fm}---\n${body}`;
}

export interface WikilinkParts {
  target: string;
  alias: string | null;
  heading: string | null;
  embed: boolean;
}

/** `![[Target#Heading|Alias]]` → parts. */
export function parseWikilink(raw: string): WikilinkParts | null {
  const m = /^(!?)\[\[([^\]\n]+)\]\]$/.exec(raw);
  if (!m) return null;
  const [targetPart, alias = null] = m[2].split("|", 2);
  const [target, heading = null] = targetPart.split("#", 2);
  return { target: target.trim(), alias: alias?.trim() || null, heading: heading?.trim() || null, embed: m[1] === "!" };
}
