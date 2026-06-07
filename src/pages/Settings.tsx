import { Bell, Database, Download, Folder, Globe2, Link, RefreshCw, Save, Settings as SettingsIcon, Upload, Wrench } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../ui/api";
import { pickJsonFile, pickZipFile, saveJsonFile, saveZipFile } from "../ui/dialogs";
import type { AppRun, AppSettings, AppSnapshot, EnvironmentSnapshotInfo, HealthReport, PortInspection, ReleaseInfo, ResourceProcess } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { useT } from "../ui/i18n";
import { useStoredBoolean } from "../ui/preferences";

const tabs = ["General", "Paths", "Startup", "Network", "Theme", "Notifications", "Integrations", "Updates", "Backups", "Advanced"];
const themeOptions = ["Light", "Pearl", "Graphite", "Azure", "Forest", "Dark", "Midnight", "Carbon", "High Contrast", "System"];

export function SettingsPage({
  state,
  run,
}: {
  state: AppSnapshot;
  run: AppRun;
}) {
  const t = useT();
  const [settings, setSettings] = useState<AppSettings>(state.settings);
  const [activeTab, setActiveTab] = useState("General");
  const [health, setHealth] = useState<HealthReport>();
  const [healthError, setHealthError] = useState("");
  const [ports, setPorts] = useState<PortInspection[]>([]);
  const [release, setRelease] = useState<ReleaseInfo>();
  const [snapshots, setSnapshots] = useState<EnvironmentSnapshotInfo[]>([]);
  const [processes, setProcesses] = useState<ResourceProcess[]>([]);
  const [snapshotName, setSnapshotName] = useState("working");
  const [downloadedInstaller, setDownloadedInstaller] = useState("");
  const [notificationLevel, setNotificationLevel] = useState(localStorage.getItem("localstack.notificationLevel") ?? "Errors only");
  const [scheduledBackups, setScheduledBackups] = useStoredBoolean("localstack.scheduledBackups", false);
  const saveTimer = useRef<number | undefined>(undefined);
  const availableThemeOptions = themeOptions.includes(settings.theme) || !settings.theme
    ? themeOptions
    : [settings.theme, ...themeOptions];
  useEffect(() => {
    setSettings(state.settings);
  }, [state.settings]);
  useEffect(() => () => {
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
  }, []);
  const save = (next: AppSettings, immediate = false) => {
    if (saveTimer.current) window.clearTimeout(saveTimer.current);
    const commit = () => void run(() => api.saveSettings(next), { silent: true });
    if (immediate) {
      commit();
    } else {
      saveTimer.current = window.setTimeout(commit, 900);
    }
  };
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K], immediate = false) => {
    const next = { ...settings, [key]: value };
    setSettings(next);
    save(next, immediate);
  };
  const runHealthCheck = async () => {
    setHealthError("");
    try {
      const report = await run(api.runHealthCheck, { label: "Running health check..." });
      if (report && typeof report === "object" && "checks" in report) {
        setHealth(report as HealthReport);
      }
    } catch (error) {
      setHealthError(error instanceof Error ? error.message : String(error));
    }
  };
  const scanPorts = async () => {
    const result = await run(api.scanPorts, { label: "Scanning service ports..." });
    if (Array.isArray(result)) setPorts(result as PortInspection[]);
  };
  const checkRelease = async () => {
    const result = await run(api.checkLatestRelease, { label: "Checking for updates..." });
    if (result && typeof result === "object" && "latestVersion" in result) setRelease(result as ReleaseInfo);
  };
  const downloadUpdate = async () => {
    const result = await run(api.downloadLatestReleaseInstaller, { label: "Downloading update installer..." });
    if (typeof result === "string") setDownloadedInstaller(result);
  };
  const createDiagnosticBundle = async () => {
    const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
    const path = await saveZipFile(`${settings.backupsFolder}\\localstack-diagnostics-${stamp}.zip`);
    if (path) await run(() => api.createDiagnosticBundle(path), { label: "Creating diagnostic bundle..." });
  };
  const updateNotificationLevel = (value: string) => {
    setNotificationLevel(value);
    localStorage.setItem("localstack.notificationLevel", value);
  };
  const repairAll = async () => {
    setHealthError("");
    try {
      await run(api.detectDependencies, { label: "Detecting installed dependencies..." });
      try {
        await run(api.syncHostsFile, { label: "Synchronizing Windows hosts file..." });
      } catch (error) {
        setHealthError(error instanceof Error ? error.message : String(error));
      }
      const report = await run(api.repairEnvironment, { label: "Repairing environment..." });
      const finalReport = await run(api.runHealthCheck, { label: "Running final health check..." });
      if (report && typeof report === "object" && "checks" in report) {
        setHealth(report as HealthReport);
      }
      if (finalReport && typeof finalReport === "object" && "checks" in finalReport) {
        setHealth(finalReport as HealthReport);
      }
    } catch (error) {
      setHealthError(error instanceof Error ? error.message : String(error));
    }
  };
  const importSettings = async () => {
    const path = await pickJsonFile();
    if (path) {
      await run(() => api.importSettings(path));
    }
  };
  const exportSettings = async () => {
    const path = await saveJsonFile(`${state.appDataDir}\\settings-export.json`);
    if (path) {
      await run(() => api.exportSettings(path));
    }
  };
  const createBackup = async () => {
    const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
    const path = await saveZipFile(`${settings.backupsFolder}\\localstack-pro-backup-${stamp}.zip`);
    if (path) {
      await run(() => api.createAppBackup(path), { label: "Creating application backup..." });
    }
  };
  const backupAllDatabases = async () => {
    for (const database of state.databases) {
      await run(() => api.backupDatabase(database.id), { label: `Backing up ${database.name}...`, serial: true });
    }
  };
  const restoreBackup = async () => {
    const path = await pickZipFile();
    if (path) {
      await run(() => api.restoreAppBackup(path), { label: "Restoring application backup..." });
    }
  };
  const loadSnapshots = async () => {
    const result = await run(api.listEnvironmentSnapshots, { label: "Loading environment snapshots..." });
    if (Array.isArray(result)) setSnapshots(result as EnvironmentSnapshotInfo[]);
  };
  const createSnapshot = async () => {
    if (!snapshotName.trim()) return;
    const result = await run(() => api.createEnvironmentSnapshot(snapshotName), { label: "Creating environment snapshot..." });
    if (result && typeof result === "object" && "id" in result) await loadSnapshots();
  };
  const loadProcesses = async () => {
    const result = await run(api.resourceMonitor, { label: "Reading resource monitor..." });
    if (Array.isArray(result)) setProcesses(result as ResourceProcess[]);
  };
  return (
    <div className="settings-page">
      <h1>{t("Settings")}</h1>
      <div className="tabs wide">{tabs.map((tab) => <button key={tab} className={activeTab === tab ? "active" : ""} onClick={() => setActiveTab(tab)}>{t(tab)}</button>)}</div>
      <div className="settings-grid">
        <section className="settings-main">
          {activeTab === "General" && <><Panel title="Application">
              <SettingSelect label="Language" value={settings.language} onChange={(value) => update("language", value, true)} options={["English (US)", "Russian"]} />
              <SettingSelect label="Preferred Browser" value={settings.preferredBrowser} onChange={(value) => update("preferredBrowser", value)} options={["Default System Browser", "Chrome", "Edge", "Firefox"]} />
              <SettingSelect label="UI Density" value={settings.uiDensity} onChange={(value) => update("uiDensity", value)} options={["Comfortable", "Compact"]} />
            </Panel><Panel title="Behavior">
              <Switch label="Minimize to System Tray" checked={settings.minimizeToTray} onChange={(value) => update("minimizeToTray", value)} />
              <Switch label="Close to System Tray" checked={settings.closeToTray} onChange={(value) => update("closeToTray", value)} />
              <Switch label="Enable Telemetry" checked={settings.telemetry} onChange={(value) => update("telemetry", value)} />
            </Panel></>}
          {activeTab === "Paths" && <Panel title="Paths">
            <SettingInput label="Projects Folder" value={settings.projectsFolder} onChange={(value) => update("projectsFolder", value)} />
            <SettingInput label="Services Folder" value={settings.servicesFolder} onChange={(value) => update("servicesFolder", value)} />
            <SettingInput label="Backups Folder" value={settings.backupsFolder} onChange={(value) => update("backupsFolder", value)} />
            <div className="toolbar"><Button icon={<Folder size={16} />} onClick={() => void run(() => api.openPath(settings.projectsFolder))}>Open Projects</Button><Button icon={<Folder size={16} />} onClick={() => void run(() => api.openPath(settings.servicesFolder))}>Open Services</Button><Button icon={<Folder size={16} />} onClick={() => void run(() => api.openPath(settings.backupsFolder))}>Open Backups</Button></div>
          </Panel>}
          {activeTab === "Startup" && <Panel title="Startup">
            <Switch label="Launch on Startup" checked={settings.launchOnStartup} onChange={(value) => update("launchOnStartup", value)} />
            <Switch label="Start Minimized to Tray" checked={settings.minimizeToTray} onChange={(value) => update("minimizeToTray", value)} />
            <Switch label="Close to System Tray" checked={settings.closeToTray} onChange={(value) => update("closeToTray", value)} />
          </Panel>}
          {activeTab === "Network" && <><Panel title="Network">
            <SettingNumber label="HTTP Port Start" value={settings.httpPortStart} onChange={(value) => update("httpPortStart", value)} />
            <SettingNumber label="HTTP Port End" value={settings.httpPortEnd} onChange={(value) => update("httpPortEnd", value)} />
            <Switch label="Proxy Enabled" checked={settings.proxyEnabled} onChange={(value) => update("proxyEnabled", value)} />
          </Panel><Panel title="Port Manager" action={<Button icon={<RefreshCw size={16} />} onClick={() => void scanPorts()}>Scan Ports</Button>}>
            <table className="data-table compact-table">
              <thead><tr><th>Port</th><th>Status</th><th>Service</th><th>PID</th><th>Process</th></tr></thead>
              <tbody>{ports.map((port) => <tr key={port.port}><td><strong>{port.port}</strong></td><td>{port.status}</td><td>{port.service ?? "-"}</td><td>{port.pid ?? "-"}</td><td>{port.process ?? port.action}</td></tr>)}</tbody>
            </table>
            {ports.length === 0 && <p className="muted">{t("Click Scan Ports to inspect local service ports.")}</p>}
          </Panel></>}
          {activeTab === "Theme" && <Panel title="Theme">
            <SettingSelect label="Theme" value={settings.theme} onChange={(value) => update("theme", value, true)} options={availableThemeOptions} />
            <SettingSelect label="UI Density" value={settings.uiDensity} onChange={(value) => update("uiDensity", value)} options={["Comfortable", "Compact"]} />
          </Panel>}
          {activeTab === "Notifications" && <Panel title="Notifications">
            <Switch label="Show Notifications" checked={settings.showNotifications} onChange={(value) => update("showNotifications", value)} />
            <Switch label="Play Sound on Events" checked={settings.playSound} onChange={(value) => update("playSound", value)} />
            <SettingSelect label="Notification Level" value={notificationLevel} onChange={updateNotificationLevel} options={["Errors only", "Errors and completed actions", "All events"]} />
          </Panel>}
          {activeTab === "Integrations" && <Panel title="Integrations">
            <SettingSelect label="Preferred Browser" value={settings.preferredBrowser} onChange={(value) => update("preferredBrowser", value)} options={["Default System Browser", "Chrome", "Edge", "Firefox"]} />
            <Button icon={<Folder size={16} />} onClick={() => void run(() => api.openPath(state.appDataDir))}>Open App Data</Button>
          </Panel>}
          {activeTab === "Updates" && <Panel title="Updates">
            <Switch label="Check for Updates on Startup" checked={settings.checkUpdatesOnStartup} onChange={(value) => update("checkUpdatesOnStartup", value)} />
            <Button icon={<RefreshCw size={16} />} onClick={() => void checkRelease()}>Check Now</Button>
            {release && <div className="release-card"><strong>{release.updateAvailable ? t("Update available") : t("You are up to date")}</strong><span>{release.currentVersion} → {release.latestVersion}</span><div className="toolbar"><Button onClick={() => void run(() => api.openUrl(release.url))}>Open Release</Button><Button onClick={() => void downloadUpdate()}>Download Update</Button>{downloadedInstaller && <Button variant="primary" onClick={() => void run(() => api.installDownloadedUpdate(downloadedInstaller), { label: "Starting update installer..." })}>Install Update</Button>}</div></div>}
          </Panel>}
          {activeTab === "Backups" && <Panel title="Backups">
            <SettingInput label="Backups Folder" value={settings.backupsFolder} onChange={(value) => update("backupsFolder", value)} />
            <SettingNumber label="Backup Retention Days" value={settings.backupRetentionDays} onChange={(value) => update("backupRetentionDays", value)} />
            <Switch label="Scheduled Backups" checked={scheduledBackups} onChange={setScheduledBackups} />
            <div className="toolbar">
              <Button icon={<Save size={16} />} onClick={() => void createBackup()}>Create Backup</Button>
              <Button icon={<Database size={16} />} onClick={() => void backupAllDatabases()}>Backup Databases</Button>
              <Button icon={<Upload size={16} />} onClick={() => void restoreBackup()}>Restore Backup</Button>
              <Button icon={<Folder size={16} />} onClick={() => void run(() => api.openPath(settings.backupsFolder))}>Open Backups</Button>
            </div>
            <p className="muted">{scheduledBackups ? t("Scheduled backups are enabled for the local reminder panel.") : t("Scheduled backups are paused.")}</p>
          </Panel>}
          {activeTab === "Backups" && <Panel title="Environment Snapshots" action={<><input className="toolbar-input" value={snapshotName} onChange={(event) => setSnapshotName(event.target.value)} /><Button icon={<RefreshCw size={16} />} onClick={() => void loadSnapshots()}>Refresh</Button><Button variant="primary" icon={<Save size={16} />} disabled={!snapshotName.trim()} onClick={() => void createSnapshot()}>Create Snapshot</Button></>}>
            <table className="data-table compact-table">
              <thead><tr><th>Name</th><th>Created</th><th>Hosts</th><th>Services</th><th>Databases</th><th>Action</th></tr></thead>
              <tbody>{snapshots.map((item) => <tr key={item.id}><td><strong>{item.name}</strong></td><td>{new Date(item.createdAt).toLocaleString()}</td><td>{item.hosts}</td><td>{item.services}</td><td>{item.databases}</td><td><Button onClick={() => void run(() => api.restoreEnvironmentSnapshot(item.id), { label: `Restoring ${item.name}...` })}>Restore</Button></td></tr>)}</tbody>
            </table>
            {snapshots.length === 0 && <p className="muted">No environment snapshots yet.</p>}
          </Panel>}
          {activeTab === "Advanced" && <><Panel title="Logging">
              <SettingSelect label="Log Level" value={settings.logLevel} onChange={(value) => update("logLevel", value)} options={["Information", "Warning", "Error", "Debug"]} />
              <SettingSelect label="Max Log File Size" value={settings.maxLogFileSize} onChange={(value) => update("maxLogFileSize", value)} options={["10 MB", "50 MB", "100 MB"]} />
              <SettingNumber label="Retain Logs Days" value={settings.retainLogsDays} onChange={(value) => update("retainLogsDays", value)} />
              <Switch label="Show Timestamps" checked={settings.showTimestamps} onChange={(value) => update("showTimestamps", value)} />
              <Button icon={<RefreshCw size={16} />} onClick={() => void run(api.clearLogs)}>Reset All Warnings</Button>
              <Button icon={<Download size={16} />} onClick={() => void createDiagnosticBundle()}>Export Diagnostic Bundle</Button>
            </Panel></>}
          <Panel title="Import / Export Settings">
            <Button icon={<Upload size={16} />} onClick={() => void importSettings()}>Import Settings</Button>
            <Button icon={<Upload size={16} />} onClick={() => void exportSettings()}>Export Settings</Button>
            <Button icon={<Save size={16} />} onClick={() => void run(() => api.saveSettings(settings))}>Save Settings</Button>
          </Panel>
          <Panel title="Reset Settings">
            <Button variant="danger" icon={<RefreshCw size={16} />} onClick={() => void run(api.resetSettings)}>Reset to Defaults</Button>
          </Panel>
        </section>
        <aside className="detail-rail">
          <Panel title="About Settings">
            {tabs.map((tab) => <div className="about-row" key={tab}>{iconFor(tab)}<div><strong>{t(tab)}</strong><p>{t(descriptionFor(tab))}</p></div></div>)}
          </Panel>
          <Panel title="Need Help?">
            <p className="muted">{t("Visit documentation for detailed guides and troubleshooting.")}</p>
            <div className="toolbar">
              <Button onClick={() => void run(api.openDocumentation)}>Open Documentation</Button>
              <Button icon={<RefreshCw size={16} />} onClick={() => void runHealthCheck()}>Health Check</Button>
              <Button icon={<Wrench size={16} />} onClick={() => void repairAll()}>One-click Fix</Button>
            </div>
            {healthError && <div className="error-inline">{healthError}</div>}
            {health && <HealthCheckView report={health} />}
          </Panel>
          <Panel title="Security Center">
            <SecurityCenter state={state} />
          </Panel>
          <Panel title="Performance">
            <PerformanceCenter state={state} />
          </Panel>
          <Panel title="Resource Monitor" action={<Button icon={<RefreshCw size={16} />} onClick={() => void loadProcesses()}>Refresh</Button>}>
            <table className="data-table compact-table">
              <thead><tr><th>Process</th><th>PID</th><th>CPU</th><th>RAM</th><th>Action</th></tr></thead>
              <tbody>{processes.slice(0, 8).map((item) => <tr key={item.pid}><td><strong>{item.name}</strong><small>{item.command}</small></td><td>{item.pid}</td><td>{item.cpu.toFixed(1)}</td><td>{item.memoryMb} MB</td><td><Button variant="danger" onClick={() => void run(() => api.killProcess(item.pid), { label: `Stopping process ${item.pid}...` }).then(() => loadProcesses())}>Stop</Button></td></tr>)}</tbody>
            </table>
            {processes.length === 0 && <p className="muted">Click Refresh to inspect running processes.</p>}
          </Panel>
        </aside>
      </div>
    </div>
  );
}

function PerformanceCenter({ state }: { state: AppSnapshot }) {
  const slowServices = state.services
    .filter((service) => service.status === "running")
    .sort((a, b) => (b.cpu + b.memoryMb / 128) - (a.cpu + a.memoryMb / 128))
    .slice(0, 5);
  const healthScore = Math.max(0, Math.round(100 - state.services.filter((service) => service.status === "error").length * 15 - state.hosts.filter((host) => host.status === "error").length * 12));
  return (
    <div className="security-list">
      <div><strong>{healthScore}%</strong><span>Health Score</span><small>{state.services.filter((service) => service.status === "running").length} running services</small></div>
      <div><strong>{state.system.cpu}%</strong><span>CPU</span><small>System load</small></div>
      <div><strong>{state.system.memoryGb.toFixed(1)} GB</strong><span>Memory</span><small>Application snapshot</small></div>
      <div><strong>{slowServices[0]?.name ?? "None"}</strong><span>Top Service</span><small>{slowServices[0] ? `${slowServices[0].cpu.toFixed(1)}% CPU, ${slowServices[0].memoryMb} MB` : "No running services"}</small></div>
    </div>
  );
}

function SecurityCenter({ state }: { state: AppSnapshot }) {
  const exposed = state.services.filter((service) => service.status === "running").flatMap((service) => service.ports.map((port) => `${service.name}:${port}`));
  const weakDatabases = state.databases.filter((database) => !database.password || ["localstack", "password"].includes(database.password.toLowerCase()));
  const untrustedCerts = state.certificates.filter((certificate) => !certificate.trusted);
  const defaultPhp = state.phpVersions.find((php) => php.default);
  const displayErrors = defaultPhp?.ini.display_errors?.toLowerCase() === "on";
  return (
    <div className="security-list">
      <div><strong>{exposed.length}</strong><span>Open service ports</span><small>{exposed.slice(0, 3).join(", ") || "None"}</small></div>
      <div><strong>{weakDatabases.length}</strong><span>Weak database passwords</span><small>{weakDatabases.map((item) => item.name).slice(0, 3).join(", ") || "None"}</small></div>
      <div><strong>{untrustedCerts.length}</strong><span>Untrusted certificates</span><small>{untrustedCerts.map((item) => item.domain).slice(0, 3).join(", ") || "None"}</small></div>
      <div><strong>{displayErrors ? "On" : "Off"}</strong><span>PHP display_errors</span><small>{defaultPhp?.version ?? "No default PHP"}</small></div>
    </div>
  );
}

function HealthCheckView({ report }: { report: HealthReport }) {
  const t = useT();
  const critical = report.checks.filter((check) => check.severity === "error");
  const warnings = report.checks.filter((check) => check.severity === "warning");
  const visible = [...critical, ...warnings].slice(0, 10);
  return (
    <div className="health-result">
      <div className="kv detail-kv">
        <span>{t("Summary")}</span><strong>{report.summary}</strong>
        <span>{t("OK")}</span><strong>{report.ok}</strong>
        <span>{t("Warnings")}</span><strong>{report.warnings}</strong>
        <span>{t("Errors")}</span><strong>{report.errors}</strong>
      </div>
      <div className="health-mini-grid">
        <span><strong>{critical.length}</strong><small>{t("Critical")}</small></span>
        <span><strong>{warnings.length}</strong><small>{t("Warnings")}</small></span>
        <span><strong>{report.checks.length}</strong><small>{t("Total checks")}</small></span>
      </div>
      {visible.length === 0 ? (
        <p className="green-text">{t("All checks passed.")}</p>
      ) : visible.map((check) => (
        <div className={check.severity === "error" ? "error-inline" : "warning-inline"} key={check.id}>
          <strong>{check.title}</strong>
          <span>{check.message}</span>
          {check.action && <small>{check.action}</small>}
        </div>
      ))}
    </div>
  );
}

function Switch({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  const t = useT();
  return <label className="setting-line"><span>{t(label)}</span><span className={`toggle ${checked ? "on" : ""}`} onClick={() => onChange(!checked)} /></label>;
}

function SettingSelect({ label, value, onChange, options }: { label: string; value: string; onChange: (value: string) => void; options: string[] }) {
  const t = useT();
  return <label className="setting-line"><span>{t(label)}</span><select value={value} onChange={(event) => onChange(event.target.value)}>{options.map((option) => <option key={option} value={option}>{t(option)}</option>)}</select></label>;
}

function SettingInput({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  const t = useT();
  return <label className="setting-line"><span>{t(label)}</span><input value={value} onChange={(event) => onChange(event.target.value)} /></label>;
}

function SettingNumber({ label, value, onChange }: { label: string; value: number; onChange: (value: number) => void }) {
  const t = useT();
  return <label className="setting-line"><span>{t(label)}</span><input type="number" value={value} onChange={(event) => onChange(Number(event.target.value))} /></label>;
}

function iconFor(tab: string) {
  if (tab === "Paths") return <Folder size={17} />;
  if (tab === "Network") return <Globe2 size={17} />;
  if (tab === "Notifications") return <Bell size={17} />;
  if (tab === "Integrations") return <Link size={17} />;
  if (tab === "Backups") return <Database size={17} />;
  return <SettingsIcon size={17} />;
}

function descriptionFor(tab: string) {
  const map: Record<string, string> = {
    General: "Configure the core behavior and preferences of the application.",
    Paths: "Manage default folders for projects, logs, and services.",
    Startup: "Configure what happens when LocalStack Pro starts.",
    Network: "Manage ports, proxies, and network resolution.",
    Theme: "Customize the appearance of the application.",
    Notifications: "Control how and when you receive alerts.",
    Integrations: "Connect LocalStack Pro with external tools.",
    Updates: "Set your update channel and update behavior.",
    Backups: "Configure automatic backups and retention.",
    Advanced: "Advanced settings for power users."
  };
  return map[tab];
}
