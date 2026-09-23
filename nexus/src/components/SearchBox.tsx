import { useEffect, useRef, useState } from "react";
import { api, type SearchHit } from "../api/tauri";
import { useStore } from "../state/store";

export function SearchBox() {
  const [q, setQ] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [open, setOpen] = useState(false);
  const openFile = useStore((s) => s.openFile);
  const seq = useRef(0);

  useEffect(() => {
    const query = q.trim();
    if (!query) {
      setHits([]);
      return;
    }
    const mine = ++seq.current;
    const t = setTimeout(async () => {
      try {
        const res = await api.search(query, 30);
        if (mine === seq.current) setHits(res);
      } catch {
        /* ignore transient errors while typing */
      }
    }, 80);
    return () => clearTimeout(t);
  }, [q]);

  return (
    <div className="search" onBlur={() => setTimeout(() => setOpen(false), 150)}>
      <input
        placeholder="Search vault…"
        value={q}
        onFocus={() => setOpen(true)}
        onChange={(e) => {
          setQ(e.target.value);
          setOpen(true);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && hits[0]) {
            void openFile(hits[0].path);
            setOpen(false);
          } else if (e.key === "Escape") setOpen(false);
        }}
      />
      {open && hits.length > 0 && (
        <div className="search-results">
          {hits.map((h) => (
            <div
              key={h.path}
              className="search-hit"
              onMouseDown={() => {
                void openFile(h.path);
                setOpen(false);
              }}
            >
              <div className="search-title">{h.title || h.path}</div>
              {h.snippet && <div className="search-snippet">{h.snippet}</div>}
              <div className="search-path">{h.path}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
