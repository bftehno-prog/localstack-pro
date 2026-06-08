import { Download, FileText, Pause, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../ui/api";
import { saveTextFile, saveZipFile } from "../ui/dialogs";
import type { AppRun, AppSnapshot, LogEntry, LogFileTail, LogLevel } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";

export function LogsPage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const [level, setLevel] = useState<LogLevel | "All">("All");
  const [service, setService] = useState("All");
  const [host, setHost] = useState("All");
  const [query, setQuery] = useState("");
  const [timeRange, setTimeRange] = useState("Last 15 minutes");
  const [liveTail, setLiveTail] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [logSource, setLogSource] = useState("application");
  const [fileTail, setFileTail] = useState<LogFileTail | null>(null);
  const logBoxRef = useRef<HTMLDivElement>(null);
  const fallbackLog: LogEntry = { id: "empty", timestamp: new Date().toISOString(), level: "INFO", service: "LocalStack Pro", host: undefined, processId: undefined, source: "application.log", line: undefined, message: "No log entries yet.", detail: undefined };
  const [selected, setSelected] = useState<LogEntry>(state.logs.find((log) => log.level === "ERROR") ?? state.logs[0] ?? fallbackLog);
  const logSources = useMemo(() => [
    { value: "application", label: "Application" },
    ...state.services.map((item) => ({ value: item.id, label: item.name })),
    ...state.hosts.map((item) => ({ value: `host:${item.domain}`, label: item.domain }))
  ], [state.hosts, state.services]);
  const tailFile = useCallback(async (silent = false) => {
    const result = silent
      ? await api.tailLogFile(logSource, 120)
      : await run(() => api.tailLogFile(logSource, 120), { label: "Reading log file" });
    if (result && typeof result === "object" && "lines" in result) {
      setFileTail(result as LogFileTail);
    }
  }, [logSource, run]);
  useEffect(() => {
    void tailFile(true);
  }, [tailFile]);
  useEffect(() => {
    if (!liveTail) return;
    const timer = window.setInterval(() => {
      void tailFile(true);
    }, 10000);
    return () => window.clearInterval(timer);
  }, [liveTail, tailFile]);
  const logs = state.logs.filter((log) => {
    const matchesLevel = level === "All" || log.level === level;
    const matchesService = service === "All" || log.service.toLowerCase() === service.toLowerCase();
    const matchesHost = host === "All" || log.host === host;
    const haystack = `${log.service} ${log.host ?? ""} ${log.message} ${log.detail ?? ""}`.toLowerCase();
    return matchesLevel && matchesService && matchesHost && (!query.trim() || haystack.includes(query.toLowerCase()));
  });
  const visibleLogs = (logs.length > 0 ? logs : [fallbackLog]).slice(0, 1000);
  const visibleTailLines = (fileTail?.lines ?? []).slice(-1500).filter((line) => {
    const matchesLevel = level === "All" || line.toUpperCase().includes(level);
    return matchesLevel && (!query.trim() || line.toLowerCase().includes(query.toLowerCase()));
  });
  useEffect(() => {
    if (autoScroll && logBoxRef.current) {
      logBoxRef.current.scrollTop = logBoxRef.current.scrollHeight;
    }
  }, [autoScroll, fileTail, visibleLogs.length, visibleTailLines.length]);
  const lineClass = (line: string) => {
    const upper = line.toUpperCase();
    if (upper.includes("ERROR") || upper.includes("CRITICAL") || upper.includes("FATAL")) return "log-error";
    if (upper.includes("WARNING") || upper.includes("WARN")) return "log-warning";
    if (upper.includes("DEBUG")) return "log-debug";
    return "log-info";
  };
  const counts = (wanted: LogLevel) => state.logs.filter((log) => log.level === wanted).length;
  const insights = useMemo(() => smartLogInsights(state.logs, fileTail?.lines ?? []), [fileTail, state.logs]);
  const exportLogs = async () => {
    const path = await saveTextFile(`${state.appDataDir}\\logs-export.txt`);
    if (path) {
      await run(() => api.exportLogs(path));
    }
  };
  const exportDiagnostics = async () => {
    const path = await saveZipFile(`${state.appDataDir}\\localstack-diagnostics.zip`);
    if (path) await run(() => api.createDiagnosticBundle(path), { label: "Exporting diagnostic bundle..." });
  };
  return (
    <div className="logs-page">
      <div className="stat-row">
        <Panel><div className="stat danger">Recent Errors<strong>{counts("ERROR")}</strong><small>Last 15 minutes</small></div></Panel>
        <Panel><div className="stat warning">Warnings<strong>{counts("WARNING")}</strong><small>Last 15 minutes</small></div></Panel>
        <Panel><div className="stat blue">Requests / Min<strong>{Math.max(state.logs.length, 0)}</strong><small>Current log volume</small></div></Panel>
        <Panel><div className="stat green">Service Health<strong>{state.services.every((item) => item.status !== "error") ? "Healthy" : "Errors"}</strong><small>{state.services.filter((item) => item.status === "running").length} / {state.services.length} services</small></div></Panel>
        <Panel><div className="toolbar"><select value={timeRange} onChange={(event) => setTimeRange(event.target.value)}><option>Last 15 minutes</option><option>Last hour</option><option>Today</option><option>All time</option></select><Button variant="icon" icon={<RefreshCw size={16} />} onClick={() => void tailFile()} /></div></Panel>
      </div>
      <div className="page-grid">
        <section>
          <Panel>
            <div className="tabs">{["All", "Apache", "Nginx", "PHP", "MySQL", "Redis", "Mailpit"].map((tab) => <button key={tab} className={service === tab ? "active" : ""} onClick={() => setService(tab)}>{tab}</button>)}</div>
            <div className="filters">
              <input placeholder="Search logs..." value={query} onChange={(event) => setQuery(event.target.value)} />
              <select value={logSource} onChange={(event) => setLogSource(event.target.value)}>{logSources.map((item) => <option key={item.value} value={item.value}>{item.label}</option>)}</select>
              <select value={level} onChange={(event) => setLevel(event.target.value as LogLevel | "All")}><option>All</option><option>INFO</option><option>WARNING</option><option>ERROR</option><option>DEBUG</option></select>
              <select value={service} onChange={(event) => setService(event.target.value)}><option>All</option>{Array.from(new Set(state.logs.map((log) => log.service))).map((item) => <option key={item}>{item}</option>)}</select>
              <select value={host} onChange={(event) => setHost(event.target.value)}><option>All</option>{Array.from(new Set(state.logs.map((log) => log.host).filter(Boolean))).map((item) => <option key={item}>{item}</option>)}</select>
              <label className="inline-toggle"><span className={`toggle ${liveTail ? "on" : ""}`} onClick={() => setLiveTail((value) => !value)} />Live Tail</label>
              <Button icon={<Pause size={16} />} onClick={() => setLiveTail((value) => !value)}>{liveTail ? "Pause" : "Paused"}</Button>
              <Button icon={<RefreshCw size={16} />} onClick={() => void tailFile()}>Tail File</Button>
              <Button icon={<Trash2 size={16} />} onClick={() => void run(api.clearLogs)}>Clear</Button>
              <Button icon={<Download size={16} />} onClick={() => void exportLogs()}>Export</Button>
              <Button icon={<FileText size={16} />} onClick={() => void exportDiagnostics()}>Diagnostic Bundle</Button>
              <Button icon={<FileText size={16} />} onClick={() => void run(() => api.openPath(fileTail?.path ?? `${state.appDataDir}\\logs`))}>Open File</Button>
            </div>
            <div className="log-box" ref={logBoxRef}>
              {fileTail ? visibleTailLines.map((line, index) => <pre key={`${fileTail.path}-${index}`} className={lineClass(line)}>{line}</pre>) : visibleLogs.map((log) => (
                  <pre key={log.id} className={`log-${log.level.toLowerCase()}`} onClick={() => setSelected(log)}>
                    [{new Date(log.timestamp).toLocaleTimeString()}] <b>{log.level}</b> [{log.service}] {log.message}
                    {log.detail ? `\n${log.detail}` : ""}
                  </pre>
                ))}
            </div>
            <div className="table-foot"><span>{fileTail ? `Showing ${visibleTailLines.length} lines from ${fileTail.path}` : `Showing ${visibleLogs.length} of ${logs.length} filtered / ${state.logs.length} total`}</span><label><input type="checkbox" checked={autoScroll} onChange={(event) => setAutoScroll(event.target.checked)} /> Auto-scroll</label></div>
          </Panel>
        </section>
        <aside className="detail-rail">
          <Panel title="Smart Logs">
            {insights.length === 0 ? (
              <p className="green-text">No recurring issues detected.</p>
            ) : (
              <div className="smart-log-list">
                {insights.map((item) => (
                  <div className="smart-log-item" key={item.title}>
                    <strong>{item.title}</strong>
                    <span>{item.count}</span>
                    <small>{item.action}</small>
                  </div>
                ))}
              </div>
            )}
          </Panel>
          <Panel title="Log Details">
            <div className="kv detail-kv"><span>Timestamp</span><strong>{new Date(selected.timestamp).toLocaleString()}</strong><span>Level</span><strong className={`level-${selected.level.toLowerCase()}`}>{selected.level}</strong><span>Service</span><strong>{selected.service}</strong><span>Host</span><strong>{selected.host}</strong><span>Process ID</span><strong>{selected.processId}</strong><span>Source</span><strong>{selected.source}</strong><span>Line</span><strong>{selected.line}</strong><span>Message</span><code>{selected.message}</code></div>
          </Panel>
          <Panel title="Statistics"><div className="kv detail-kv"><span>Total Entries</span><strong>{state.logs.length}</strong><span>Errors</span><strong>{counts("ERROR")}</strong><span>Warnings</span><strong>{counts("WARNING")}</strong><span>Info</span><strong>{counts("INFO")}</strong><span>Debug</span><strong>{counts("DEBUG")}</strong></div></Panel>
          <Panel title="File Tail"><div className="kv detail-kv"><span>Source</span><strong>{fileTail?.source ?? logSource}</strong><span>Path</span><strong>{fileTail?.path ?? "-"}</strong><span>Lines</span><strong>{fileTail?.lines.length ?? 0}</strong><span>Updated</span><strong>{fileTail ? new Date(fileTail.generatedAt).toLocaleTimeString() : "-"}</strong></div></Panel>
        </aside>
      </div>
    </div>
  );
}

function smartLogInsights(logs: LogEntry[], lines: string[]) {
  const buckets: Record<string, { title: string; action: string; count: number }> = {
    port: { title: "Port conflict", action: "Open Health Check or change the service port.", count: 0 },
    ssl: { title: "SSL trust issue", action: "Open SSL and run Repair Trust.", count: 0 },
    binary: { title: "Missing executable", action: "Open Services and click Detect or Install.", count: 0 },
    database: { title: "Database auth/connection", action: "Start database service and test credentials.", count: 0 },
    host: { title: "Hosts/vhost mapping", action: "Run Sync Hosts File or Repair Host.", count: 0 }
  };
  const all = [
    ...logs.map((log) => `${log.level} ${log.service} ${log.message} ${log.detail ?? ""}`),
    ...lines
  ].map((line) => line.toLowerCase());
  for (const text of all) {
    if (text.includes("port") && (text.includes("busy") || text.includes("in use") || text.includes("listen"))) buckets.port.count += 1;
    if (text.includes("ssl") || text.includes("certificate") || text.includes("trust")) buckets.ssl.count += 1;
    if (text.includes("executable") || text.includes("not found") || text.includes("missing")) buckets.binary.count += 1;
    if (text.includes("database") || text.includes("mysql") || text.includes("postgres") || text.includes("access denied")) buckets.database.count += 1;
    if (text.includes("hosts file") || text.includes("vhost") || text.includes("servername") || text.includes("mapped")) buckets.host.count += 1;
  }
  return Object.values(buckets).filter((bucket) => bucket.count > 0).sort((a, b) => b.count - a.count).slice(0, 5);
}
