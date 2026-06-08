import { Box, Clock, Database, Download, FileText, Folder, MoreVertical, Play, RefreshCw, Search, Settings, Square, Wrench } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../ui/api";
import type { AppRun, AppSnapshot, InstalledTool, ServiceInfo } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";

export function ServicesPage({
  state,
  run,
  selected,
  setSelected
}: {
  state: AppSnapshot;
  run: AppRun;
  selected?: ServiceInfo;
  setSelected: (service: ServiceInfo) => void;
}) {
  const current = selected
    ? state.services.find((service) => service.id === selected.id) ?? selected
    : state.services[0];
  const [configPath, setConfigPath] = useState("");
  const [configText, setConfigText] = useState("");
  const [tools, setTools] = useState<InstalledTool[]>([]);
  const [timeline, setTimeline] = useState<Array<{ id: string; time: string; service: string; action: string; status: string }>>([]);
  const dependencyRows = useMemo(() => state.services.map((service) => ({
    service,
    executable: service.executablePath ? "set" : "missing",
    ports: service.ports.length ? service.ports.join(", ") : "none",
    autostart: service.autostart ? "on" : "off",
    state: service.lastError ? "needs fix" : service.status
  })), [state.services]);
  const addTimeline = (service: ServiceInfo, action: string, status: string) => {
    setTimeline((items) => [{ id: crypto.randomUUID(), time: new Date().toLocaleTimeString(), service: service.name, action, status }, ...items].slice(0, 12));
  };
  const serviceAction = async (service: ServiceInfo, action: "start" | "stop" | "restart") => {
    addTimeline(service, action, "queued");
    try {
      if (action === "start" && service.lastError?.toLowerCase().includes("not found")) {
        await run(() => api.installServiceDependency(service.id), { label: `Installing ${service.name}...`, serial: true });
      }
      await run(() => action === "start" ? api.startService(service.id) : action === "stop" ? api.stopService(service.id) : api.restartService(service.id), { label: `${action === "start" ? "Starting" : action === "stop" ? "Stopping" : "Restarting"} ${service.name}...`, serial: true });
      addTimeline(service, action, "done");
    } catch {
      addTimeline(service, action, "error");
    }
  };
  const fixService = async (service: ServiceInfo) => {
    addTimeline(service, "fix", "queued");
    try {
      await run(() => api.installServiceDependency(service.id), { label: `Installing ${service.name}...`, serial: true });
      await run(api.detectDependencies, { label: "Detecting installed dependencies...", serial: true });
      await run(() => api.restartService(service.id), { label: `Restarting ${service.name}...`, serial: true });
      addTimeline(service, "fix", "done");
    } catch {
      addTimeline(service, "fix", "error");
    }
  };
  const startProfile = (name: string, serviceIds: string[]) => {
    const availableIds = serviceIds.filter((serviceId) => state.services.some((item) => item.id === serviceId));
    void run(() => api.startServiceProfile(availableIds), {
      label: `Starting ${name} profile...`,
      successLabel: `${name} profile started.`
    });
  };
  const loadConfig = async (service: ServiceInfo) => {
    const result = await run(() => api.readConfigFile(service.configPath), { label: `Reading ${service.name} config...` });
    if (result && typeof result === "object" && "content" in result) {
      setConfigPath(result.path);
      setConfigText(result.content);
    }
  };
  const inspectTools = async () => {
    const result = await run(api.inspectInstalledTools, { label: "Inspecting installed tools..." });
    if (Array.isArray(result)) setTools(result as InstalledTool[]);
  };
  return (
    <div className="page-grid">
      <section>
        <div className="page-title">
          <div><h1>Services</h1><p>Manage and monitor all system services</p></div>
          <div className="toolbar">
            <Button variant="primary" icon={<Play size={16} />} onClick={() => void run(api.startAll, { label: "Starting all services..." })}>Start All</Button>
            <Button icon={<Square size={15} />} onClick={() => void run(api.stopAll, { label: "Stopping all services..." })}>Stop All</Button>
            <Button icon={<RefreshCw size={15} />} onClick={() => void run(api.restartAll, { label: "Restarting all services..." })}>Restart All</Button>
            <Button icon={<Search size={15} />} onClick={() => void run(api.detectDependencies, { label: "Detecting installed dependencies..." })}>Detect</Button>
            <Button icon={<Search size={15} />} onClick={() => void inspectTools()}>Installed Tools</Button>
            <Button icon={<Download size={15} />} onClick={() => void run(api.installAllMissingDependencies, { label: "Installing missing service dependencies...", successLabel: "Missing dependencies installed or detected." })}>Install Missing</Button>
            <Button variant="icon" icon={<MoreVertical size={17} />} onClick={() => void run(() => api.openPath(state.settings.servicesFolder), { label: "Opening services folder..." })} />
          </div>
        </div>
        <Panel title="Service Profiles">
          <div className="profile-grid">
            <Button icon={<Play size={15} />} title="Start Apache, MySQL and Mailpit" onClick={() => startProfile("WordPress", ["apache", "mysql", "mailpit"])}>WordPress</Button>
            <Button icon={<Play size={15} />} title="Start Apache, MySQL, Redis and Mailpit" onClick={() => startProfile("Laravel", ["apache", "mysql", "redis", "mailpit"])}>Laravel</Button>
            <Button icon={<Play size={15} />} title="Start Nginx and Node.js Proxy" onClick={() => startProfile("Static", ["nginx", "node-proxy"])}>Static</Button>
            <Button icon={<Play size={15} />} title="Start MySQL, PostgreSQL and Redis" onClick={() => startProfile("Database", ["mysql", "postgresql", "redis"])}>Database</Button>
            <Button icon={<Play size={15} />} title="Start Apache for PHP hosts" onClick={() => startProfile("PHP-only", ["apache"])}>PHP-only</Button>
          </div>
        </Panel>
        <Panel title="Dependency Graph">
          <div className="dependency-graph">
            {dependencyRows.map(({ service, executable, ports, autostart, state }) => (
              <div key={service.id}>
                <strong>{service.name}</strong>
                <span>exe: {executable}</span>
                <span>ports: {ports}</span>
                <span>autostart: {autostart}</span>
                <span>{state}</span>
              </div>
            ))}
          </div>
        </Panel>
        {timeline.length > 0 && (
          <Panel title="Startup Timeline">
            <div className="timeline-list">
              {timeline.map((item) => (
                <div key={item.id}>
                  <Clock size={15} />
                  <strong>{item.time}</strong>
                  <span>{item.service}</span>
                  <small>{item.action} · {item.status}</small>
                </div>
              ))}
            </div>
          </Panel>
        )}
        {tools.length > 0 && (
          <Panel title="Installed Tools">
            <div className="tools-grid">
              {tools.map((tool) => (
                <div className="tool-card" key={tool.id}>
                  <strong>{tool.name}</strong>
                  <span className={tool.status === "installed" ? "green-text" : "orange-text"}>{tool.status}</span>
                  <small>{tool.version ?? "-"}</small>
                  <code>{tool.path ?? tool.command}</code>
                </div>
              ))}
            </div>
          </Panel>
        )}
        <div className="service-list">
          {state.services.map((service) => (
            <Panel key={service.id} className={current?.id === service.id ? "selected-panel" : ""}>
              <div className="service-row" onClick={() => setSelected(service)}>
                <div className={`service-icon icon-${service.id}`}>{service.id.includes("sql") || service.id === "redis" ? <Database size={44} /> : <Box size={44} />}</div>
                <div className="service-name"><strong>{service.name}</strong><small>{service.version}</small></div>
                <StatusDot status={service.status} />
                <label className="inline-toggle" onClick={(event) => {
                  event.stopPropagation();
                  void run(() => api.saveService({ ...service, autostart: !service.autostart }), { label: `Saving ${service.name} settings...` });
                }}><span className={`toggle ${service.autostart ? "on" : ""}`} />Autostart</label>
                <div className="service-metrics">
                  <span>Ports <strong>{service.ports.join(", ")}</strong></span>
                  <span>CPU <strong>{service.cpu.toFixed(1)}%</strong><i style={{ width: `${Math.max(service.cpu, 3)}%` }} /></span>
                  <span>RAM <strong>{service.memoryMb} MB</strong><i style={{ width: `${Math.min(service.memoryMb / 4, 100)}%` }} /></span>
                </div>
                <div className="service-actions">
                  <Button variant="primary" icon={service.status === "running" ? <Square size={14} /> : <Play size={14} />} onClick={(event) => { event.stopPropagation(); void serviceAction(service, service.status === "running" ? "stop" : "start"); }}>
                    {service.status === "running" ? "Stop" : "Start"}
                  </Button>
                  <Button icon={<RefreshCw size={16} />} onClick={(event) => { event.stopPropagation(); void serviceAction(service, "restart"); }}>Restart</Button>
                  <Button icon={<Wrench size={16} />} onClick={(event) => { event.stopPropagation(); void fixService(service); }}>Fix</Button>
                  <Button icon={<Settings size={16} />} onClick={(event) => { event.stopPropagation(); void loadConfig(service); }}>Config</Button>
                  <Button icon={<FileText size={16} />} onClick={(event) => { event.stopPropagation(); void run(() => api.openPath(service.logPath), { label: `Opening ${service.name} logs...` }); }}>Logs</Button>
                  <Button icon={<Download size={16} />} onClick={(event) => { event.stopPropagation(); void run(() => api.installServiceDependency(service.id), { label: `Installing ${service.name}...`, successLabel: `${service.name} installed or detected.` }); }}>Install</Button>
                </div>
              </div>
              {service.lastError && <div className="error-inline">{service.lastError}</div>}
            </Panel>
          ))}
        </div>
      </section>
      {current && (
        <aside className="detail-rail">
          <Panel title={current.name} action={<><StatusDot status={current.status} /><Button variant="icon" icon={<MoreVertical size={17} />} onClick={() => void run(() => api.saveService(current))} /></>}>
            <div className="kv detail-kv">
              <span>Version</span><strong>{current.version}</strong>
              <span>Executable</span><button onClick={() => void run(() => api.openPath(current.executablePath), { label: `Opening ${current.name} executable folder...` })}>{current.executablePath}<Folder size={16} /></button>
              <span>Config</span><button onClick={() => void run(() => api.openPath(current.configPath), { label: `Opening ${current.name} config...` })}>{current.configPath}<FileText size={16} /></button>
              <span>Log</span><button onClick={() => void run(() => api.openPath(current.logPath), { label: `Opening ${current.name} log...` })}>{current.logPath}<FileText size={16} /></button>
              <span>Ports</span><strong>{current.ports.join(", ")}</strong>
              <span>Process ID</span><strong>{current.pid ?? "-"}</strong>
              <span>Uptime</span><strong>{Math.floor(current.uptimeSeconds / 60)}m</strong>
            </div>
          </Panel>
          <Panel title="Quick Actions">
            <div className="quick-grid">
              <Button icon={<Settings size={17} />} onClick={() => void loadConfig(current)}>Edit Config</Button>
              <Button icon={<FileText size={17} />} onClick={() => void run(() => api.openPath(current.logPath), { label: `Opening ${current.name} logs...` })}>View Logs</Button>
              <Button icon={<Folder size={17} />} onClick={() => void run(() => api.openPath(current.executablePath), { label: `Opening ${current.name} folder...` })}>Open Root Folder</Button>
              <Button icon={<Download size={17} />} onClick={() => void run(() => api.installServiceDependency(current.id), { label: `Installing ${current.name}...`, successLabel: `${current.name} installed or detected.` })}>Install</Button>
              <Button icon={<Wrench size={17} />} onClick={() => void fixService(current)}>Fix this service</Button>
              <Button icon={<RefreshCw size={17} />} onClick={() => void serviceAction(current, "restart")}>Restart</Button>
            </div>
          </Panel>
          {configPath && <Panel title="Config Editor" action={<Button icon={<Settings size={16} />} onClick={() => void run(() => api.saveConfigFile(configPath, configText), { label: `Saving ${current.name} config...` })}>Save Config</Button>}>
            <textarea className="config-editor" value={configText} onChange={(event) => setConfigText(event.target.value)} />
            <p className="muted">{configPath}</p>
          </Panel>}
        </aside>
      )}
    </div>
  );
}
