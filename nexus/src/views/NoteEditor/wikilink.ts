import { Node, InputRule, mergeAttributes } from "@tiptap/core";
import type MarkdownIt from "markdown-it";
import { parseWikilink } from "./markdown";

/**
 * Inline atom for `[[target|alias]]` / `![[embed]]`. The raw source is kept
 * verbatim and written back unchanged, so wikilinks survive a TipTap
 * roundtrip byte-for-byte (prosemirror-markdown would otherwise escape `[`).
 */
export const Wikilink = Node.create({
  name: "wikilink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,

  addAttributes() {
    return { raw: { default: "" } };
  },

  parseHTML() {
    return [{ tag: "span[data-wikilink]", getAttrs: (el) => ({ raw: (el as HTMLElement).getAttribute("data-wikilink") }) }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const parts = parseWikilink(node.attrs.raw as string);
    const label = parts ? (parts.alias ?? parts.target + (parts.heading ? ` › ${parts.heading}` : "")) : node.attrs.raw;
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-wikilink": node.attrs.raw,
        "data-target": parts?.target ?? "",
        class: parts?.embed ? "wikilink embed" : "wikilink",
      }),
      parts?.embed ? `⧉ ${label}` : label,
    ];
  },

  renderText({ node }) {
    return node.attrs.raw as string;
  },

  addInputRules() {
    return [
      new InputRule({
        find: /(!?\[\[[^\]\n]+\]\])$/,
        handler: ({ state, range, match }) => {
          state.tr.replaceWith(range.from, range.to, this.type.create({ raw: match[1] }));
        },
      }),
    ];
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: { write: (s: string) => void }, node: { attrs: { raw: string } }) {
          state.write(node.attrs.raw);
        },
        parse: {
          setup(md: MarkdownIt) {
            md.inline.ruler.before("link", "wikilink", (st, silent) => {
              const start = st.pos;
              const embed = st.src.charCodeAt(start) === 0x21; /* ! */
              const open = start + (embed ? 1 : 0);
              if (st.src.charCodeAt(open) !== 0x5b || st.src.charCodeAt(open + 1) !== 0x5b) return false;
              const close = st.src.indexOf("]]", open + 2);
              if (close < 0) return false;
              const raw = st.src.slice(start, close + 2);
              if (raw.includes("\n") || !parseWikilink(raw)) return false;
              if (!silent) {
                const tok = st.push("wikilink", "span", 0);
                tok.content = raw;
              }
              st.pos = close + 2;
              return true;
            });
            md.renderer.rules.wikilink = (tokens, idx) =>
              `<span data-wikilink="${md.utils.escapeHtml(tokens[idx].content)}"></span>`;
          },
        },
      },
    };
  },
});
