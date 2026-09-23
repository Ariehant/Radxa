// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { Markdown } from "tiptap-markdown";
import { Wikilink } from "../views/NoteEditor/wikilink";
import { joinFrontmatter, parseWikilink, splitFrontmatter } from "../views/NoteEditor/markdown";

function roundtrip(md: string): string {
  const editor = new Editor({
    extensions: [StarterKit, Wikilink, Markdown.configure({ html: true, tightLists: true, bulletListMarker: "-", linkify: false })],
    content: md,
  });
  const out = (editor.storage.markdown as { getMarkdown: () => string }).getMarkdown();
  editor.destroy();
  return out;
}

describe("frontmatter", () => {
  it("split + join is lossless", () => {
    const src = "---\nid: 1\ntitle: \"A: b\"\ntags: [x, y]\n---\n\n# Body\n\ntext\n";
    const { frontmatter, body } = splitFrontmatter(src);
    expect(frontmatter).toBe("id: 1\ntitle: \"A: b\"\ntags: [x, y]\n");
    expect(joinFrontmatter(frontmatter, body)).toBe(src);
    expect(joinFrontmatter(null, "plain")).toBe("plain");
    expect(splitFrontmatter("no fm").frontmatter).toBeNull();
  });
});

describe("wikilinks", () => {
  it("parses parts", () => {
    expect(parseWikilink("![[Target#H|Alias]]")).toEqual({ target: "Target", heading: "H", alias: "Alias", embed: true });
    expect(parseWikilink("[[x]]")?.embed).toBe(false);
    expect(parseWikilink("[x]")).toBeNull();
  });

  it("survive a TipTap markdown roundtrip byte-for-byte", () => {
    const md = "# Title\n\nSee [[Rust Ownership]], [[notes/a|alias]] and ![[diagram.png]].\n\n- item with [[Link#Heading]]\n- two";
    expect(roundtrip(md)).toBe(md);
  });

  it("is stable: a second roundtrip changes nothing", () => {
    const md = "Some *emphasis*, **bold**, `code` and [[x]].\n\n1. one\n2. two\n\n> quote\n\n```rust\nfn main() {}\n```";
    const once = roundtrip(md);
    expect(roundtrip(once)).toBe(once);
    expect(once).toContain("[[x]]");
  });
});
