// Injected before the app loads in e2e runs: stands in for the Tauri runtime
// and forwards the single `rpc` command to the dev bridge (examples/dev_bridge.rs),
// which runs the real Rust backend. Events are long-polled.
(() => {
  const B = window.__NEXUS_BRIDGE__ || "http://127.0.0.1:7777";
  const callbacks = new Map();
  const listeners = {};
  let next = 1;
  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) {
      const id = next++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback(id) {
      callbacks.delete(id);
    },
    async invoke(cmd, args) {
      if (cmd === "rpc") {
        const r = await fetch(B + "/rpc", { method: "POST", body: JSON.stringify(args.request) });
        return r.json();
      }
      if (cmd === "plugin:event|listen") {
        (listeners[args.event] ||= []).push(args.handler);
        return args.handler;
      }
      if (cmd === "plugin:event|unlisten") return null;
      if (cmd === "plugin:dialog|open") return window.__NEXUS_PICK__ ?? null;
      throw new Error("unmocked tauri command " + cmd);
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  (async function poll() {
    let since = 0;
    for (;;) {
      try {
        const r = await fetch(B + "/events?since=" + since);
        const j = await r.json();
        since = j.next;
        for (const [name, payload] of j.events) for (const h of listeners[name] || []) callbacks.get(h)?.({ event: name, id: h, payload });
      } catch {
        await new Promise((res) => setTimeout(res, 300));
      }
    }
  })();
})();
