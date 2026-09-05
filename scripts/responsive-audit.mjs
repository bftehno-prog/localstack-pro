import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const reportDir = path.join(root, "reports", "responsive");
const port = Number(process.env.LOCALSTACK_AUDIT_PORT ?? 4187);
const baseUrl = `http://127.0.0.1:${port}`;

const pages = [
  ["overview", "#overview"],
  ["hosts", "#hosts"],
  ["services", "#services"],
  ["php", "#php"],
  ["database", "#database"],
  ["cms", "#cms"],
  ["ssl", "#ssl"],
  ["logs", "#logs"],
  ["files", "#files"],
  ["settings", "#settings"]
];

const viewports = [
  ["desktop-1440", { width: 1440, height: 900 }],
  ["desktop-1280", { width: 1280, height: 800 }],
  ["tablet-768", { width: 768, height: 1024 }],
  ["mobile-390", { width: 390, height: 844 }],
  ["mobile-360", { width: 360, height: 800 }]
];

await mkdir(reportDir, { recursive: true });

const server = await startServer();
const browser = await launchBrowser();
const results = [];

try {
  for (const [viewportName, viewport] of viewports) {
    const context = await browser.newContext({ viewport, deviceScaleFactor: 1 });
    const page = await context.newPage();
    page.setDefaultNavigationTimeout(30000);
    page.setDefaultTimeout(30000);
    for (const [pageName, hash] of pages) {
      const url = `${baseUrl}/${hash}`;
      await gotoWithRetry(page, url);
      await page.waitForTimeout(500);
      const screenshot = `${viewportName}-${pageName}.png`;
      await page.screenshot({ path: path.join(reportDir, screenshot), fullPage: true });
      const audit = await page.evaluate(runDomAudit);
      results.push({
        page: pageName,
        viewport: viewportName,
        width: viewport.width,
        height: viewport.height,
        screenshot,
        ...audit
      });
    }
    await context.close();
  }
} finally {
  await browser.close();
  await stopServer(server);
}

const summary = {
  generatedAt: new Date().toISOString(),
  failures: results.filter((item) => item.issues.length > 0).length,
  total: results.length,
  results
};

await writeFile(path.join(reportDir, "responsive-report.json"), JSON.stringify(summary, null, 2), "utf8");
await writeFile(path.join(reportDir, "responsive-report.html"), renderHtml(summary), "utf8");

const failed = summary.failures > 0;
console.log(`Responsive audit: ${summary.total - summary.failures}/${summary.total} passed`);
console.log(`Report: ${path.join(reportDir, "responsive-report.html")}`);
if (failed) {
  for (const item of results.filter((result) => result.issues.length > 0)) {
    console.log(`${item.viewport}/${item.page}: ${item.issues.join("; ")}`);
  }
  process.exitCode = 1;
}

async function startServer() {
  if (await isServerReady(`${baseUrl}/`)) {
    return { pid: undefined, kill: () => undefined };
  }
  const vite = path.join(root, "node_modules", "vite", "bin", "vite.js");
  const child = spawn(process.execPath, [vite, "--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, BROWSER: "none" }
  });
  child.stdout.on("data", (data) => process.stdout.write(data));
  child.stderr.on("data", (data) => process.stderr.write(data));
  await waitForServer(`${baseUrl}/`);
  return child;
}

async function stopServer(server) {
  if (!server?.pid) return;
  if (process.platform === "win32") {
    await new Promise((resolve) => {
      const killer = spawn(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", `taskkill /PID ${server.pid} /T /F >nul 2>nul`], {
        stdio: "ignore"
      });
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
  throw new Error(`Vite server did not start at ${url}`);
}

async function isServerReady(url) {
  try {
    const response = await fetch(url);
    return response.ok;
  } catch {
    return false;
  }
}

async function launchBrowser() {
  const channel = process.env.PLAYWRIGHT_CHANNEL ?? "msedge";
  try {
    return await chromium.launch({ channel, headless: true });
  } catch {
    return chromium.launch({ headless: true });
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

function runDomAudit() {
  const issues = [];
  const doc = document.documentElement;
  const body = document.body;
  const horizontalOverflow = Math.max(doc.scrollWidth, body.scrollWidth) - window.innerWidth;
  if (horizontalOverflow > 2) issues.push(`horizontal overflow ${horizontalOverflow}px`);

  const visibleElements = [...document.querySelectorAll("button, a, input, select, textarea, .panel, .card, .stat, .pill, td, th")]
    .filter((element) => {
      const rect = element.getBoundingClientRect();
      const style = getComputedStyle(element);
      return rect.width > 0 && rect.height > 0 && style.visibility !== "hidden" && style.display !== "none" && isActuallyVisible(element, rect);
    });

  const clipped = visibleElements
    .filter((element) => {
      if (element.matches("input, textarea")) return false;
      const style = getComputedStyle(element);
      if (style.overflow === "visible") return false;
      return element.scrollWidth - element.clientWidth > 2 || element.scrollHeight - element.clientHeight > 2;
    })
    .slice(0, 8)
    .map(labelFor);
  if (clipped.length > 0) issues.push(`clipped content: ${clipped.join(", ")}`);

  const controls = visibleElements.filter((element) => element.matches("button, a, input, select, textarea"));
  const overlaps = [];
  for (let i = 0; i < controls.length; i += 1) {
    const a = controls[i].getBoundingClientRect();
    for (let j = i + 1; j < controls.length; j += 1) {
      const b = controls[j].getBoundingClientRect();
      const area = intersectionArea(a, b);
      if (area > 12) {
        overlaps.push(`${labelFor(controls[i])} overlaps ${labelFor(controls[j])}`);
        break;
      }
    }
    if (overlaps.length >= 8) break;
  }
  if (overlaps.length > 0) issues.push(`overlapping controls: ${overlaps.join(", ")}`);

  const blank = document.body.innerText.trim().length < 80;
  if (blank) issues.push("page looks blank");

  return {
    url: location.href,
    title: document.title,
    horizontalOverflow,
    clippedCount: clipped.length,
    overlapCount: overlaps.length,
    issues
  };

  function intersectionArea(a, b) {
    const x = Math.max(0, Math.min(a.right, b.right) - Math.max(a.left, b.left));
    const y = Math.max(0, Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top));
    return x * y;
  }

  function labelFor(element) {
    const text = (element.innerText || element.value || element.getAttribute("aria-label") || element.className || element.tagName)
      .toString()
      .replace(/\s+/g, " ")
      .trim();
    return text.slice(0, 44) || element.tagName.toLowerCase();
  }

  function isActuallyVisible(element, rect) {
    const x = Math.min(Math.max(rect.left + Math.min(rect.width / 2, 8), 0), window.innerWidth - 1);
    const y = Math.min(Math.max(rect.top + Math.min(rect.height / 2, 8), 0), window.innerHeight - 1);
    const top = document.elementFromPoint(x, y);
    return !!top && (top === element || element.contains(top) || top.contains(element));
  }
}

function renderHtml(summary) {
  const rows = summary.results.map((item) => {
    const ok = item.issues.length === 0;
    return `<tr class="${ok ? "ok" : "fail"}"><td>${item.viewport}</td><td>${item.page}</td><td>${ok ? "OK" : "FAIL"}</td><td>${item.issues.join("<br>") || "-"}</td><td><a href="./${item.screenshot}"><img src="./${item.screenshot}" alt="${item.viewport} ${item.page}"></a></td></tr>`;
  }).join("\n");
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>LocalStack Pro Responsive Audit</title>
  <style>
    body{font-family:Segoe UI,Arial,sans-serif;margin:0;background:#f6f7f9;color:#1f2328}
    main{padding:24px;max-width:1320px;margin:auto}
    h1{font-size:24px;margin:0 0 8px}
    p{color:#626a73;margin:0 0 18px}
    table{border-collapse:collapse;width:100%;background:#fff;border:1px solid #dfe4ea}
    th,td{border-bottom:1px solid #e7ebf0;padding:10px;text-align:left;vertical-align:top;font-size:13px}
    th{background:#f2f5f8}
    tr.fail td:nth-child(3){color:#b42318;font-weight:700}
    tr.ok td:nth-child(3){color:#16833a;font-weight:700}
    img{width:220px;border:1px solid #dfe4ea;border-radius:6px}
  </style>
</head>
<body><main>
  <h1>LocalStack Pro Responsive Audit</h1>
  <p>${summary.generatedAt} · ${summary.total - summary.failures}/${summary.total} passed</p>
  <table><thead><tr><th>Viewport</th><th>Page</th><th>Status</th><th>Issues</th><th>Screenshot</th></tr></thead><tbody>${rows}</tbody></table>
</main></body></html>`;
}
