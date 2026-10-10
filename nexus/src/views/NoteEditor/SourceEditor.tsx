import { useEffect, useRef } from "react";
import { EditorView, basicSetup } from "codemirror";
import { EditorState } from "@codemirror/state";
import { Decoration, MatchDecorator, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { markdown } from "@codemirror/lang-markdown";
import { tags } from "@lezer/highlight";
import { parseWikilink } from "./markdown";

interface Props {
  /** Changing `docKey` replaces the document; `value` alone is only the initial value. */
  docKey: string;
  value: string;
  onChange?: (value: string) => void;
  onOpenLink?: (target: string) => void;
}

const wikilinkMatcher = new MatchDecorator({
  regexp: /!?\[\[[^\]\n]+\]\]/g,
  decoration: (m) => Decoration.mark({ class: "cm-wikilink", attributes: { "data-wikilink": m[0] } }),
});

const wikilinks = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = wikilinkMatcher.createDeco(view);
    }
    update(u: ViewUpdate) {
      this.decorations = wikilinkMatcher.updateDeco(u, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

/** Obsidian-flavoured live styling: headings scale, emphasis renders. */
const livePreview = HighlightStyle.define([
  { tag: tags.heading1, fontSize: "1.6em", fontWeight: "600" },
  { tag: tags.heading2, fontSize: "1.35em", fontWeight: "600" },
  { tag: tags.heading3, fontSize: "1.15em", fontWeight: "600" },
  { tag: tags.strong, fontWeight: "600" },
  { tag: tags.emphasis, fontStyle: "italic" },
  { tag: tags.strikethrough, textDecoration: "line-through" },
  { tag: tags.link, color: "var(--accent)" },
  { tag: tags.url, color: "var(--fg-3)" },
  { tag: tags.monospace, fontFamily: "var(--mono)", color: "var(--fg-2)" },
  { tag: tags.processingInstruction, color: "var(--fg-3)" },
  { tag: tags.meta, color: "var(--fg-3)" },
]);

/** CodeMirror 6 source editor. Ctrl/Cmd-click a wikilink to follow it. */
export function SourceEditor({ docKey, value, onChange, onOpenLink }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const cbs = useRef({ onChange, onOpenLink });
  cbs.current = { onChange, onOpenLink };

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          markdown(),
          syntaxHighlighting(livePreview),
          wikilinks,
          EditorView.lineWrapping,
          EditorView.domEventHandlers({
            mousedown: (e) => {
              if (!(e.ctrlKey || e.metaKey)) return false;
              const el = (e.target as HTMLElement).closest("[data-wikilink]");
              const parts = el && parseWikilink(el.getAttribute("data-wikilink") ?? "");
              if (parts) {
                e.preventDefault();
                cbs.current.onOpenLink?.(parts.target);
                return true;
              }
              return false;
            },
          }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) cbs.current.onChange?.(u.state.doc.toString());
          }),
        ],
      }),
    });
    return () => v.destroy();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docKey]);

  return <div className="source-editor" ref={host} />;
}
