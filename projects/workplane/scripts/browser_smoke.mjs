#!/usr/bin/env node

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const base = process.env.WORKPLANE_BROWSER_BASE ?? "http://127.0.0.1:18080";
const output = process.env.WORKPLANE_BROWSER_EVIDENCE ?? "target/agent-evidence/m1-browser";
const chrome = process.env.CHROME_PATH ?? [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
].find((candidate) => { try { return Boolean(candidate && process.getBuiltinModule("node:fs").existsSync(candidate)); } catch { return false; } });
if (!chrome) throw new Error("Chrome/Chromium is required for the fail-closed browser gate");

await process.getBuiltinModule("node:fs/promises").mkdir(output, { recursive: true });
const profile = await mkdtemp(join(tmpdir(), "workplane-browser-"));
const port = 9327 + Math.floor(Math.random() * 500);
const processHandle = spawn(chrome, [
  "--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check",
  "--disable-background-networking", "--disable-component-update", "--disable-sync",
  `--remote-debugging-port=${String(port)}`, `--user-data-dir=${profile}`,
  "--window-size=1440,1000", "about:blank",
], { stdio: ["ignore", "ignore", "pipe"] });
let chromeLog = "";
processHandle.stderr.on("data", (chunk) => { chromeLog += String(chunk); });

try {
  // Hosted runners can take longer than five seconds to initialize Chrome
  // under load. Keep the gate fail-closed, but give the DevTools endpoint a
  // bounded 30-second startup window before judging the browser unavailable.
  const version = await pollJSON(`http://127.0.0.1:${String(port)}/json/version`, 600);
  if (!version.webSocketDebuggerUrl) throw new Error("Chrome did not expose DevTools");
  await fetch(`http://127.0.0.1:${String(port)}/json/new?${encodeURIComponent(base)}`, { method: "PUT" });
  const pages = await pollJSON(`http://127.0.0.1:${String(port)}/json/list`);
  const page = pages.find((entry) => entry.type === "page" && entry.url.startsWith(base));
  if (!page) throw new Error("Workplane browser target was not created");
  const cdp = await connect(page.webSocketDebuggerUrl);
  const consoleEvents = [];
  const network = [];
  const requests = new Map();
  cdp.on("Runtime.consoleAPICalled", (params) => {
    consoleEvents.push({ type: params.type, values: params.args.map((arg) => String(arg.value ?? arg.description ?? "")) });
  });
  cdp.on("Runtime.exceptionThrown", (params) => {
    consoleEvents.push({ type: "exception", values: [params.exceptionDetails.text] });
  });
  cdp.on("Network.requestWillBeSent", (params) => {
    if (params.request.url.startsWith(base)) requests.set(params.requestId, { method: params.request.method, url: params.request.url });
  });
  cdp.on("Network.responseReceived", (params) => {
    const request = requests.get(params.requestId);
    if (request) network.push({ ...request, status: params.response.status, mime_type: params.response.mimeType });
  });
  cdp.on("Network.loadingFailed", (params) => {
    const request = requests.get(params.requestId);
    if (request) network.push({ ...request, failed: true, error: params.errorText });
  });
  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("Network.enable");
  await cdp.send("Accessibility.enable");
  await cdp.send("Page.navigate", { url: base });
  await waitFor(cdp, `document.querySelector("h1")?.textContent?.includes("One project plane")`);
  await evaluate(cdp, `document.querySelectorAll("button")[0].click()`);
  try {
    await waitFor(cdp, `document.querySelector('[role="status"]')?.textContent?.includes("Human session established")`);
  } catch (error) {
    console.error(JSON.stringify({ consoleEvents, network }, null, 2));
    throw error;
  }
  await evaluate(cdp, `[...document.querySelectorAll("button")].find((button) => button.textContent.includes("Create through public API")).click()`);
  await waitFor(cdp, `document.querySelector('[role="status"]')?.textContent?.includes("project.created committed atomically")`);
  await evaluate(cdp, `[...document.querySelectorAll("button")].find((button) => button.textContent.includes("Record continue decision")).click()`);
  await waitFor(cdp, `document.querySelector('[role="status"]')?.textContent?.includes("Continue decision committed")`);
  await waitFor(cdp, `[...document.querySelectorAll(".activity strong")].map((node) => node.textContent).join(",") === "project.created,decision.recorded"`);

  const dom = await evaluate(cdp, `document.documentElement.outerHTML`);
  const text = await evaluate(cdp, `document.body.innerText`);
  const accessibility = await cdp.send("Accessibility.getFullAXTree");
  const screenshot = await cdp.send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  const sourceMapUsed = await sourceMapContainsGeneratedClient();
  if (!text.includes("VERSION\n2") || !text.includes("human · 00000000-0000-4000-8000-000000000001")) throw new Error(`committed projection was not rendered:\n${text}`);
  if (!sourceMapUsed) throw new Error("production bundle source map omits generated TypeScript client");
  const relevantConsole = consoleEvents.filter((event) => ["error", "warning", "exception", "assert"].includes(event.type));
  const failures = network.filter((entry) => entry.failed || entry.status >= 400);
  const requiredPaths = ["/api/v1/session/login", "/projects", "/decisions", "/activity"];
  for (const expected of requiredPaths) if (!network.some((entry) => entry.url.includes(expected))) throw new Error(`browser network omitted ${expected}`);
  if (relevantConsole.length || failures.length) throw new Error(`browser console/network failures: ${JSON.stringify({ relevantConsole, failures })}`);

  await writeFile(join(output, "browser-screenshot.png"), Buffer.from(screenshot.data, "base64"));
  await writeFile(join(output, "browser-dom.html"), `${dom}\n`);
  await writeFile(join(output, "browser-text.txt"), `${text}\n`);
  await writeFile(join(output, "browser-accessibility.json"), `${JSON.stringify(accessibility, null, 2)}\n`);
  await writeFile(join(output, "browser-console.json"), `${JSON.stringify(consoleEvents, null, 2)}\n`);
  await writeFile(join(output, "browser-network.json"), `${JSON.stringify(network, null, 2)}\n`);
  const digest = createHash("sha256").update(Buffer.from(screenshot.data, "base64")).digest("hex");
  await writeFile(join(output, "browser-summary.json"), `${JSON.stringify({ result: "pass", viewport: { width: 1440, height: 1000 }, screenshot_sha256: digest, generated_client_in_source_map: true, console_failures: 0, network_failures: 0 }, null, 2)}\n`);
  cdp.close();
  console.log(`browser walking slice passed: screenshot_sha256=${digest}`);
} catch (error) {
  if (chromeLog) console.error(chromeLog);
  throw error;
} finally {
  processHandle.kill("SIGTERM");
  if (processHandle.exitCode === null) {
    await Promise.race([
      new Promise((resolve) => processHandle.once("exit", resolve)),
      new Promise((resolve) => setTimeout(resolve, 2000)),
    ]);
  }
  for (let attempt = 0; attempt < 5; attempt += 1) {
    try { await rm(profile, { recursive: true, force: true }); break; }
    catch { await new Promise((resolve) => setTimeout(resolve, 100)); }
  }
  if (chromeLog && processHandle.exitCode && processHandle.exitCode !== 0) console.error(chromeLog);
}

async function sourceMapContainsGeneratedClient() {
  const html = await (await fetch(base)).text();
  const script = html.match(/<script[^>]+src="([^"]+)"/)?.[1];
  if (!script) return false;
  const source = await (await fetch(new URL(script, base))).text();
  const mapName = source.match(/sourceMappingURL=([^\s]+)/)?.[1];
  if (!mapName) return false;
  const map = JSON.parse(await (await fetch(new URL(mapName, new URL(script, base)))).text());
  return map.sources.some((entry) => entry.endsWith("/src/api/client.gen.ts"));
}

async function pollJSON(url, attempts = 100) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try { const response = await fetch(url); if (response.ok) return response.json(); } catch { /* retry */ }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`timed out waiting for ${url}`);
}

async function connect(url) {
  const socket = new WebSocket(url);
  const pending = new Map();
  const listeners = new Map();
  let sequence = 0;
  await new Promise((resolve, reject) => { socket.addEventListener("open", resolve, { once: true }); socket.addEventListener("error", reject, { once: true }); });
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    if (message.id) {
      const request = pending.get(message.id); pending.delete(message.id);
      if (message.error) request.reject(new Error(message.error.message)); else request.resolve(message.result);
    } else {
      for (const listener of listeners.get(message.method) ?? []) listener(message.params);
    }
  });
  return {
    send(method, params = {}) { sequence += 1; return new Promise((resolve, reject) => { pending.set(sequence, { resolve, reject }); socket.send(JSON.stringify({ id: sequence, method, params })); }); },
    on(method, listener) { const current = listeners.get(method) ?? []; current.push(listener); listeners.set(method, current); },
    close() { socket.close(); },
  };
}

async function evaluate(cdp, expression) {
  const response = await cdp.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (response.exceptionDetails) throw new Error(response.exceptionDetails.text);
  return response.result.value;
}

async function waitFor(cdp, expression) {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (await evaluate(cdp, `Boolean(${expression})`)) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  const state = await evaluate(cdp, `document.body?.innerText ?? document.documentElement.outerHTML`);
  throw new Error(`browser condition timed out: ${expression}\nvisible state:\n${state}`);
}
