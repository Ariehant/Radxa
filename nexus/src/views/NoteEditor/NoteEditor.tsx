import { useStore } from "../../state/store";
import { Skeleton } from "../../components/Skeleton";
import { SourceEditor } from "./SourceEditor";

export function NoteEditor() {
  const doc = useStore((s) => s.doc);
  const loading = useStore((s) => s.docLoading);

  if (!doc) return loading ? <Skeleton lines={16} /> : <div className="empty">Select a file</div>;

  return (
    <div className="note-editor">
      <div className="note-header">
        <span className="note-path">{doc.path}</span>
      </div>
      <div className="note-body">
        <SourceEditor docKey={doc.path} value={doc.content} />
      </div>
    </div>
  );
}
