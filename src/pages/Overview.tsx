import { Database, ExternalLink, Folder, Globe2, List, MoreVertical, Play, Plus, Search, Star, Terminal, Wrench } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../ui/api";
import { pickJsonFile, saveJsonFile, saveZipFile } from "../ui/dialogs";
import type { AppRun, AppSnapshot, HostInfo, SitePreview } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { TopServices } from "../components/TopServices";
import { useT } from "../ui/i18n";
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
  const t = useT();
  const selected = state.hosts.find((host) => host.id === selectedHost?.id) ?? state.hosts.find((host) => host.domain === "shop.test") ?? state.hosts[0];
  const [query, setQuery] = useState("");
  const [moreOpen, setMoreOpen] = useState(false);
  const [preview, setPreview] = useState<SitePreview>();
  const [monitorResults, setMonitorResults] = useState<Record<string, SitePreview>>({});
  const [linksVersion, setLinksVersion] = useState(0);
  const [linkLabel, setLinkLabel] = useState("");
  const [linkUrl, setLinkUrl] = useState("");
  const [workdayHostId, setWorkdayHostId] = useState(() => localStorage.getItem("localstack.workdayHost") ?? "");
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
  const openSelectedWithChecks = async (host: HostInfo) => {
    localStorage.setItem("localstack.lastHost", host.id);
    await run(() => api.diagnoseHost(host.id), { label: `Checking ${host.domain}...` }).catch(() => undefined);
    const serviceId = host.webServer.toLowerCase() === "nginx" ? "nginx" : "apache";
    const service = state.services.find((item) => item.id === serviceId);
    if (service?.status !== "running") {
      await run(() => api.startService(serviceId), { label: `Starting ${host.webServer}...` });
    }
    await run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." }).catch(() => undefined);
    await run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` });
  };
  const startWorkday = async () => {
    const target = state.hosts.find((host) => host.id === workdayHostId)
      ?? state.hosts.find((host) => host.id === localStorage.getItem("localstack.lastHost"))
      ?? selected;
    if (!target) return;
    localStorage.setItem("localstack.workdayHost", target.id);
    const ids = new Set<string>();
    ids.add(target.webServer.toLowerCase() === "nginx" ? "nginx" : "apache");
    if (target.database) ids.add("mysql");
    if (target.mailService !== "Disabled") ids.add("mailpit");
    await run(() => api.startServiceProfile(Array.from(ids)), { label: `Starting workspace for ${target.domain}...` });
    await run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file...", silent: true }).catch(() => undefined);
    await run(() => api.openHost(target.id), { label: `Opening ${target.domain}...` });
  };
  const previewSelected = async (host: HostInfo) => {
    const result = await run(() => api.previewHost(host.id), { label: `Previewing ${host.domain}...` });
    if (result && typeof result === "object" && "responseTimeMs" in result) setPreview(result as SitePreview);
  };
  const runHttpMonitor = async () => {
    const next: Record<string, SitePreview> = {};
    for (const host of visibleHosts.slice(0, 8)) {
      const result = await run(() => api.previewHost(host.id), { label: `Checking ${host.domain}...`, serial: true });
      if (result && typeof result === "object" && "responseTimeMs" in result) next[host.id] = result as SitePreview;
    }
    setMonitorResults(next);
  };
  const storageKey = selected ? `localstack.accessLinks.${selected.id}` : "";
  const accessLinks = useMemo(() => {
    void linksVersion;
    if (!selected || typeof window === "undefined") return [];
    try {
      return JSON.parse(window.localStorage.getItem(storageKey) ?? "[]") as Array<{ label: string; url: string }>;
    } catch {
      return [];
    }
  }, [linksVersion, selected, storageKey]);
  const addAccessLink = () => {
    if (!selected || typeof window === "undefined") return;
    if (!linkLabel.trim() || !linkUrl.trim()) return;
    window.localStorage.setItem(storageKey, JSON.stringify([...accessLinks, { label: linkLabel.trim(), url: linkUrl.trim() }]));
    setLinkLabel("");
    setLinkUrl("");
    setLinksVersion((value) => value + 1);
  };
  const removeAccessLink = (index: number) => {
    if (!selected || typeof window === "undefined") return;
    window.localStorage.setItem(storageKey, JSON.stringify(accessLinks.filter((_, itemIndex) => itemIndex !== index)));
    setLinksVersion((value) => value + 1);
  };
  const exportPortable = async (host: HostInfo) => {
    const path = await saveZipFile(`${state.settings.backupsFolder}\\${host.domain}-portable.zip`);
    if (path) await run(() => api.exportPortableHost(host.id, path), { label: `Exporting ${host.domain} portable package...` });
  };
  return (
    <>
      <TopServices
        state={state}
        onStartAll={() => void run(api.startAll, { label: "Starting all services..." })}
        onStopAll={() => void run(api.stopAll, { label: "Stopping all services..." })}
        onRestartAll={() => void run(api.restartAll, { label: "Restarting all services..." })}
        onOpenSite={() => selected && void openSelectedWithChecks(selected)}
        onToggleService={(serviceId, running) => void run(() => running ? api.stopService(serviceId) : api.startService(serviceId), { label: `${running ? "Stopping" : "Starting"} ${serviceId}...` })}
      />
      {issues.length > 0 && (
        <Panel title="Needs Attention">
          <div className="issue-grid">
            {issues.map((issue) => (
              <button key={issue.key} className="issue-card" onClick={() => void issue.action()}>
                <Wrench size={16} />
                <strong>{issue.count}</strong>
                <span>{t(issue.label)}</span>
              </button>
            ))}
          </div>
        </Panel>
      )}
      <Panel title="Workday">
        <div className="workday-row">
          <select value={workdayHostId} onChange={(event) => setWorkdayHostId(event.target.value)}>
            <option value="">{t("Last opened project")}</option>
            {state.hosts.map((host) => <option key={host.id} value={host.id}>{host.domain}</option>)}
          </select>
          <Button variant="primary" icon={<Play size={16} />} onClick={() => void startWorkday()}>Start Workday</Button>
          <Button icon={<Wrench size={16} />} onClick={() => void run(() => api.repairEnvironment(), { label: String(t("Repairing environment...")) })}>{t("Ready Check")}</Button>
        </div>
      </Panel>
      <div className="overview-grid">
        <section className="stack-left">
          <Panel title="Recent Projects">
            <div className="recent-project-grid">
              {visibleHosts.slice(0, 6).map((host) => (
                <button key={host.id} onClick={() => { selectHost(host); void openSelectedWithChecks(host); }}>
                  <strong>{host.domain}</strong>
                  <span>{host.rootFolder}</span>
                </button>
              ))}
            </div>
          </Panel>
          <Panel
            title="Hosts"
            action={
              <div className="toolbar">
                <label className="search">
                  <Search size={17} />
                  <input placeholder={String(t("Search hosts..."))} value={query} onChange={(event) => setQuery(event.target.value)} />
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
                  <th>{t("Domain")}</th>
                  <th>{t("Root Folder")}</th>
                  <th>{t("PHP Version")}</th>
                  <th>SSL</th>
                  <th>{t("Readiness")}</th>
                  <th>{t("Status")}</th>
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
                    <td><span className={host.ssl ? "green-text" : "muted"}>{t(host.ssl ? "Valid" : "Disabled")}</span></td>
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
              <span>{visibleHosts.length} {t("Hosts")}</span>
              <div className="segmented">
                <Button variant="icon" icon={<List size={16} />} onClick={() => void run(() => api.openPath(`${state.appDataDir}\\hosts`))} />
                <Button variant="icon" icon={<Database size={16} />} onClick={() => void run(() => api.openDatabaseAdmin("phpmyadmin"))} />
              </div>
            </div>
          </Panel>
          <Panel title="Ports" action={<Button icon={<Search size={16} />} onClick={() => void runHttpMonitor()}>Run Monitor</Button>}>
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
          {Object.keys(monitorResults).length > 0 && (
            <Panel title="HTTP Monitor">
              <div className="monitor-list">
                {visibleHosts.filter((host) => monitorResults[host.id]).map((host) => {
                  const result = monitorResults[host.id];
                  const ok = /^2|^3/.test(result.status);
                  return (
                    <button key={host.id} className="monitor-row" onClick={() => void run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` })}>
                      <span className={`status-dot ${ok ? "green" : "error"}`} />
                      <strong>{host.domain}</strong>
                      <span>{result.status}</span>
                      <small>{result.responseTimeMs}ms</small>
                    </button>
                  );
                })}
              </div>
            </Panel>
          )}
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
                <strong>{t(readinessLabel(hostReadinessScore(state, selected)))}</strong>
              </div>
              <div className="kv form-kv">
                <span>{t("Root Folder")}</span>
                <button onClick={() => void run(() => api.openPath(selected.rootFolder))}>{selected.rootFolder}<Folder size={16} /></button>
                <span>URL</span>
                <button onClick={() => void run(() => api.openHost(selected.id))}>{hostUrl(selected)}<ExternalLink size={16} /></button>
                <span>{t("PHP Version")}</span>
                <strong>{selected.phpVersion}</strong>
                <span>{t("Web Server")}</span>
                <strong>{selected.webServer}</strong>
                <span>{t("Document Root")}</span>
                <strong>{selected.documentRoot}</strong>
                <span>{t("Error Log")}</span>
                <button onClick={() => void run(() => api.openPath(`${selected.rootFolder}\\logs\\error.log`))}>{selected.rootFolder}\\logs\\error.log</button>
              </div>
            </Panel>
            <Panel title={t("Quick Actions")}>
              <div className="quick-grid">
                <Button icon={<Globe2 size={17} />} onClick={() => void openSelectedWithChecks(selected)}>
                  Open in Browser
                </Button>
                <Button icon={<Folder size={17} />} onClick={() => void run(() => api.openPath(selected.rootFolder))}>
                  {t("Open Root Folder")}
                </Button>
                <Button icon={<Terminal size={17} />} onClick={() => void run(() => api.openTerminal(selected.rootFolder))}>
                  Open in Terminal
                </Button>
                <Button icon={<Database size={17} />} onClick={() => void run(() => api.openDatabaseAdmin("phpmyadmin"))}>
                  phpMyAdmin
                </Button>
                <Button icon={<Search size={17} />} onClick={() => void previewSelected(selected)}>
                  Preview Site
                </Button>
                <Button icon={<Folder size={17} />} onClick={() => void exportPortable(selected)}>
                  Export Portable
                </Button>
                <Button icon={<Terminal size={17} />} onClick={() => void run(() => api.runProjectCommand(selected.rootFolder, "npm-install"), { label: `Running npm install for ${selected.domain}...` })}>
                  npm install
                </Button>
                <Button icon={<Terminal size={17} />} onClick={() => void run(() => api.runProjectCommand(selected.rootFolder, "composer-install"), { label: `Running composer install for ${selected.domain}...` })}>
                  composer install
                </Button>
                <Button icon={<Terminal size={17} />} onClick={() => void run(() => api.runProjectCommand(selected.rootFolder, "artisan-migrate"), { label: `Running migrations for ${selected.domain}...` })}>
                  artisan migrate
                </Button>
              </div>
              {preview && <div className="preview-result"><strong>{preview.status}</strong><span>{preview.responseTimeMs}ms · {preview.contentType}</span><small>{preview.redirectedTo ?? preview.message}</small></div>}
            </Panel>
            <Panel title="Access Links" action={<Button icon={<Plus size={15} />} disabled={!linkLabel.trim() || !linkUrl.trim()} onClick={addAccessLink}>Add Link</Button>}>
              <div className="access-link-form">
                <input value={linkLabel} onChange={(event) => setLinkLabel(event.target.value)} placeholder="Name" />
                <input value={linkUrl} onChange={(event) => setLinkUrl(event.target.value)} placeholder="URL" />
              </div>
              <div className="access-link-list">
                <button onClick={() => void run(() => api.openHost(selected.id), { label: `Opening ${selected.domain}...` })}><Globe2 size={16} />{hostUrl(selected)}</button>
                <button onClick={() => void run(() => api.openDatabaseAdmin("phpmyadmin"), { label: "Opening phpMyAdmin..." })}><Database size={16} />phpMyAdmin</button>
                <button onClick={() => void run(() => api.openDatabaseAdmin("adminer"), { label: "Opening Adminer..." })}><Database size={16} />Adminer</button>
                {accessLinks.map((link, index) => (
                  <span className="access-link-item" key={`${link.url}-${index}`}>
                    <button onClick={() => void run(() => api.openUrl(link.url), { label: `Opening ${link.label}...` })}><ExternalLink size={16} />{link.label}</button>
                    <Button variant="icon" aria-label="Delete" icon={<MoreVertical size={14} />} onClick={() => removeAccessLink(index)} />
                  </span>
                ))}
              </div>
            </Panel>
          </aside>
        )}
      </div>
    </>
  );
}

export function hostUrl(host: HostInfo) {
  const useHttps = host.ssl;
  const scheme = useHttps ? "https" : "http";
  const port = useHttps ? host.httpsPort : host.httpPort;
  const defaultPort = (useHttps && port === 443) || (!useHttps && port === 80);
  return defaultPort ? `${scheme}://${host.domain}` : `${scheme}://${host.domain}:${port}`;
}
