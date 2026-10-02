import { useEffect, useMemo } from "react";
import { useStore } from "../../state/store";
import { Skeleton } from "../../components/Skeleton";
import { SourceEditor } from "./SourceEditor";
import { RichEditor } from "./RichEditor";
import { BacklinksPanel } from "./BacklinksPanel";
import { joinFrontmatter, splitFrontmatter } from "./markdown";

const TEXT_EXT = /\.(md|canvas|json|txt|toml|ya?ml)$/i;

export function NoteEditor() {
  const doc = useStore((s) => s.doc);
  const loading = useStore((s) => s.docLoading);
  const mode = useStore((s) => s.editorMode);
  const setMode = useStore((s) => s.setEditorMode);
  const editDoc = useStore((s) => s.editDoc);
  const saveDoc = useStore((s) => s.saveDoc);
  const reloadDoc = useStore((s) => s.reloadDoc);
  const openLink = useStore((s) => s.openLink);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void saveDoc();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [saveDoc]);

  // Split once per external content replacement, not per keystroke.
  const parts = useMemo(
    () => (doc ? splitFrontmatter(doc.content) : null),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [doc?.path, doc?.rev],
  );

  if (!doc || !parts) return loading ? <Skeleton lines={16} /> : <div className="empty">Select a file</div>;

  const isMarkdown = doc.path.toLowerCase().endsWith(".md");
  const effectiveMode = isMarkdown ? mode : "source";
  const key = `${doc.path}#${doc.rev}#${effectiveMode}`;

  if (!TEXT_EXT.test(doc.path)) {
    return <div className="empty">{doc.path} is not a text file.</div>;
  }

  return (
    <div className="note-editor">
      <div className="note-header">
        <span className="note-path">{doc.path}</span>
        <span className={"save-state" + (doc.conflict ? " conflict" : "")}>
          {doc.conflict ? "Changed on disk" : doc.saving ? "Saving…" : doc.dirty ? "Edited" : "Saved"}
        </span>
        {doc.conflict && (
          <>
            <button onClick={() => void reloadDoc()}>Reload</button>
            <button onClick={() => void saveDoc(true)}>Overwrite</button>
          </>
        )}
        {isMarkdown && (
          <div className="segmented">
            <button className={effectiveMode === "rich" ? "active" : ""} onClick={() => setMode("rich")}>
              Rich
            </button>
            <button className={effectiveMode === "source" ? "active" : ""} onClick={() => setMode("source")}>
              Source
            </button>
          </div>
        )}
      </div>
      <div className="note-body">
        {effectiveMode === "source" ? (
          <SourceEditor docKey={key} value={doc.content} onChange={editDoc} onOpenLink={openLink} />
        ) : (
          <div className="rich-pane">
            {parts.frontmatter !== null && (
              <details className="frontmatter">
                <summary>Properties</summary>
                <textarea
                  key={key}
                  spellCheck={false}
                  defaultValue={parts.frontmatter}
                  rows={Math.min(14, parts.frontmatter.split("\n").length + 1)}
                  onChange={(e) => {
                    parts.frontmatter = e.target.value;
                    editDoc(joinFrontmatter(parts.frontmatter, parts.body));
                  }}
                />
              </details>
            )}
            <RichEditor
              docKey={key}
              body={parts.body}
              onOpenLink={openLink}
              onChange={(body) => {
                parts.body = body;
                editDoc(joinFrontmatter(parts.frontmatter, body));
              }}
            />
          </div>
        )}
        <BacklinksPanel />
      </div>
    </div>
  );
}
