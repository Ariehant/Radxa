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

const exe = join(root, "src-tauri/target/debug/examples", process.platform === "win32" ? "dev_bridge.exe" : "dev_bridge");
const bridge = start(exe, [String(BRIDGE_PORT)]);
const vite = start(process.execPath, [join(root, "node_modules/vite/bin/vite.js"), "--port", String(UI_PORT), "--strictPort"]);

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
  check(Date.now() - t0 < 3000, `vault open → first paint in ${Date.now() - t0} ms (< 3 s)`);
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
  check(Date.now() - t1 < 1000, `flow canvas open in ${Date.now() - t1} ms`);
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
