import { useStore } from "../state/store";

export function StatusBar() {
  const st = useStore((s) => s.indexStatus);
  if (!st) return <div className="statusbar" />;
  return (
    <div className="statusbar">
      <span>
        {st.scanning || st.pending > 0 ? `Indexing… ${st.pending} queued` : "Indexed"} · {st.stats.files} files ·{" "}
        {st.stats.edges} links
      </span>
    </div>
  );
}
