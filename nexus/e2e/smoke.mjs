// End-to-end smoke test: real Rust backend (dev bridge) + real React UI in
// headless Chromium. Usage: node e2e/smoke.mjs [screenshotDir]
//
// Requires: `cargo build --example dev_bridge` (in src-tauri) and a Chromium
// (PLAYWRIGHT_BROWSERS_PATH or CHROMIUM_PATH).
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, mkdirSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const shots = process.argv[2] || join(tmpdir(), "nexus-e2e");
mkdirSync(shots, { recursive: true });
const BRIDGE_PORT = 7788;
const UI_PORT = 1430;

function findChromium() {
  if (process.env.CHROMIUM_PATH) return process.env.CHROMIUM_PATH;
  const base = process.env.PLAYWRIGHT_BROWSERS_PATH || "/opt/pw-browsers";
  for (const d of readdirSync(base).filter((d) => d.startsWith("chromium-"))) {
    for (const p of ["chrome-linux/chrome", "chrome-win/chrome.exe"]) if (existsSync(join(base, d, p))) return join(base, d, p);
  }
  if (existsSync(join(base, "chromium"))) return join(base, "chromium");
  throw new Error("no chromium found");
}

const procs = [];
function start(cmd, args, opts = {}) {
  const p = spawn(cmd, args, { cwd: root, stdio: ["ignore", "pipe", "pipe"], ...opts });
  procs.push(p);
  return p;
}
function waitFor(p, re) {
  return new Promise((res, rej) => {
    const t = setTimeout(() => rej(new Error("timeout waiting for " + re)), 60_000);
    const on = (d) => {
      if (re.test(String(d))) {
        clearTimeout(t);
        res();
      }
    };
    p.stdout.on("data", on);
    p.stderr.on("data", on);
    p.on("exit", (c) => rej(new Error(`process exited ${c}`)));
  });
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(fn, what, ms = 8000) {
  const t0 = Date.now();
  for (;;) {
    try {
      const v = await fn();
      if (v) return v;
    } catch {
      /* retry */
    }
    if (Date.now() - t0 > ms) throw new Error("timed out: " + what);
    await sleep(100);
  }
}

let failures = 0;
function check(cond, msg) {
  console.log(`${cond ? "✓" : "✗"} ${msg}`);
  if (!cond) failures++;
}

// NEXUS_E2E_PROD=1: release backend + production frontend bundle, and the
// spec §9 perf targets become hard assertions (dev builds only report them).
const PROD = !!process.env.NEXUS_E2E_PROD;
const exe = join(root, `src-tauri/target/${PROD ? "release" : "debug"}/examples`, process.platform === "win32" ? "dev_bridge.exe" : "dev_bridge");
const bridge = start(exe, [String(BRIDGE_PORT)]);
const viteBin = join(root, "node_modules/vite/bin/vite.js");
const vite = start(process.execPath, PROD ? [viteBin, "preview", "--port", String(UI_PORT), "--strictPort"] : [viteBin, "--port", String(UI_PORT), "--strictPort"]);
function perf(ok, msg) {
  if (PROD) check(ok, msg);
  else console.log(`${ok ? "✓" : "·"} ${msg} [dev build: informational]`);
}

try {
  await Promise.all([waitFor(bridge, /dev bridge on/), waitFor(vite, /Local:/)]);
  const vault = mkdtempSync(join(tmpdir(), "nexus-vault-"));
  const browser = await chromium.launch({ executablePath: findChromium() });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const consoleErrors = [];
  page.on("console", (m) => m.type() === "error" && consoleErrors.push(m.text()));
  page.on("pageerror", (e) => consoleErrors.push(String(e)));
  await page.addInitScript(`window.__NEXUS_BRIDGE__ = "http://127.0.0.1:${BRIDGE_PORT}"; window.__NEXUS_PICK__ = ${JSON.stringify(vault)};`);
  await page.addInitScript({ path: join(here, "tauri-mock.js") });

  // --- M1: create vault, tree renders notes/ and flows/
  const t0 = Date.now();
  await page.goto(`http://localhost:${UI_PORT}/`);
  await page.getByText("Create vault…").click();
  await page.locator(".tree-row", { hasText: "welcome.md" }).waitFor();
  perf(Date.now() - t0 < 3000, `vault open → first paint in ${Date.now() - t0} ms (< 3 s)`);
  check(await page.locator(".tree-row", { hasText: "deploy-pipeline" }).isVisible(), "file tree shows flows/");
  await page.screenshot({ path: join(shots, "01-vault.png") });

  // --- M2/M3: open note (rich), backlinks, edit in source, save, reopen identical
  await page.locator(".tree-row", { hasText: "getting-started.md" }).click();
  await page.locator(".rich-editor .ProseMirror").waitFor();
  await until(async () => (await page.locator(".backlink").count()) > 0, "backlinks");
  check((await page.locator(".backlink-title").first().textContent()) === "Welcome to Nexus", "backlinks panel lists Welcome to Nexus");
  await page.screenshot({ path: join(shots, "02-rich-editor.png") });

  await page.locator(".tree-row", { hasText: "welcome.md" }).click();
  await page.getByRole("button", { name: "Source" }).click();
  await page.locator(".cm-content").click();
  await page.keyboard.press("Control+End");
  await page.keyboard.type("\nAdded from e2e: [[Release Notes]]\n");
  await until(async () => (await page.locator(".save-state").textContent()) === "Saved", "autosave");
  const onDisk = readFileSync(join(vault, "notes/welcome.md"), "utf8");
  check(onDisk.includes("Added from e2e: [[Release Notes]]"), "edit saved atomically to disk");
  await page.locator(".tree-row", { hasText: "release-notes.md" }).click();
  await until(async () => (await page.locator(".backlink-title", { hasText: "Welcome to Nexus" }).count()) > 0, "new backlink");
  check(true, "new wikilink shows up as a backlink after save");
  await page.locator(".tree-row", { hasText: "welcome.md" }).click();
  await page.locator(".cm-content").waitFor();
  const reopened = await page.evaluate(async () => {
    const r = await window.__TAURI_INTERNALS__.invoke("rpc", { request: { jsonrpc: "2.0", id: 1, method: "note.read", params: { path: "notes/welcome.md" } } });
    return r.result.content;
  });
  check(reopened === onDisk, "reopen → identical content");
  await page.screenshot({ path: join(shots, "03-source-editor.png") });

  // Rich mode never rewrites an untouched note.
  await page.getByRole("button", { name: "Rich" }).click();
  await page.locator(".rich-editor .ProseMirror").waitFor();
  await sleep(1200);
  check(readFileSync(join(vault, "notes/welcome.md"), "utf8") === onDisk, "opening in rich mode does not rewrite the file");

  // --- M4: flow canvas
  await page.locator(".tree-row", { hasText: "deploy-pipeline" }).click();
  const t1 = Date.now();
  await page.locator(".tree-row", { hasText: "flow.md" }).click();
  await page.locator(".node-card").first().waitFor();
  perf(Date.now() - t1 < 1000, `flow canvas open in ${Date.now() - t1} ms`);
  check((await page.locator(".node-card").count()) === 3, "3 nodes rendered from flow.md at flow.canvas positions");
  check((await page.locator(".edge-line").count()) === 2, "2 typed edges rendered");
  await page.screenshot({ path: join(shots, "04-flow-canvas.png") });

  const mdBefore = readFileSync(join(vault, "flows/deploy-pipeline/flow.md"), "utf8");
  const header = page.locator(".node-card[data-node='n2'] .node-header");
  const hb = await header.boundingBox();
  await page.mouse.move(hb.x + 40, hb.y + 10);
  await page.mouse.down();
  await page.mouse.move(hb.x + 40, hb.y + 200, { steps: 8 });
  await page.mouse.up();
  await until(() => JSON.parse(readFileSync(join(vault, "flows/deploy-pipeline/flow.canvas"), "utf8")).nodes.find((n) => n.id === "n2").y > 150, "canvas saved");
  check(readFileSync(join(vault, "flows/deploy-pipeline/flow.md"), "utf8") === mdBefore, "dragging a node updates flow.canvas only");

  // Remove edge n2→n3 then re-create it by dragging port to port.
  await page.locator(".edge-hit").nth(1).dispatchEvent("pointerdown");
  await page.locator(".flow-canvas").press("Delete");
  await until(() => !readFileSync(join(vault, "flows/deploy-pipeline/flow.md"), "utf8").includes("n3.content"), "edge removed");
  check((await page.locator(".edge-line").count()) === 1, "edge deleted from flow.md");

  const out = await page.locator("[data-port='n2.out']").boundingBox();
  const inp = await page.locator("[data-port='n3.content']").boundingBox();
  await page.mouse.move(out.x + 6, out.y + 6);
  await page.mouse.down();
  await page.mouse.move(inp.x + 6, inp.y + 6, { steps: 10 });
  await page.mouse.up();
  await until(() => readFileSync(join(vault, "flows/deploy-pipeline/flow.md"), "utf8").includes("n3.content"), "edge re-created");
  check(true, "port → port drag creates a typed edge in flow.md");

  // Incompatible: n2.out (string) → n2.in on another llm node (document[]) is blocked.
  await page.locator(".palette-item", { hasText: "LLM Summarize" }).click();
  await page.locator(".node-card[data-node='n4']").waitFor();
  const md2 = readFileSync(join(vault, "flows/deploy-pipeline/flow.md"), "utf8");
  const o2 = await page.locator("[data-port='n2.out']").boundingBox();
  const i4 = await page.locator("[data-port='n4.in']").boundingBox();
  await page.mouse.move(o2.x + 6, o2.y + 6);
  await page.mouse.down();
  await page.mouse.move(i4.x + 6, i4.y + 6, { steps: 10 });
  check(await page.locator(".port.bad").count() > 0, "incompatible target port highlighted red while dragging");
  await page.screenshot({ path: join(shots, "05-type-mismatch.png") });
  await page.mouse.up();
  await sleep(400);
  const md3 = readFileSync(join(vault, "flows/deploy-pipeline/flow.md"), "utf8");
  check(!md3.includes("n4.in") && md3.split("edges:")[1] === md2.split("edges:")[1], "incompatible edge blocked, not written");
  check((await page.locator(".toast.error").textContent())?.includes("Type mismatch"), "type mismatch reported");

  // Reopen flow: preserved.
  await page.locator(".tree-row", { hasText: "welcome.md" }).click();
  await page.locator(".tree-row", { hasText: "flow.md" }).click();
  await page.locator(".node-card[data-node='n4']").waitFor();
  check((await page.locator(".node-card").count()) === 4 && (await page.locator(".edge-line").count()) === 2, "flow reopened with nodes + edges preserved");

  // --- M5: table + hybrid views over the same state, no reload
  let loads = 0;
  page.on("request", (r) => r.method() === "POST" && r.postData()?.includes('"flow.load"') && loads++);
  await page.getByRole("button", { name: "Table" }).click();
  await page.locator(".nodes-grid:not(.grid-head)").first().waitFor();
  check((await page.locator(".nodes-grid:not(.grid-head)").count()) === 4, "table view lists 4 node rows (from SQLite)");
  check((await page.locator(".edges-grid:not(.grid-head)").count()) === 2, "table view lists 2 edge rows");
  await page.screenshot({ path: join(shots, "06-table.png") });
  await page.getByRole("button", { name: "Hybrid" }).click();
  await page.locator(".hybrid .node-card").first().waitFor();
  await page.locator(".nodes-grid:not(.grid-head)", { hasText: "n2" }).click();
  check(await page.locator(".node-card[data-node='n2']").evaluate((el) => el.classList.contains("selected")), "selecting a table row selects the canvas node (shared state)");
  await page.screenshot({ path: join(shots, "07-hybrid.png") });
  await page.getByRole("button", { name: "Canvas" }).click();
  await page.locator(".node-card").first().waitFor();
  check(loads === 0, `switching views did not reload the flow (${loads} flow.load calls)`);

  // --- M6: execution (offline `echo` provider so no model is needed)
  await page.evaluate(async () => {
    const rpc = (method, params) => window.__TAURI_INTERNALS__.invoke("rpc", { request: { jsonrpc: "2.0", id: 9, method, params } });
    const cfg = (await rpc("config.get", {})).result;
    cfg.llm.provider = "echo";
    await rpc("config.set", cfg);
  });
  await page.locator(".node-card[data-node='n2'] .run-btn").click();
  await until(async () => (await page.locator(".node-card[data-node='n2'] .status-ok").count()) === 1, "n2 ran");
  check((await page.locator(".node-card[data-node='n1'] .status-ok").count()) === 1, "running n2 ran its upstream n1 first");
  check((await page.locator(".node-card[data-node='n3'] .node-status").count()) === 0, "downstream n3 untouched by running n2");
  await page.getByRole("button", { name: "▶ Run flow" }).click();
  await until(async () => (await page.getByRole("button", { name: "▶ Run flow" }).isEnabled()) && (await page.locator(".node-card[data-node='n3'] .status-ok").count()) === 1, "flow ran");
  check((await page.locator(".node-card[data-node='n2'] .status-cached").count()) === 1, "second run reused n2's cached output");
  check(existsSync(join(vault, "notes/generated/release-summary.md")), "write_note node generated a note in the knowledge base");
  const runs = readdirSync(join(vault, "runs/deploy-pipeline"));
  check(runs.length === 2 && runs.every((f) => f.endsWith(".md")), `run logs written to runs/deploy-pipeline/ (${runs.length})`);
  await page.locator(".node-card[data-node='n2'] .node-header").click();
  await page.locator(".inspector .output pre").first().waitFor();
  check((await page.locator(".inspector .output pre").first().textContent()).startsWith("[echo:llama3.2]"), "inspector shows the agent output");
  await page.screenshot({ path: join(shots, "08-run.png") });
  await page.getByRole("button", { name: /Last run log/ }).click();
  await page.locator(".note-path", { hasText: "runs/deploy-pipeline/" }).waitFor();
  check(true, "run log opens as a note");
  await page.screenshot({ path: join(shots, "09-run-log.png") });

  // --- M7: settings, theme, perf targets
  await page.getByTitle("Settings").click();
  await page.getByRole("heading", { name: "Settings" }).waitFor();
  await page.getByRole("button", { name: "Dark" }).click();
  check((await page.evaluate(() => document.documentElement.dataset.theme)) === "dark", "theme switch applies instantly");
  await page.screenshot({ path: join(shots, "10-settings-dark.png") });
  await page.getByRole("button", { name: "Save settings" }).click();
  await until(() => readFileSync(join(vault, ".nexus/config.toml"), "utf8").includes('theme = "dark"'), "config saved");
  check(true, "settings persisted to .nexus/config.toml");
  await page.getByRole("button", { name: "Light" }).click();
  await page.getByRole("button", { name: "Save settings" }).click();

  // 500-node flow written straight to disk (external edit → watcher → index).
  const { writeFileSync, mkdirSync: mk } = await import("node:fs");
  const N = 500;
  const nodes = [], edges = [], cnodes = [];
  for (let i = 1; i <= N; i++) {
    nodes.push(`  - id: n${i}\n    ref: templates/nodes/${i === 1 ? "fetch-data" : "llm-summarize"}.md`);
    if (i > 1) edges.push(`  - { from: n1.out, to: n${i}.in, type: "document[]" }`);
    cnodes.push({ id: `n${i}`, type: "text", x: (i % 25) * 300, y: Math.floor(i / 25) * 200, width: 240, height: 120, text: "" });
  }
  mk(join(vault, "flows/big"), { recursive: true });
  writeFileSync(join(vault, "flows/big/flow.md"), `---\ntype: flow\nname: big\nnodes:\n${nodes.join("\n")}\nedges:\n${edges.join("\n")}\n---\n`);
  writeFileSync(join(vault, "flows/big/flow.canvas"), JSON.stringify({ nodes: cnodes, edges: [] }));
  await page.locator(".tree-row[title='flows/big']").waitFor({ timeout: 10000 });
  await page.locator(".tree-row[title='flows/big']").click();
  await page.locator(".tree-row[title='flows/big/flow.md']").waitFor();
  // Time click → first painted node card entirely inside the page.
  const openMs = await page.evaluate(
    () =>
      new Promise((res) => {
        const t0 = performance.now();
        document.querySelector(".tree-row[title='flows/big/flow.md']").click();
        const tick = () => {
          const cards = document.querySelectorAll(".node-card");
          if (document.querySelector(".flow-name")?.textContent === "big" && cards.length > 0) requestAnimationFrame(() => res(performance.now() - t0));
          else requestAnimationFrame(tick);
        };
        tick();
      }),
  );
  perf(openMs < 200, `500-node flow canvas open → painted: ${openMs.toFixed(0)} ms (target < 200 ms)`);
  check((await page.locator(".node-card.compact").count()) > 0, "zoomed-out canvas renders low-detail cards");
  check((await page.locator(".flow-errors").count()) === 0, "500-node fan-out flow validates cleanly");
  // Zoom in on one corner: only on-screen nodes stay mounted.
  for (let i = 0; i < 4; i++) await page.locator(".flow-zoom button").first().click();
  const rendered = await page.locator(".node-card").count();
  check(rendered > 0 && rendered < N / 2, `viewport culling: ${rendered}/${N} node cards mounted when zoomed in`);
  await page.screenshot({ path: join(shots, "11-big-flow.png") });
  const tableMs = await page.evaluate(
    () =>
      new Promise((res) => {
        const t0 = performance.now();
        [...document.querySelectorAll(".segmented button")].find((b) => b.textContent === "Table").click();
        const tick = () => {
          if (document.querySelectorAll(".nodes-grid:not(.grid-head)").length > 0) requestAnimationFrame(() => res(performance.now() - t0));
          else requestAnimationFrame(tick);
        };
        tick();
      }),
  );
  perf(tableMs < 100, `500-row table view query → painted: ${tableMs.toFixed(0)} ms (target < 100 ms)`);
  check((await page.locator(".panel-title", { hasText: "Nodes" }).textContent()).includes("500"), "table view has all 500 rows (virtualised)");
  await page.getByRole("button", { name: "Canvas" }).click();

  if (process.env.NEXUS_E2E_EXTRA) {
    const extra = await import(process.env.NEXUS_E2E_EXTRA);
    await extra.default({ page, vault, check, until, sleep, shots });
  }

  check(consoleErrors.length === 0, `no console errors${consoleErrors.length ? ": " + consoleErrors.join(" | ") : ""}`);
  await browser.close();
} catch (e) {
  console.error(e);
  failures++;
} finally {
  for (const p of procs) p.kill();
}
console.log(failures === 0 ? "\nE2E PASSED" : `\nE2E FAILED (${failures})`);
console.log("screenshots:", shots);
process.exit(failures === 0 ? 0 : 1);
