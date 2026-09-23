import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "./state/store";
import { FileTree } from "./views/FileTree/FileTree";
import { NoteEditor } from "./views/NoteEditor/NoteEditor";

async function pickFolder(title: string): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

function Welcome() {
  const openVault = useStore((s) => s.openVault);
  return (
    <div className="welcome">
      <h1>Nexus</h1>
      <p>Knowledge + Workflow OS. Your vault is plain Markdown and JSON Canvas on disk.</p>
      <div className="welcome-actions">
        <button
          className="primary"
          onClick={async () => {
            const p = await pickFolder("Open vault");
            if (p) await openVault(p);
          }}
        >
          Open vault…
        </button>
        <button
          onClick={async () => {
            const p = await pickFolder("Create vault in folder");
            if (p) await openVault(p, true);
          }}
        >
          Create vault…
        </button>
      </div>
    </div>
  );
}

export default function App() {
  const vault = useStore((s) => s.vault);
  const error = useStore((s) => s.error);
  const setError = useStore((s) => s.setError);
  const closeVault = useStore((s) => s.closeVault);

  return (
    <div className="app">
      {error && (
        <div className="toast error" onClick={() => setError(null)}>
          {error}
        </div>
      )}
      {!vault ? (
        <Welcome />
      ) : (
        <div className="workspace">
          <aside className="sidebar">
            <div className="sidebar-header">
              <span className="vault-name" title={vault.root}>
                {vault.name}
              </span>
              <button className="ghost" onClick={closeVault} title="Close vault">
                ✕
              </button>
            </div>
            <FileTree />
          </aside>
          <main className="main">
            <NoteEditor />
          </main>
        </div>
      )}
    </div>
  );
}
