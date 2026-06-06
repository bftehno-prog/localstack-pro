import { Copy, Download, ExternalLink, Filter, Folder, MoreVertical, Play, Plus, Search, ShieldCheck, Trash2, Upload, Wrench } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../ui/api";
import { pickJsonFile, saveJsonFile } from "../ui/dialogs";
import type { AppRun, AppSnapshot, HostDiagnosticReport, HostInfo } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { hostUrl } from "./Overview";
import { hostReadinessScore, readinessClass, readinessLabel } from "../ui/readiness";

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
  const host = selected ?? state.hosts[0];
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("All Status");
  const [environment, setEnvironment] = useState("All Environments");
  const [phpVersion, setPhpVersion] = useState("All PHP Versions");
  const [ssl, setSsl] = useState("SSL: All");
  const [readinessFilter, setReadinessFilter] = useState("All Readiness");
  const [filterMenuOpen, setFilterMenuOpen] = useState(false);
  const [tab, setTab] = useState("General");
  const [diagnostics, setDiagnostics] = useState<HostDiagnosticReport | null>(null);
  useEffect(() => {
    if (selected && !state.hosts.some((item) => item.id === selected.id)) {
      setSelected(state.hosts[0]);
    }
  }, [selected, setSelected, state.hosts]);
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
  const deleteSelected = async (target: HostInfo) => {
    const next = state.hosts.find((item) => item.id !== target.id);
    await run(() => api.deleteHost(target.id), { label: `Deleting ${target.domain}...` });
    setDiagnostics(null);
    setSelected(next);
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
  }), [environment, phpVersion, query, readinessFilter, ssl, state, status]);
  return (
    <div className="page-grid">
      <section>
        <div className="page-title">
          <h1>Hosts <small>{state.hosts.length}</small></h1>
          <div className="toolbar">
            <Button variant="primary" icon={<Plus size={16} />} onClick={() => editHost()}>
              New Host
            </Button>
            <Button icon={<Copy size={16} />} disabled={!host} onClick={() => host && void run(() => api.duplicateHost(host.id), { label: `Duplicating ${host.domain}...` })}>
              Duplicate
            </Button>
            <Button icon={<Download size={16} />} onClick={() => void importHosts()}>Import</Button>
            <Button icon={<Upload size={16} />} onClick={() => void exportHosts()}>
              Export
            </Button>
            <Button icon={<ShieldCheck size={16} />} disabled={!host} onClick={() => host && void diagnose(host)}>
              Diagnose
            </Button>
            <Button icon={<Wrench size={16} />} disabled={!host} onClick={() => host && void repair(host)}>
              Repair Host
            </Button>
            <Button variant="danger" icon={<Trash2 size={16} />} disabled={!host} onClick={() => host && void deleteSelected(host)}>
              Delete
            </Button>
            <Button icon={<ShieldCheck size={16} />} onClick={() => void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." })}>
              Sync Hosts File
            </Button>
          </div>
        </div>
        <Panel>
          <div className="filters">
            <label className="search">
              <Search size={17} />
              <input placeholder="Search hosts..." value={query} onChange={(event) => setQuery(event.target.value)} />
            </label>
            <select value={status} onChange={(event) => setStatus(event.target.value)}><option>All Status</option><option>running</option><option>stopped</option><option>error</option></select>
            <select value={environment} onChange={(event) => setEnvironment(event.target.value)}><option>All Environments</option>{Array.from(new Set(state.hosts.map((item) => item.environment))).map((item) => <option key={item}>{item}</option>)}</select>
            <select value={phpVersion} onChange={(event) => setPhpVersion(event.target.value)}><option>All PHP Versions</option>{state.phpVersions.map((item) => <option key={item.version}>{item.version}</option>)}</select>
            <select value={ssl} onChange={(event) => setSsl(event.target.value)}><option>SSL: All</option><option>SSL: Enabled</option><option>SSL: Disabled</option></select>
            <div className="menu-anchor">
              <Button icon={<Filter size={16} />} aria-label="Open filter presets" onClick={() => setFilterMenuOpen((value) => !value)}>Filters</Button>
              {filterMenuOpen && (
                <div className="action-menu" onMouseLeave={() => setFilterMenuOpen(false)}>
                  <button onClick={resetFilters}>Reset Filters</button>
                  <button onClick={() => { setStatus("running"); setFilterMenuOpen(false); }}>Running Hosts</button>
                  <button onClick={() => { setSsl("SSL: Enabled"); setFilterMenuOpen(false); }}>SSL Enabled</button>
                  <button onClick={() => { setReadinessFilter("Needs Attention"); setFilterMenuOpen(false); }}>Needs Attention</button>
                </div>
              )}
            </div>
          </div>
          <table className="data-table hosts-table">
            <thead>
              <tr>
                <th></th>
                <th>Host</th>
                <th>Status</th>
                <th>Environment</th>
                <th>PHP Version</th>
                <th>SSL</th>
                <th>Readiness</th>
                <th>Tags</th>
                <th>Updated</th>
                <th>Actions</th>
              </tr>
            </thead>
            <tbody>
              {filteredHosts.map((row) => {
                const score = hostReadinessScore(state, row);
                return (
                <tr key={row.id} className={host?.id === row.id ? "selected" : ""} onClick={() => setSelected(row)}>
                  <td><input type="checkbox" checked={host?.id === row.id} readOnly /></td>
                  <td><strong>{row.domain}</strong><small>{hostUrl(row)}</small></td>
                  <td><StatusDot status={row.status} /></td>
                  <td><span className="pill blue">{row.environment}</span></td>
                  <td>{row.phpVersion}</td>
                  <td><span className={row.ssl ? "green-text" : "muted"}>{row.ssl ? "Valid" : "Disabled"}</span></td>
                  <td><span className={`score-pill ${readinessClass(score)}`}>{score}%</span></td>
                  <td>{row.tags.map((tag) => <span className="pill" key={tag}>{tag}</span>)}</td>
                  <td>2m ago</td>
                  <td className="row-actions">
                    <Button variant="icon" icon={<Play size={15} />} onClick={(event) => { event.stopPropagation(); void run(() => api.openHost(row.id), { label: `Opening ${row.domain}...` }); }} />
                    <Button variant="icon" icon={<Folder size={15} />} onClick={(event) => { event.stopPropagation(); void run(() => api.openPath(row.rootFolder), { label: `Opening ${row.rootFolder}...` }); }} />
                    <Button variant="icon" icon={<MoreVertical size={15} />} onClick={(event) => { event.stopPropagation(); editHost(row); }} />
                  </td>
                </tr>
                );
              })}
            </tbody>
          </table>
          <div className="table-foot"><span>{host ? "1 selected" : "0 selected"}</span><span>Rows: {filteredHosts.length} of {state.hosts.length}</span></div>
        </Panel>
      </section>
      {host && (
        <aside className="detail-rail">
          <Panel title={host.domain} action={<><StatusDot status={host.status} /><Button variant="icon" icon={<MoreVertical size={16} />} onClick={() => editHost(host)} /></>}>
            <div className="readiness-block">
              <span className={`score-pill ${readinessClass(hostReadinessScore(state, host))}`}>{hostReadinessScore(state, host)}%</span>
              <strong>{readinessLabel(hostReadinessScore(state, host))}</strong>
            </div>
            <div className="tabs">{["General", "Domains", "Paths", "Environment Variables", "Rewrite Rules", "Integrations"].map((item) => <button key={item} className={tab === item ? "active" : ""} onClick={() => setTab(item)}>{item}</button>)}</div>
            <div className="kv detail-kv">
              <span>Domain</span><strong>{host.domain}</strong>
              <span>URL</span><button onClick={() => void run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` })}>{hostUrl(host)}<ExternalLink size={16} /></button>
              <span>Environment</span><span className="pill blue">{host.environment}</span>
              <span>PHP Version</span><strong>{host.phpVersion}</strong>
              <span>Web Server</span><strong>{host.webServer}</strong>
              <span>Document Root</span><strong>{host.rootFolder}\\{host.documentRoot}</strong>
              <span>Status</span><StatusDot status={host.status} />
              <span>Tags</span><div>{host.tags.map((tag) => <span className="pill" key={tag}>{tag}</span>)}</div>
            </div>
            {tab !== "General" && <p className="muted">{host.domain} {tab.toLowerCase()} settings are stored in its host configuration and can be edited with the Edit action.</p>}
          </Panel>
          <Panel title="Quick Actions">
            <div className="quick-grid">
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` })}>Open in Browser</Button>
              <Button icon={<Folder size={17} />} onClick={() => void run(() => api.openPath(host.rootFolder), { label: `Opening ${host.rootFolder}...` })}>Open Root Folder</Button>
              <Button icon={<Trash2 size={17} />} onClick={() => void run(() => api.openPath(`${host.rootFolder}\\logs`), { label: `Opening logs for ${host.domain}...` })}>View Logs</Button>
              <Button icon={<ShieldCheck size={17} />} onClick={() => void diagnose(host)}>Diagnose</Button>
              <Button icon={<Wrench size={17} />} onClick={() => void repair(host)}>Repair Host</Button>
              <Button icon={<ShieldCheck size={17} />} onClick={() => void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." })}>Sync Hosts File</Button>
              <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => api.openPath(`${state.appDataDir}\\certs`), { label: "Opening certificates folder..." })}>SSL Certificate</Button>
            </div>
          </Panel>
          {diagnostics?.hostId === host.id && (
            <Panel title="Host Diagnostics" action={<StatusDot status={diagnostics.errors > 0 ? "error" : diagnostics.warnings > 0 ? "warning" : "valid"} label={diagnostics.errors > 0 ? "Issues" : diagnostics.warnings > 0 ? "Warnings" : "Healthy"} />}>
              <div className="kv detail-kv">
                <span>Summary</span><strong>{diagnostics.summary}</strong>
                <span>Checks</span><strong>{diagnostics.ok} OK / {diagnostics.warnings} Warning / {diagnostics.errors} Error</strong>
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
          <Panel title="Notes" action={<Button onClick={() => editHost(host)}>Edit</Button>}>
            <p className="muted">{host.notes}</p>
          </Panel>
        </aside>
      )}
    </div>
  );
}
