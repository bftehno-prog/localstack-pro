import { Copy, Database, Download, ExternalLink, Filter, Folder, MoreVertical, Play, Plus, Search, ShieldCheck, Star, Terminal, Trash2, Upload, Wrench } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../ui/api";
import { pickJsonFile, pickZipFile, saveJsonFile, saveZipFile } from "../ui/dialogs";
import type { AppRun, AppSnapshot, DatabaseDiagnosticReport, DatabaseInfo, HostDiagnosticReport, HostInfo, LogFileTail, NodeScript, SitePreview } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { hostUrl } from "./Overview";
import { hostReadinessScore, readinessClass, readinessLabel } from "../ui/readiness";
import { useStoredBoolean, useStoredList } from "../ui/preferences";
import { useT } from "../ui/i18n";

type HostHistoryEntry = {
  id: string;
  host: HostInfo;
  changedAt: string;
  summary: string;
};

export function HostsPage({
  state,
  run,
  selected,
  setSelected,
  editHost
}: {
  state: AppSnapshot;
  run: AppRun;
  selected?: HostInfo;
  setSelected: (host?: HostInfo) => void;
  editHost: (host?: HostInfo) => void;
}) {
  const t = useT();
  const host = selected ?? state.hosts[0];
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("All Status");
  const [environment, setEnvironment] = useState("All Environments");
  const [phpVersion, setPhpVersion] = useState("All PHP Versions");
  const [ssl, setSsl] = useState("SSL: All");
  const [readinessFilter, setReadinessFilter] = useState("All Readiness");
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);
  const [tab, setTab] = useState("General");
  const [diagnostics, setDiagnostics] = useState<HostDiagnosticReport | null>(null);
  const [dbDiagnostics, setDbDiagnostics] = useState<DatabaseDiagnosticReport | null>(null);
  const [hostLogs, setHostLogs] = useState<LogFileTail | null>(null);
  const [nodeScripts, setNodeScripts] = useState<NodeScript[]>([]);
  const [sitePreviews, setSitePreviews] = useState<Record<string, SitePreview>>({});
  const [hostHistory, setHostHistory] = useState<HostHistoryEntry[]>(() => {
    try {
      const value = JSON.parse(localStorage.getItem("localstack.hostHistory") ?? "[]");
      return Array.isArray(value) ? value : [];
    } catch {
      return [];
    }
  });
  const [wizardType, setWizardType] = useState("WordPress");
  const [wizardDomain, setWizardDomain] = useState("new.test");
  const [wizardFolder, setWizardFolder] = useState(`${state.settings.projectsFolder}\\new`);
  const [wizardDatabase, setWizardDatabase] = useState("new_db");
  const [wizardSsl, setWizardSsl] = useState(false);
  const favorites = useStoredList("localstack.favoriteHosts");
  const [compactRows, setCompactRows] = useStoredBoolean("localstack.hosts.compactRows", false);
  useEffect(() => {
    if (selected && !state.hosts.some((item) => item.id === selected.id)) {
      setSelected(state.hosts[0]);
    }
  }, [selected, setSelected, state.hosts]);
  useEffect(() => {
    let cancelled = false;
    const check = async () => {
      const targets = state.hosts
        .filter((item) => favorites.items.includes(item.id) || item.id === host?.id)
        .slice(0, 3);
      if (targets.length === 0) return;
      const updates: Record<string, SitePreview> = {};
      await Promise.allSettled(targets.map(async (item) => {
        try {
          updates[item.id] = await api.previewHost(item.id);
        } catch {
          // Live checks are advisory and must stay quiet.
        }
      }));
      if (!cancelled) setSitePreviews((current) => ({ ...current, ...updates }));
    };
    const initial = window.setTimeout(() => void check(), 2500);
    const timer = window.setInterval(check, 300000);
    return () => {
      cancelled = true;
      window.clearTimeout(initial);
      window.clearInterval(timer);
    };
  }, [favorites.items, host?.id, state.hosts]);
  const importHosts = async () => {
    const path = await pickJsonFile();
    if (path) {
      await run(() => api.importHosts(path), { label: "Importing hosts..." });
    }
  };
  const exportHosts = async () => {
    const path = await saveJsonFile(`${state.appDataDir}\\hosts-export.json`);
    if (path) {
      await run(() => api.exportHosts(path), { label: "Exporting hosts..." });
    }
  };
  const backupHost = async (target: HostInfo) => {
    const path = await saveZipFile(`${state.settings.backupsFolder}\\${target.domain}-host-backup.zip`);
    if (path) await run(() => api.backupHost(target.id, path), { label: `Backing up ${target.domain}...` });
  };
  const restoreHost = async () => {
    const path = await pickZipFile();
    if (path) await run(() => api.restoreHostBackup(path), { label: "Restoring host backup..." });
  };
  const diagnose = async (target: HostInfo) => {
    const report = await run(() => api.diagnoseHost(target.id), { label: `Diagnosing ${target.domain}...` });
    if (report && typeof report === "object" && "hostId" in report) {
      setDiagnostics(report);
    }
  };
  const repair = async (target: HostInfo) => {
    const report = await run(() => api.repairHost(target.id), { label: `Repairing ${target.domain}...` });
    if (report && typeof report === "object" && "hostId" in report) {
      setDiagnostics(report);
    }
  };
  const testHostDatabase = async (target: HostInfo) => {
    const database = state.databases.find((item) => item.id === target.database || item.name === target.database);
    if (!database) return;
    const report = await run(() => api.testDatabaseConnection(database.id), { label: `Testing ${target.domain} database...` });
    if (report && typeof report === "object" && "databaseId" in report) setDbDiagnostics(report as DatabaseDiagnosticReport);
  };
  const loadHostLogs = async (target: HostInfo) => {
    const result = await run(() => api.tailLogFile(`host:${target.domain}`, 120), { label: `Reading logs for ${target.domain}...` });
    if (result && typeof result === "object" && "lines" in result) setHostLogs(result as LogFileTail);
  };
  const loadNodeScripts = async (target: HostInfo) => {
    const result = await run(() => api.listNodeScripts(target.rootFolder), { label: `Reading package.json for ${target.domain}...` });
    if (Array.isArray(result)) setNodeScripts(result as NodeScript[]);
  };
  const deleteSelected = async (target: HostInfo) => {
    const next = state.hosts.find((item) => item.id !== target.id);
    await run(() => api.deleteHost(target.id), { label: `Deleting ${target.domain}...` });
    setDiagnostics(null);
    setSelected(next);
  };
  const quickUpdateHost = async (patch: Partial<HostInfo>) => {
    if (!host) return;
    const entry = { id: crypto.randomUUID(), host, changedAt: new Date().toISOString(), summary: Object.keys(patch).join(", ") };
    const nextHistory = [entry, ...hostHistory].slice(0, 40);
    setHostHistory(nextHistory);
    localStorage.setItem("localstack.hostHistory", JSON.stringify(nextHistory));
    await run(() => api.saveHost({ ...host, ...patch, updatedAt: new Date().toISOString() }), { label: `Saving ${host.domain}...` });
  };
  const rollbackHost = async () => {
    if (!host) return;
    const entry = hostHistory.find((item) => item.host.id === host.id);
    if (!entry) return;
    await run(() => api.saveHost({ ...entry.host, updatedAt: new Date().toISOString() }), { label: `Rolling back ${host.domain}...` });
    const next = hostHistory.filter((item) => item.id !== entry.id);
    setHostHistory(next);
    localStorage.setItem("localstack.hostHistory", JSON.stringify(next));
  };
  const startRequiredForHost = async (target: HostInfo) => {
    const ids = new Set<string>();
    ids.add(target.webServer.toLowerCase().includes("nginx") ? "nginx" : "apache");
    if (target.database) ids.add("mysql");
    if (target.mailService) ids.add("mailpit");
    if (target.tags.some((tag) => tag.toLowerCase().includes("redis"))) ids.add("redis");
    await run(() => api.startServiceProfile(Array.from(ids)), { label: `Starting required services for ${target.domain}...` });
  };
  const resetFilters = () => {
    setQuery("");
    setStatus("All Status");
    setEnvironment("All Environments");
    setPhpVersion("All PHP Versions");
    setSsl("SSL: All");
    setReadinessFilter("All Readiness");
    setFilterMenuOpen(false);
  };
  const createWizardHost = async () => {
    const slug = wizardDomain.split(".")[0].replace(/[^a-z0-9_-]+/gi, "").toLowerCase() || "site";
    const now = new Date().toISOString();
    const next: HostInfo = {
      id: wizardDomain.trim().toLowerCase(),
      domain: wizardDomain.trim().toLowerCase(),
      rootFolder: wizardFolder.trim(),
      documentRoot: wizardType.includes("Node") || wizardType === "Static" ? "." : "public",
      phpVersion: state.phpVersions.find((php) => php.default)?.version ?? state.phpVersions[0]?.version ?? "8.3",
      webServer: wizardType.includes("Node") ? "Apache" : "Apache",
      ssl: wizardSsl,
      environment: "Development",
      httpPort: 80,
      httpsPort: 443,
      database: wizardDatabase.trim(),
      mailService: "Mailpit",
      envVariables: {
        APP_ENV: "local",
        APP_DEBUG: "true",
        APP_URL: `${wizardSsl ? "https" : "http"}://${wizardDomain.trim().toLowerCase()}`,
        DB_DATABASE: wizardDatabase.trim(),
        DB_USERNAME: `${slug}_user`,
        DB_PASSWORD: "localstack"
      },
      rewriteRules: "",
      notes: `${wizardType} host created by Host Wizard.`,
      status: "stopped",
      tags: [wizardType.toLowerCase().replace(/[^a-z0-9]+/g, "-")],
      createdAt: now,
      updatedAt: now
    };
    if (wizardDatabase.trim()) {
      const database: DatabaseInfo = {
        id: wizardDatabase.trim(),
        name: wizardDatabase.trim(),
        description: `${next.domain} database`,
        engine: "MySQL",
        version: "8.0.36",
        schemas: 1,
        user: `${slug}_user`,
        password: "localstack",
        port: 3306,
        status: "stopped",
        sizeMb: 0,
        createdAt: now
      };
      if (!state.databases.some((item) => item.id === database.id || item.name === database.name)) {
        await run(() => api.createDatabase(database), { label: `Creating database ${database.name}...` }).catch(() => undefined);
      }
    }
    await run(() => api.saveHost(next), { label: `Creating host ${next.domain}...` });
    setSelected(next);
  };
  const filteredHosts = useMemo(() => state.hosts.filter((item) => {
    const matchesQuery = !query.trim() || `${item.domain} ${item.rootFolder} ${item.tags.join(" ")}`.toLowerCase().includes(query.toLowerCase());
    const matchesStatus = status === "All Status" || item.status === status.toLowerCase();
    const matchesEnvironment = environment === "All Environments" || item.environment === environment;
    const matchesPhp = phpVersion === "All PHP Versions" || item.phpVersion === phpVersion;
    const matchesSsl = ssl === "SSL: All" || (ssl === "SSL: Enabled" ? item.ssl : !item.ssl);
    const score = hostReadinessScore(state, item);
    const matchesReadiness = readinessFilter === "All Readiness"
      || (readinessFilter === "Ready" && score === 100)
      || (readinessFilter === "Needs Attention" && score < 100);
    return matchesQuery && matchesStatus && matchesEnvironment && matchesPhp && matchesSsl && matchesReadiness;
  }).sort((a, b) => Number(favorites.items.includes(b.id)) - Number(favorites.items.includes(a.id)) || a.domain.localeCompare(b.domain)), [environment, favorites.items, phpVersion, query, readinessFilter, ssl, state, status]);
  return (
    <div className="page-grid">
      <section>
        <div className="page-title">
          <h1>{t("Hosts")} <small>{state.hosts.length}</small></h1>
          <div className="toolbar">
            <Button variant="primary" icon={<Plus size={16} />} onClick={() => editHost()}>
              {t("New Host")}
            </Button>
            <Button icon={<Copy size={16} />} disabled={!host} onClick={() => host && void run(() => api.duplicateHost(host.id), { label: `Duplicating ${host.domain}...` })}>
              {t("Duplicate")}
            </Button>
            <Button icon={<Download size={16} />} onClick={() => void importHosts()}>{t("Import")}</Button>
            <Button icon={<Upload size={16} />} onClick={() => void exportHosts()}>
              {t("Export")}
            </Button>
            <Button icon={<Upload size={16} />} disabled={!host} onClick={() => host && void backupHost(host)}>
              {t("Backup Host")}
            </Button>
            <Button icon={<Download size={16} />} onClick={() => void restoreHost()}>
              {t("Restore Host")}
            </Button>
            <Button icon={<ShieldCheck size={16} />} disabled={!host} onClick={() => host && void diagnose(host)}>
              {t("Diagnose")}
            </Button>
            <Button icon={<Wrench size={16} />} disabled={!host} onClick={() => host && void repair(host)}>
              {t("Repair Host")}
            </Button>
              <Button variant="danger" icon={<Trash2 size={16} />} disabled={!host} onClick={() => host && void deleteSelected(host)}>
              {t("Delete")}
            </Button>
            <Button icon={<ShieldCheck size={16} />} onClick={() => void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." })}>
              {t("Sync Hosts File")}
            </Button>
            <Button icon={<Star size={16} />} onClick={() => host && favorites.toggle(host.id)} disabled={!host}>
              {host && favorites.items.includes(host.id) ? t("Unpin") : t("Pin")}
            </Button>
          </div>
        </div>
        <Panel title={t("Host Wizard")}>
          <div className="wizard-inline">
            <select value={wizardType} onChange={(event) => setWizardType(event.target.value)}>{["WordPress", "Laravel", "Symfony", "Custom PHP", "Static", "Node.js", "Next.js"].map((item) => <option key={item}>{item}</option>)}</select>
            <input value={wizardDomain} onChange={(event) => {
              const value = event.target.value;
              setWizardDomain(value);
              const slug = value.split(".")[0].replace(/[^a-z0-9_-]+/gi, "").toLowerCase();
              setWizardFolder(`${state.settings.projectsFolder}\\${slug || "new"}`);
              setWizardDatabase(`${slug || "new"}_db`);
            }} />
            <input value={wizardFolder} onChange={(event) => setWizardFolder(event.target.value)} />
            <input value={wizardDatabase} onChange={(event) => setWizardDatabase(event.target.value)} />
            <label className="inline-toggle"><span className={`toggle ${wizardSsl ? "on" : ""}`} onClick={() => setWizardSsl((value) => !value)} />SSL</label>
            <Button variant="primary" icon={<Plus size={16} />} onClick={() => void createWizardHost()}>{t("Create Host")}</Button>
          </div>
        </Panel>
        <Panel>
          <div className="filters">
            <label className="search">
              <Search size={17} />
              <input placeholder={t("Search hosts...")} value={query} onChange={(event) => setQuery(event.target.value)} />
            </label>
            <select value={status} onChange={(event) => setStatus(event.target.value)}><option value="All Status">{t("All Status")}</option><option value="running">{t("running")}</option><option value="stopped">{t("stopped")}</option><option value="error">{t("error")}</option></select>
            <select value={environment} onChange={(event) => setEnvironment(event.target.value)}><option value="All Environments">{t("All Environments")}</option>{Array.from(new Set(state.hosts.map((item) => item.environment))).map((item) => <option key={item} value={item}>{t(item)}</option>)}</select>
            <select value={phpVersion} onChange={(event) => setPhpVersion(event.target.value)}><option value="All PHP Versions">{t("All PHP Versions")}</option>{state.phpVersions.map((item) => <option key={item.version}>{item.version}</option>)}</select>
            <select value={ssl} onChange={(event) => setSsl(event.target.value)}><option value="SSL: All">{t("SSL: All")}</option><option value="SSL: Enabled">{t("SSL: Enabled")}</option><option value="SSL: Disabled">{t("SSL: Disabled")}</option></select>
            <div className="menu-anchor">
              <Button icon={<Filter size={16} />} aria-label={t("Open filter presets")} onClick={() => setFilterMenuOpen((value) => !value)}>{t("Filters")}</Button>
              {filterMenuOpen && (
                <div className="action-menu" onMouseLeave={() => setFilterMenuOpen(false)}>
                  <button onClick={resetFilters}>{t("Reset Filters")}</button>
                  <button onClick={() => { setStatus("running"); setFilterMenuOpen(false); }}>{t("Running Hosts")}</button>
                  <button onClick={() => { setSsl("SSL: Enabled"); setFilterMenuOpen(false); }}>{t("SSL: Enabled")}</button>
                  <button onClick={() => { setReadinessFilter("Needs Attention"); setFilterMenuOpen(false); }}>{t("Needs Attention")}</button>
                </div>
              )}
            </div>
            <Button onClick={() => setCompactRows((value) => !value)}>{compactRows ? t("Comfortable Rows") : t("Compact Rows")}</Button>
          </div>
          <table className={`data-table hosts-table ${compactRows ? "compact-table" : ""}`}>
            <thead>
              <tr>
                <th></th>
                <th>{t("Host")}</th>
                <th>{t("Status")}</th>
                <th>{t("Environment")}</th>
                <th>{t("PHP Version")}</th>
                <th>SSL</th>
                <th>{t("Readiness")}</th>
                <th>{t("Tags")}</th>
                <th>{t("Updated")}</th>
                <th>{t("Actions")}</th>
              </tr>
            </thead>
            <tbody>
              {filteredHosts.map((row) => {
                const score = hostReadinessScore(state, row);
                return (
                <tr key={row.id} className={host?.id === row.id ? "selected" : ""} onClick={() => setSelected(row)}>
                  <td><input type="checkbox" checked={host?.id === row.id} readOnly /></td>
                  <td><button className={`star-button ${favorites.items.includes(row.id) ? "active" : ""}`} onClick={(event) => { event.stopPropagation(); favorites.toggle(row.id); }}><Star size={14} /></button><strong>{row.domain}</strong><small>{hostUrl(row)}</small></td>
                  <td><StatusDot status={row.status} /></td>
                  <td><span className="pill blue">{t(row.environment)}</span></td>
                  <td>{row.phpVersion}</td>
                  <td><span className={row.ssl ? "green-text" : "muted"}>{t(row.ssl ? "Valid" : "Disabled")}</span></td>
                  <td><span className={`score-pill ${readinessClass(score)}`}>{score}%</span>{sitePreviews[row.id] && <small>{sitePreviews[row.id].status}</small>}</td>
                  <td>{row.tags.map((tag) => <span className="pill" key={tag}>{tag}</span>)}</td>
                  <td>{t("2m ago")}</td>
                  <td className="row-actions">
                    <Button variant="icon" icon={<Play size={15} />} onClick={(event) => { event.stopPropagation(); void run(() => api.openHost(row.id), { label: `Opening ${row.domain}...` }); }} />
                    <Button variant="icon" icon={<Folder size={15} />} onClick={(event) => { event.stopPropagation(); void run(() => api.openPath(row.rootFolder), { label: `Opening ${row.rootFolder}...` }); }} />
                    <div className="menu-anchor">
                      <Button variant="icon" aria-label="More host actions" icon={<MoreVertical size={15} />} onClick={(event) => { event.stopPropagation(); setOpenMenuId(openMenuId === row.id ? null : row.id); }} />
                      {openMenuId === row.id && (
                        <div className="action-menu" onMouseLeave={() => setOpenMenuId(null)}>
                          <button onClick={(event) => { event.stopPropagation(); setOpenMenuId(null); editHost(row); }}>{t("Edit")}</button>
                          <button onClick={(event) => { event.stopPropagation(); setOpenMenuId(null); void run(() => api.duplicateHost(row.id), { label: `Duplicating ${row.domain}...` }); }}>{t("Duplicate")}</button>
                          <button onClick={(event) => { event.stopPropagation(); setOpenMenuId(null); void diagnose(row); }}>{t("Diagnose")}</button>
                          <button onClick={(event) => { event.stopPropagation(); setOpenMenuId(null); void repair(row); }}>{t("Repair Host")}</button>
                          <button onClick={(event) => { event.stopPropagation(); setOpenMenuId(null); void deleteSelected(row); }}>{t("Delete")}</button>
                        </div>
                      )}
                    </div>
                  </td>
                </tr>
                );
              })}
            </tbody>
          </table>
          <div className="table-foot"><span>{host ? `1 ${t("selected")}` : `0 ${t("selected")}`}</span><span>{t("Rows")}: {filteredHosts.length} {t("of")} {state.hosts.length}</span></div>
        </Panel>
      </section>
      {host && (
        <aside className="detail-rail">
          <Panel title={host.domain} action={<><StatusDot status={host.status} /><Button variant="icon" icon={<MoreVertical size={16} />} onClick={() => editHost(host)} /></>}>
            <div className="readiness-block">
              <span className={`score-pill ${readinessClass(hostReadinessScore(state, host))}`}>{hostReadinessScore(state, host)}%</span>
              <strong>{t(readinessLabel(hostReadinessScore(state, host)))}</strong>
            </div>
            <div className="tabs">{["General", "Domains", "Paths", "Environment Variables", "Rewrite Rules", "Integrations"].map((item) => <button key={item} className={tab === item ? "active" : ""} onClick={() => setTab(item)}>{t(item)}</button>)}</div>
            <div className="kv detail-kv">
              <span>{t("Domain")}</span><strong>{host.domain}</strong>
              <span>URL</span><button onClick={() => void run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` })}>{hostUrl(host)}<ExternalLink size={16} /></button>
              <span>{t("Environment")}</span><span className="pill blue">{t(host.environment)}</span>
              <span>{t("PHP Version")}</span><strong>{host.phpVersion}</strong>
              <span>{t("Web Server")}</span><strong>{host.webServer}</strong>
              <span>{t("Document Root")}</span><strong>{host.rootFolder}\\{host.documentRoot}</strong>
              <span>{t("Status")}</span><StatusDot status={host.status} />
              <span>{t("Tags")}</span><div>{host.tags.map((tag) => <span className="pill" key={tag}>{tag}</span>)}</div>
            </div>
            <div className="form-grid two quick-host-edit">
              <label>
                PHP
                <select value={host.phpVersion} onChange={(event) => void quickUpdateHost({ phpVersion: event.target.value })}>
                  {state.phpVersions.map((php) => <option key={php.version}>{php.version}</option>)}
                </select>
              </label>
              <label>
                {t("Document Root")}
                <input key={host.id} defaultValue={host.documentRoot} onBlur={(event) => event.target.value !== host.documentRoot && void quickUpdateHost({ documentRoot: event.target.value })} />
              </label>
              <label className="toggle-line">
                SSL
                <span className={`toggle ${host.ssl ? "on" : ""}`} onClick={() => void quickUpdateHost({ ssl: !host.ssl })} />
              </label>
            </div>
            {tab !== "General" && <p className="muted">{host.domain}: {t(tab)} {t("settings are stored in its host configuration and can be edited with the Edit action.")}</p>}
          </Panel>
          <Panel title={t("Quick Actions")}>
            <div className="quick-grid">
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` })}>{t("Open in Browser")}</Button>
              <Button icon={<Folder size={17} />} onClick={() => void run(() => api.openPath(host.rootFolder), { label: `Opening ${host.rootFolder}...` })}>{t("Open Root Folder")}</Button>
              <Button icon={<Trash2 size={17} />} onClick={() => void run(() => api.openPath(`${host.rootFolder}\\logs`), { label: `Opening logs for ${host.domain}...` })}>{t("View Logs")}</Button>
              <Button icon={<ShieldCheck size={17} />} onClick={() => void diagnose(host)}>{t("Diagnose")}</Button>
              <Button icon={<Wrench size={17} />} onClick={() => void repair(host)}>{t("Repair Host")}</Button>
              <Button icon={<Database size={17} />} onClick={() => void testHostDatabase(host)}>{t("Test Database")}</Button>
              <Button icon={<Terminal size={17} />} onClick={() => void loadNodeScripts(host)}>{t("Node Scripts")}</Button>
              <Button icon={<Play size={17} />} onClick={() => void startRequiredForHost(host)}>{t("Start Required")}</Button>
              <Button icon={<Wrench size={17} />} disabled={!hostHistory.some((item) => item.host.id === host.id)} onClick={() => void rollbackHost()}>{t("Rollback Change")}</Button>
              <Button icon={<Folder size={17} />} onClick={() => void loadHostLogs(host)}>{t("Host Logs")}</Button>
              <Button icon={<Upload size={17} />} onClick={() => void backupHost(host)}>{t("Backup Host")}</Button>
              <Button icon={<Download size={17} />} onClick={() => void restoreHost()}>{t("Restore Host")}</Button>
              <Button icon={<ShieldCheck size={17} />} onClick={() => void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." })}>{t("Sync Hosts File")}</Button>
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.openPath(`${state.appDataDir}\\certs`), { label: "Opening certificates folder..." })}>{t("SSL Certificate")}</Button>
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.runProjectCommand(host.rootFolder, "npm-install"), { label: `Running npm install for ${host.domain}...` })}>npm install</Button>
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.runProjectCommand(host.rootFolder, "npm-dev"), { label: `Starting Node dev server for ${host.domain}...` })}>npm run dev</Button>
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.runProjectCommand(host.rootFolder, "composer-install"), { label: `Running composer install for ${host.domain}...` })}>composer install</Button>
            </div>
          </Panel>
          <Panel title={t("Dependency Map")}>
            <div className="dependency-map">
              <span><strong>{host.domain}</strong><small>{t("Host")}</small></span>
              <span><strong>{host.webServer}</strong><small>{t("Web Server")}</small></span>
              <span><strong>PHP {host.phpVersion}</strong><small>{t("Runtime")}</small></span>
              <span><strong>{host.database || t("None")}</strong><small>{t("Database")}</small></span>
              <span><strong>{t(host.ssl ? "SSL on" : "SSL off")}</strong><small>{t("Certificate")}</small></span>
            </div>
          </Panel>
          <Panel title={t("Change History")}>
            <div className="content-results compact-results">
              {hostHistory.filter((item) => item.host.id === host.id).slice(0, 6).map((entry) => (
                <button key={entry.id} onClick={() => void run(() => api.saveHost({ ...entry.host, updatedAt: new Date().toISOString() }), { label: `Restoring ${entry.host.domain}...` })}>
                  <strong>{entry.summary}</strong>
                  <span>{new Date(entry.changedAt).toLocaleString()}</span>
                </button>
              ))}
              {!hostHistory.some((item) => item.host.id === host.id) && <div className="empty-row">{t("No changes yet.")}</div>}
            </div>
          </Panel>
          {diagnostics?.hostId === host.id && (
            <Panel title={t("Host Diagnostics")} action={<StatusDot status={diagnostics.errors > 0 ? "error" : diagnostics.warnings > 0 ? "warning" : "valid"} label={diagnostics.errors > 0 ? t("Issues") : diagnostics.warnings > 0 ? t("Warnings") : t("Healthy")} />}>
              <div className="kv detail-kv">
                <span>{t("Summary")}</span><strong>{diagnostics.summary}</strong>
                <span>{t("Checks")}</span><strong>{diagnostics.ok} OK / {diagnostics.warnings} {t("Warning")} / {diagnostics.errors} {t("Error")}</strong>
              </div>
              <table className="data-table diagnostics-table">
                <tbody>
                  {diagnostics.checks.map((check) => (
                    <tr key={check.id}>
                      <td><StatusDot status={check.severity === "ok" ? "valid" : check.severity} label={check.title} /></td>
                      <td><strong>{check.message}</strong>{check.detail && <small>{check.detail}</small>}{check.action && <small>{check.action}</small>}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Panel>
          )}
          {dbDiagnostics && (
            <Panel title={t("Database Connection")} action={<StatusDot status={dbDiagnostics.errors > 0 ? "error" : dbDiagnostics.warnings > 0 ? "warning" : "valid"} />}>
              <div className="kv detail-kv"><span>{t("Database")}</span><strong>{dbDiagnostics.database}</strong><span>{t("Summary")}</span><strong>{dbDiagnostics.summary}</strong></div>
            </Panel>
          )}
          {nodeScripts.length > 0 && (
            <Panel title={t("Node App Manager")}>
              <div className="script-list">
                {nodeScripts.map((script) => (
                  <button key={script.name} onClick={() => void run(() => api.runNodeScript(host.rootFolder, script.name), { label: `Running npm run ${script.name}...` })}>
                    <Terminal size={15} />
                    <strong>{script.name}</strong>
                    <small>{script.command}</small>
                  </button>
                ))}
              </div>
            </Panel>
          )}
          {hostLogs && (
            <Panel title="Host Logs">
              <div className="log-box compact-log">{hostLogs.lines.slice(-20).map((line, index) => <pre key={index}>{line}</pre>)}</div>
            </Panel>
          )}
          <Panel title={t("Notes")} action={<Button onClick={() => editHost(host)}>{t("Edit")}</Button>}>
            <p className="muted">{t(host.notes)}</p>
          </Panel>
        </aside>
      )}
    </div>
  );
}
