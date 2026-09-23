import { useEffect, useRef } from "react";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import Link from "@tiptap/extension-link";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import { Markdown } from "tiptap-markdown";
import { Wikilink } from "./wikilink";

interface Props {
  docKey: string;
  body: string;
  onChange: (body: string) => void;
  onOpenLink: (target: string) => void;
}

function getMarkdown(editor: { storage: Record<string, unknown> }): string {
  return (editor.storage.markdown as { getMarkdown: () => string }).getMarkdown();
}

/**
 * Notion-style block editor over the Markdown body. Roundtrip safety:
 * opening a note never rewrites it — until the user actually edits, the
 * original body is reported unchanged, and leading whitespace is preserved.
 */
export function RichEditor({ docKey, body, onChange, onOpenLink }: Props) {
  const baseline = useRef<string | null>(null);
  const original = useRef(body);
  const onChangeRef = useRef(onChange);
  const onOpenRef = useRef(onOpenLink);
  onChangeRef.current = onChange;
  onOpenRef.current = onOpenLink;

  const editor = useEditor(
    {
      extensions: [
        StarterKit,
        Link.configure({ openOnClick: false, autolink: false }),
        TaskList,
        TaskItem.configure({ nested: true }),
        Wikilink,
        Markdown.configure({
          html: true,
          tightLists: true,
          bulletListMarker: "-",
          linkify: false,
          breaks: false,
          transformPastedText: true,
          transformCopiedText: true,
        }),
      ],
      content: body,
      onCreate: ({ editor }) => {
        baseline.current = getMarkdown(editor);
        original.current = body;
      },
      onUpdate: ({ editor }) => {
        const md = getMarkdown(editor);
        if (md === baseline.current) {
          onChangeRef.current(original.current);
          return;
        }
        const lead = /^\s*/.exec(original.current)?.[0] ?? "";
        onChangeRef.current(lead + md.replace(/\s*$/, "") + "\n");
      },
      editorProps: {
        handleClick: (_view, _pos, event) => {
          const el = (event.target as HTMLElement).closest("[data-wikilink]");
          if (el) {
            onOpenRef.current(el.getAttribute("data-target") ?? "");
            return true;
          }
          return false;
        },
      },
    },
    [docKey],
  );

  useEffect(() => () => editor?.destroy(), [editor]);

  return <EditorContent className="rich-editor" editor={editor} />;
}
