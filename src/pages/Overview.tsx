import { Database, ExternalLink, Folder, Globe2, List, MoreVertical, Play, Plus, Search, Star, Terminal, Wrench } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../ui/api";
import { pickJsonFile, saveJsonFile } from "../ui/dialogs";
import type { AppRun, AppSnapshot, HostInfo } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { TopServices } from "../components/TopServices";
import { hostReadinessScore, readinessClass, readinessLabel } from "../ui/readiness";
import { useStoredList } from "../ui/preferences";

export function OverviewPage({
  state,
  run,
  selectedHost,
  selectHost,
  editHost
}: {
  state: AppSnapshot;
  run: AppRun;
  selectedHost?: HostInfo;
  selectHost: (host: HostInfo) => void;
  editHost: (host?: HostInfo) => void;
}) {
  const selected = state.hosts.find((host) => host.id === selectedHost?.id) ?? state.hosts.find((host) => host.domain === "shop.test") ?? state.hosts[0];
  const [query, setQuery] = useState("");
  const [moreOpen, setMoreOpen] = useState(false);
  const favorites = useStoredList("localstack.favoriteHosts");
  const visibleHosts = useMemo(() => state.hosts
    .filter((host) => `${host.domain} ${host.rootFolder}`.toLowerCase().includes(query.toLowerCase()))
    .sort((a, b) => Number(favorites.items.includes(b.id)) - Number(favorites.items.includes(a.id)) || a.domain.localeCompare(b.domain)), [favorites.items, query, state.hosts]);
  const issues = useMemo(() => {
    const stoppedServices = state.services.filter((service) => service.autostart && service.status !== "running").length;
    const weakHosts = state.hosts.filter((host) => hostReadinessScore(state, host) < 100).length;
    const sslIssues = state.hosts.filter((host) => host.ssl && !state.certificates.some((cert) => cert.domain === host.domain && cert.trusted)).length;
    return [
      { key: "services", label: "Autostart services stopped", count: stoppedServices, action: () => run(api.startAll, { label: "Starting all services..." }) },
      { key: "hosts", label: "Hosts need attention", count: weakHosts, action: () => run(() => api.repairEnvironment(), { label: "Repairing environment..." }) },
      { key: "ssl", label: "SSL trust checks", count: sslIssues, action: () => run(() => api.openMainPage("ssl"), { label: "Opening SSL..." }) }
    ].filter((item) => item.count > 0);
  }, [run, state]);
  const portCards = useMemo(() => {
    const serviceForPort = (port: number) => state.services.find((service) => service.ports.includes(port));
    return [
      ["HTTP", serviceForPort(80)],
      ["HTTPS", serviceForPort(443)],
      ["MySQL", state.services.find((service) => service.id === "mysql")],
      ["Redis", state.services.find((service) => service.id === "redis")],
      ["Mailpit SMTP", state.services.find((service) => service.id === "mailpit")],
      ["Mailpit UI", state.services.find((service) => service.id === "mailpit")]
    ].map(([name, service]) => {
      const typedService = typeof service === "object" ? service : undefined;
      const port = name === "Mailpit UI" ? 8025 : typedService?.ports[0] ?? 0;
      return { name: String(name), port, running: typedService?.status === "running" };
    });
  }, [state.services]);
  const importHosts = async () => {
    const path = await pickJsonFile();
    if (path) {
      await run(() => api.importHosts(path));
    }
  };
  const exportHosts = async () => {
    const path = await saveJsonFile(`${state.appDataDir}\\hosts-export.json`);
    if (path) {
      await run(() => api.exportHosts(path));
    }
  };
  return (
    <>
      <TopServices
        state={state}
        onStartAll={() => void run(api.startAll, { label: "Starting all services..." })}
        onStopAll={() => void run(api.stopAll, { label: "Stopping all services..." })}
        onRestartAll={() => void run(api.restartAll, { label: "Restarting all services..." })}
        onOpenSite={() => selected && void run(() => api.openHost(selected.id), { label: `Opening ${selected.domain}...` })}
        onToggleService={(serviceId, running) => void run(() => running ? api.stopService(serviceId) : api.startService(serviceId), { label: `${running ? "Stopping" : "Starting"} ${serviceId}...` })}
      />
      {issues.length > 0 && (
        <Panel title="Needs Attention">
          <div className="issue-grid">
            {issues.map((issue) => (
              <button key={issue.key} className="issue-card" onClick={() => void issue.action()}>
                <Wrench size={16} />
                <strong>{issue.count}</strong>
                <span>{issue.label}</span>
              </button>
            ))}
          </div>
        </Panel>
      )}
      <div className="overview-grid">
        <section className="stack-left">
          <Panel
            title="Hosts"
            action={
              <div className="toolbar">
                <label className="search">
                  <Search size={17} />
                  <input placeholder="Search hosts..." value={query} onChange={(event) => setQuery(event.target.value)} />
                </label>
                <Button variant="primary" icon={<Plus size={16} />} onClick={() => editHost()}>
                  New Host
                </Button>
                <Button icon={<ExternalLink size={16} />} onClick={() => void importHosts()}>Import</Button>
                <div className="menu-anchor">
                  <Button variant="icon" aria-label="More host actions" icon={<MoreVertical size={17} />} onClick={() => setMoreOpen((value) => !value)} />
                  {moreOpen && (
                    <div className="action-menu" onMouseLeave={() => setMoreOpen(false)}>
                      <button onClick={() => { setMoreOpen(false); void exportHosts(); }}>Export Hosts</button>
                      <button onClick={() => { setMoreOpen(false); void run(() => api.openPath(`${state.appDataDir}\\hosts`), { label: "Opening hosts folder..." }); }}>Open Hosts Folder</button>
                      <button onClick={() => { setMoreOpen(false); void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." }); }}>Sync Hosts File</button>
                    </div>
                  )}
                </div>
              </div>
            }
          >
            <table className="data-table">
              <thead>
                <tr>
                  <th>Domain</th>
                  <th>Root Folder</th>
                  <th>PHP Version</th>
                  <th>SSL</th>
                  <th>Readiness</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {visibleHosts.map((host) => {
                  const score = hostReadinessScore(state, host);
                  return (
                  <tr key={host.id} className={selected?.id === host.id ? "selected" : ""} onClick={() => selectHost(host)}>
                    <td>
                      <button className={`star-button ${favorites.items.includes(host.id) ? "active" : ""}`} onClick={(event) => { event.stopPropagation(); favorites.toggle(host.id); }}><Star size={14} /></button>
                      <Globe2 size={18} />
                      <strong>{host.domain}</strong>
                    </td>
                    <td>{host.rootFolder}</td>
                    <td>{host.phpVersion}</td>
                    <td><span className={host.ssl ? "green-text" : "muted"}>{host.ssl ? "Valid" : "Disabled"}</span></td>
                    <td><span className={`score-pill ${readinessClass(score)}`}>{score}%</span></td>
                    <td>
                      <StatusDot status={host.status} />
                    </td>
                  </tr>
                  );
                })}
              </tbody>
            </table>
            <div className="table-foot">
              <span>{visibleHosts.length} hosts</span>
              <div className="segmented">
                <Button variant="icon" icon={<List size={16} />} onClick={() => void run(() => api.openPath(`${state.appDataDir}\\hosts`))} />
                <Button variant="icon" icon={<Database size={16} />} onClick={() => void run(() => api.openDatabaseAdmin("phpmyadmin"))} />
              </div>
            </div>
          </Panel>
          <Panel title="Ports">
            <div className="port-grid">
              {portCards.map(({ name, port, running }) => (
                <div className="port-card" key={name}>
                  <small>{name}</small>
                  <strong>{port || "-"}</strong>
                  <i className={`status-dot ${running ? "green" : "gray"}`} />
                </div>
              ))}
            </div>
          </Panel>
          <Panel title="Logs" action={<Button variant="icon" icon={<MoreVertical size={16} />} onClick={() => void run(() => api.openPath(`${state.appDataDir}\\logs`))} />}>
            <div className="log-box compact-log">
              {state.logs.slice(0, 5).map((entry) => (
                <pre key={entry.id}>
                  [{new Date(entry.timestamp).toLocaleTimeString()}] <b>{entry.service}</b> <span>▶</span> {entry.message}
                </pre>
              ))}
            </div>
          </Panel>
        </section>
        {selected && (
          <aside className="detail-rail">
            <Panel
              title={selected.domain}
              action={
                <div className="rail-actions">
                  <StatusDot status={selected.status} />
                  <Button variant="icon" icon={<MoreVertical size={17} />} onClick={() => editHost(selected)} />
                </div>
              }
            >
              <div className="readiness-block">
                <span className={`score-pill ${readinessClass(hostReadinessScore(state, selected))}`}>{hostReadinessScore(state, selected)}%</span>
                <strong>{readinessLabel(hostReadinessScore(state, selected))}</strong>
              </div>
              <div className="kv form-kv">
                <span>Root Folder</span>
                <button onClick={() => void run(() => api.openPath(selected.rootFolder))}>{selected.rootFolder}<Folder size={16} /></button>
                <span>URL</span>
                <button onClick={() => void run(() => api.openHost(selected.id))}>{hostUrl(selected)}<ExternalLink size={16} /></button>
                <span>PHP Version</span>
                <strong>{selected.phpVersion}</strong>
                <span>Web Server</span>
                <strong>{selected.webServer}</strong>
                <span>Document Root</span>
                <strong>{selected.documentRoot}</strong>
                <span>Error Log</span>
                <button onClick={() => void run(() => api.openPath(`${selected.rootFolder}\\logs\\error.log`))}>{selected.rootFolder}\\logs\\error.log</button>
              </div>
            </Panel>
            <Panel title="Quick Actions">
              <div className="quick-grid">
                <Button icon={<Globe2 size={17} />} onClick={() => void run(() => api.openHost(selected.id))}>
                  Open in Browser
                </Button>
                <Button icon={<Folder size={17} />} onClick={() => void run(() => api.openPath(selected.rootFolder))}>
                  Open Root Folder
                </Button>
                <Button icon={<Terminal size={17} />} onClick={() => void run(() => api.openTerminal(selected.rootFolder))}>
                  Open in Terminal
                </Button>
                <Button icon={<Database size={17} />} onClick={() => void run(() => api.openDatabaseAdmin("phpmyadmin"))}>
                  phpMyAdmin
                </Button>
              </div>
            </Panel>
          </aside>
        )}
      </div>
    </>
  );
}

export function hostUrl(host: HostInfo) {
  const forceHttps = host.ssl && host.rewriteRules.includes("FORCE_HTTPS=1");
  const scheme = forceHttps ? "https" : "http";
  const port = forceHttps ? host.httpsPort : host.httpPort;
  const defaultPort = (forceHttps && port === 443) || (!forceHttps && port === 80);
  return defaultPort ? `${scheme}://${host.domain}` : `${scheme}://${host.domain}:${port}`;
}
