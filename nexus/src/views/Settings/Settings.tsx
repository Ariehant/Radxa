import { useEffect, useState } from "react";
import { api, type Config } from "../../api/tauri";
import { useStore } from "../../state/store";
import { applyTheme } from "../../state/theme";
import { Skeleton } from "../../components/Skeleton";

export function Settings() {
  const [cfg, setCfg] = useState<Config | null>(null);
  const [saved, setSaved] = useState<Config | null>(null);
  const [providers, setProviders] = useState<{ id: string; label: string }[]>([]);
  const [models, setModels] = useState<string[] | null>(null);
  const [probe, setProbe] = useState<string | null>(null);
  const [rebuilding, setRebuilding] = useState(false);
  const setError = useStore((s) => s.setError);
  const status = useStore((s) => s.indexStatus);

  useEffect(() => {
    void api.config.get().then((c) => {
      setCfg(c);
      setSaved(c);
    });
    void api.llm.providers().then(setProviders).catch(() => setProviders([{ id: "ollama", label: "Ollama" }]));
  }, []);

  if (!cfg) return <Skeleton lines={10} />;
  const dirty = JSON.stringify(cfg) !== JSON.stringify(saved);
  const set = (patch: (c: Config) => Config) => setCfg((c) => (c ? patch(structuredClone(c)) : c));

  const save = async () => {
    try {
      const c = await api.config.set(cfg);
      setSaved(c);
      applyTheme(c.ui.theme);
    } catch (e) {
      setError(String(e));
    }
  };

  const testConnection = async () => {
    setProbe("Connecting…");
    setModels(null);
    try {
      if (dirty) await save();
      const m = await api.llm.models();
      setModels(m);
      setProbe(m.length ? `Connected — ${m.length} model${m.length === 1 ? "" : "s"} available` : "Connected, but no models are installed");
    } catch (e) {
      setProbe(String(e));
    }
  };

  return (
    <div className="settings">
      <h2>Settings</h2>
      <p className="muted">Stored per vault in <code>.nexus/config.toml</code>.</p>

      <section>
        <h3>LLM provider</h3>
        <label className="field">
          <span>Provider</span>
          <select value={cfg.llm.provider} onChange={(e) => set((c) => ((c.llm.provider = e.target.value), c))}>
            {providers.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>Base URL</span>
          <input value={cfg.llm.base_url} onChange={(e) => set((c) => ((c.llm.base_url = e.target.value), c))} />
        </label>
        <label className="field">
          <span>Default model</span>
          <input list="nexus-models" value={cfg.llm.model} onChange={(e) => set((c) => ((c.llm.model = e.target.value), c))} />
          <datalist id="nexus-models">{models?.map((m) => <option key={m} value={m} />)}</datalist>
        </label>
        <label className="field">
          <span>API key (optional — Ollama needs none)</span>
          <input
            type="password"
            autoComplete="off"
            value={cfg.llm.api_key ?? ""}
            onChange={(e) => set((c) => ((c.llm.api_key = e.target.value || null), c))}
          />
        </label>
        <div className="row">
          <label className="field">
            <span>Timeout (s)</span>
            <input
              type="number"
              min={5}
              value={cfg.llm.timeout_secs}
              onChange={(e) => set((c) => ((c.llm.timeout_secs = Number(e.target.value) || 60), c))}
            />
          </label>
          <label className="field">
            <span>Max tool steps</span>
            <input
              type="number"
              min={1}
              max={20}
              value={cfg.llm.max_tool_steps}
              onChange={(e) => set((c) => ((c.llm.max_tool_steps = Number(e.target.value) || 6), c))}
            />
          </label>
        </div>
        <div className="row">
          <button onClick={() => void testConnection()}>Test connection</button>
          {probe && <span className="muted">{probe}</span>}
        </div>
      </section>

      <section>
        <h3>Appearance</h3>
        <div className="segmented">
          {(["system", "light", "dark"] as const).map((t) => (
            <button
              key={t}
              className={cfg.ui.theme === t ? "active" : ""}
              onClick={() => {
                set((c) => ((c.ui.theme = t), c));
                applyTheme(t);
              }}
            >
              {t[0].toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>
      </section>

      <section>
        <h3>Version control</h3>
        <label className="check">
          <input type="checkbox" checked={cfg.git.auto_commit} onChange={(e) => set((c) => ((c.git.auto_commit = e.target.checked), c))} />
          Auto-commit every save when the vault is a git repository (applies on next open)
        </label>
      </section>

      <section>
        <h3>Index</h3>
        <p className="muted">
          The index is derived and disposable: {status?.stats.files ?? 0} files, {status?.stats.edges ?? 0} links. Rebuilding deletes it and
          re-reads every file.
        </p>
        <button
          disabled={rebuilding}
          onClick={async () => {
            setRebuilding(true);
            try {
              await api.index.rebuild();
            } finally {
              setTimeout(() => setRebuilding(false), 800);
            }
          }}
        >
          {rebuilding ? "Rebuilding…" : "Rebuild index"}
        </button>
      </section>

      <div className="settings-actions">
        <button className="primary" disabled={!dirty} onClick={() => void save()}>
          Save settings
        </button>
        {dirty && (
          <button
            onClick={() => {
              setCfg(saved);
              if (saved) applyTheme(saved.ui.theme);
            }}
          >
            Discard
          </button>
        )}
      </div>
    </div>
  );
}
