import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const reportDir = path.join(root, "reports", "themes");
const port = Number(process.env.LOCALSTACK_THEME_AUDIT_PORT ?? 4188);
const baseUrl = `http://127.0.0.1:${port}`;
const themes = ["light", "pearl", "graphite", "azure", "forest", "dark", "midnight", "carbon", "wet-asphalt", "high-contrast"];

await mkdir(reportDir, { recursive: true });
const server = await startServer();
const browser = await launchBrowser();
const results = [];

try {
  const page = await browser.newPage({ viewport: { width: 1366, height: 820 } });
  page.setDefaultNavigationTimeout(30000);
  page.setDefaultTimeout(30000);
  for (const theme of themes) {
    await gotoWithRetry(page, `${baseUrl}/#overview`);
    await page.waitForTimeout(400);
    await page.evaluate((themeName) => {
      const frame = document.querySelector(".app-frame");
      if (!frame) return;
      frame.className = frame.className
        .split(/\s+/)
        .filter((name) => !name.startsWith("theme-"))
        .concat(`theme-${themeName}`)
        .join(" ");
    }, theme);
    await page.screenshot({ path: path.join(reportDir, `${theme}.png`), fullPage: true });
    const checks = await page.evaluate(checkThemeContrast);
    results.push({ theme, screenshot: `${theme}.png`, checks, issues: checks.filter((item) => !item.ok) });
  }
} finally {
  await browser.close();
  await stopServer(server);
}

const summary = {
  generatedAt: new Date().toISOString(),
  total: results.length,
  failures: results.filter((item) => item.issues.length > 0).length,
  results
};
await writeFile(path.join(reportDir, "theme-report.json"), JSON.stringify(summary, null, 2), "utf8");
await writeFile(path.join(reportDir, "theme-report.html"), renderHtml(summary), "utf8");
console.log(`Theme audit: ${summary.total - summary.failures}/${summary.total} passed`);
console.log(`Report: ${path.join(reportDir, "theme-report.html")}`);
if (summary.failures > 0) {
  for (const item of results.filter((result) => result.issues.length > 0)) {
    console.log(`${item.theme}: ${item.issues.map((issue) => `${issue.label} ${issue.ratio}`).join(", ")}`);
  }
  process.exitCode = 1;
}

function checkThemeContrast() {
  let fixture = document.querySelector("#theme-audit-fixture");
  if (!fixture) {
    const host = document.querySelector(".app-frame") ?? document.body;
    fixture = document.createElement("div");
    fixture.id = "theme-audit-fixture";
    fixture.style.cssText = "position:fixed;left:12px;bottom:12px;z-index:99999;display:flex;gap:8px;padding:8px;background:var(--card);border:1px solid var(--line)";
    fixture.innerHTML = '<button class="btn">Button</button><button class="btn btn-primary">Primary</button><button class="btn btn-danger">Delete</button><span class="pill blue">Blue pill</span><nav class="nav"><button class="active"><span>Active</span></button></nav>';
    host.appendChild(fixture);
  }
  const targets = [
    [".btn", "button"],
    [".btn-primary", "primary button"],
    [".btn-danger", "danger button"],
    ["#theme-audit-fixture .nav button.active span", "active nav"],
    [".pill.blue", "blue pill"]
  ];
  return targets.map(([selector, label]) => {
    const element = selector.includes(".nav")
      ? document.querySelector(selector)
      : fixture.querySelector(selector);
    if (!element) return { selector, label, ratio: 0, ok: false };
    const styles = getComputedStyle(element);
    const fg = rgb(styles.color);
    const bg = effectiveBackground(selector.includes(".nav") ? element.closest("button") : element);
    const ratio = contrast(fg, bg);
    return { selector, label, ratio: Math.round(ratio * 100) / 100, ok: ratio >= 4.5, fg, bg };
  });

  function effectiveBackground(element) {
    let node = element;
    while (node) {
      const color = getComputedStyle(node).backgroundColor;
      const parsed = rgb(color);
      if (parsed.a > 0) return parsed;
      node = node.parentElement;
    }
    return { r: 255, g: 255, b: 255, a: 1 };
  }

  function rgb(value) {
    const match = value.match(/rgba?\(([^)]+)\)/);
    if (!match) return { r: 0, g: 0, b: 0, a: 1 };
    const parts = match[1].split(",").map((part) => Number.parseFloat(part.trim()));
    return { r: parts[0], g: parts[1], b: parts[2], a: parts[3] ?? 1 };
  }

  function contrast(a, b) {
    const l1 = luminance(a);
    const l2 = luminance(b);
    const lighter = Math.max(l1, l2);
    const darker = Math.min(l1, l2);
    return (lighter + 0.05) / (darker + 0.05);
  }

  function luminance(color) {
    const values = [color.r, color.g, color.b].map((value) => {
      const channel = value / 255;
      return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    });
    return values[0] * 0.2126 + values[1] * 0.7152 + values[2] * 0.0722;
  }
}

async function startServer() {
  if (await isServerReady(`${baseUrl}/`)) return { pid: undefined, kill: () => undefined };
  const vite = path.join(root, "node_modules", "vite", "bin", "vite.js");
  const child = spawn(process.execPath, [vite, "--host", "127.0.0.1", "--port", String(port), "--strictPort"], { cwd: root, stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, BROWSER: "none" } });
  child.stdout.on("data", (data) => process.stdout.write(data));
  child.stderr.on("data", (data) => process.stderr.write(data));
  await waitForServer(`${baseUrl}/`);
  return child;
}

async function launchBrowser() {
  const channel = process.env.PLAYWRIGHT_CHANNEL ?? "msedge";
  try {
    return await chromium.launch({ channel, headless: true });
  } catch {
    return chromium.launch({ headless: true });
  }
}

async function stopServer(server) {
  if (!server?.pid) return;
  if (process.platform === "win32") {
    await new Promise((resolve) => {
      const killer = spawn(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", `taskkill /PID ${server.pid} /T /F >nul 2>nul`], { stdio: "ignore" });
      killer.on("exit", resolve);
      killer.on("error", resolve);
    });
  } else {
    server.kill("SIGTERM");
  }
}

async function waitForServer(url) {
  const started = Date.now();
  while (Date.now() - started < 30000) {
    if (await isServerReady(url)) return;
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error(`Theme audit server did not start at ${url}.`);
}

async function isServerReady(url) {
  try {
    const response = await fetch(url);
    return response.ok;
  } catch {
    return false;
  }
}

async function gotoWithRetry(page, url) {
  let lastError;
  for (let attempt = 0; attempt < 3; attempt += 1) {
    try {
      await page.goto("about:blank", { waitUntil: "domcontentloaded", timeout: 10000 }).catch(() => undefined);
      await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30000 });
      await page.waitForLoadState("load", { timeout: 15000 }).catch(() => undefined);
      return;
    } catch (error) {
      lastError = error;
      await page.waitForTimeout(800);
    }
  }
  throw lastError;
}

function renderHtml(summary) {
  const rows = summary.results.map((item) => `<tr class="${item.issues.length ? "fail" : "ok"}"><td>${item.theme}</td><td>${item.issues.length ? "FAIL" : "OK"}</td><td>${item.checks.map((check) => `${check.label}: ${check.ratio}`).join("<br>")}</td><td><a href="./${item.screenshot}"><img src="./${item.screenshot}" alt="${item.theme}"></a></td></tr>`).join("");
  return `<!doctype html><html><head><meta charset="utf-8"><title>Theme Audit</title><style>body{font-family:Segoe UI,Arial,sans-serif;background:#f6f7f9;color:#1f2328;margin:0}main{max-width:1180px;margin:auto;padding:24px}table{width:100%;border-collapse:collapse;background:#fff;border:1px solid #dfe4ea}td,th{padding:10px;border-bottom:1px solid #e7ebf0;text-align:left;vertical-align:top}.ok td:nth-child(2){color:#16833a;font-weight:700}.fail td:nth-child(2){color:#b42318;font-weight:700}img{width:260px;border:1px solid #dfe4ea;border-radius:6px}</style></head><body><main><h1>Theme Audit</h1><p>${summary.generatedAt} · ${summary.total - summary.failures}/${summary.total} passed</p><table><thead><tr><th>Theme</th><th>Status</th><th>Contrast</th><th>Screenshot</th></tr></thead><tbody>${rows}</tbody></table></main></body></html>`;
}
