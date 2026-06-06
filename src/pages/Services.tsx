import { Box, Database, Download, FileText, Folder, MoreVertical, Play, RefreshCw, Search, Settings, Square } from "lucide-react";
import { api } from "../ui/api";
import type { AppRun, AppSnapshot, ServiceInfo } from "../ui/types";
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
  const startProfile = async (name: string, serviceIds: string[]) => {
    for (const serviceId of serviceIds) {
      const service = state.services.find((item) => item.id === serviceId);
      if (service && service.status !== "running") {
        await run(() => api.startService(serviceId), { label: `Starting ${service.name} for ${name} profile...` });
      }
    }
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
            <Button icon={<Download size={15} />} onClick={() => void run(api.installAllMissingDependencies, { label: "Installing missing service dependencies...", successLabel: "Missing dependencies installed or detected." })}>Install Missing</Button>
            <Button variant="icon" icon={<MoreVertical size={17} />} onClick={() => void run(() => api.openPath(state.settings.servicesFolder), { label: "Opening services folder..." })} />
          </div>
        </div>
        <Panel title="Service Profiles">
          <div className="profile-grid">
            <Button icon={<Play size={15} />} onClick={() => void startProfile("WordPress", ["apache", "mysql", "mailpit"])}>WordPress</Button>
            <Button icon={<Play size={15} />} onClick={() => void startProfile("Laravel", ["apache", "mysql", "redis", "mailpit"])}>Laravel</Button>
            <Button icon={<Play size={15} />} onClick={() => void startProfile("Static", ["nginx", "node-proxy"])}>Static</Button>
            <Button icon={<Play size={15} />} onClick={() => void startProfile("Database", ["mysql", "postgresql", "redis"])}>Database</Button>
            <Button icon={<Play size={15} />} onClick={() => void startProfile("PHP-only", ["apache"])}>PHP-only</Button>
          </div>
        </Panel>
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
                  <Button variant="primary" icon={service.status === "running" ? <Square size={14} /> : <Play size={14} />} onClick={(event) => { event.stopPropagation(); void run(() => service.status === "running" ? api.stopService(service.id) : api.startService(service.id), { label: `${service.status === "running" ? "Stopping" : "Starting"} ${service.name}...` }); }}>
                    {service.status === "running" ? "Stop" : "Start"}
                  </Button>
                  <Button icon={<RefreshCw size={16} />} onClick={(event) => { event.stopPropagation(); void run(() => api.restartService(service.id), { label: `Restarting ${service.name}...` }); }}>Restart</Button>
                  <Button icon={<Settings size={16} />} onClick={(event) => { event.stopPropagation(); void run(() => api.openPath(service.configPath), { label: `Opening ${service.name} config...` }); }}>Config</Button>
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
              <Button icon={<Settings size={17} />} onClick={() => void run(() => api.openPath(current.configPath), { label: `Opening ${current.name} config...` })}>Edit Config</Button>
              <Button icon={<FileText size={17} />} onClick={() => void run(() => api.openPath(current.logPath), { label: `Opening ${current.name} logs...` })}>View Logs</Button>
              <Button icon={<Folder size={17} />} onClick={() => void run(() => api.openPath(current.executablePath), { label: `Opening ${current.name} folder...` })}>Open Root Folder</Button>
              <Button icon={<Download size={17} />} onClick={() => void run(() => api.installServiceDependency(current.id), { label: `Installing ${current.name}...`, successLabel: `${current.name} installed or detected.` })}>Install</Button>
              <Button icon={<RefreshCw size={17} />} onClick={() => void run(() => api.restartService(current.id), { label: `Restarting ${current.name}...` })}>Restart</Button>
            </div>
          </Panel>
        </aside>
      )}
    </div>
  );
}
