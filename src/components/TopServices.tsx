import { Database, Feather, Globe2, Mail, Play, RefreshCw, Square, Box } from "lucide-react";
import { Button } from "./Button";
import { Panel } from "./Panel";
import type { AppSnapshot } from "../ui/types";
import { useT } from "../ui/i18n";

const icons: Record<string, React.ReactNode> = {
  apache: <Feather size={32} />,
  nginx: <Box size={32} />,
  mysql: <Database size={32} />,
  mariadb: <Database size={32} />,
  postgresql: <Database size={32} />,
  redis: <Database size={32} />,
  mailpit: <Mail size={32} />
};

export function TopServices({
  state,
  onStartAll,
  onStopAll,
  onRestartAll,
  onOpenSite,
  onToggleService
}: {
  state: AppSnapshot;
  onStartAll: () => void;
  onStopAll: () => void;
  onRestartAll: () => void;
  onOpenSite: () => void;
  onToggleService?: (serviceId: string, running: boolean) => void;
}) {
  const t = useT();
  const order = ["apache", "nginx", "mysql", "redis", "mailpit"];
  const visible = order
    .map((id) => state.services.find((service) => service.id === id))
    .filter((service): service is NonNullable<typeof service> => Boolean(service));
  return (
    <div className="top-row">
      <Panel title="Services" className="services-strip">
        <div className="strip-list">
          {visible.map((service) => (
            <div className="strip-service" key={service.id}>
              <div className={`service-icon icon-${service.id}`}>{icons[service.id] ?? <Box size={32} />}</div>
              <div>
                <strong>{service.name}</strong>
                <small>{service.version}</small>
              </div>
              <span className={`toggle ${service.status === "running" ? "on" : ""}`} onClick={() => onToggleService?.(service.id, service.status === "running")} />
            </div>
          ))}
        </div>
      </Panel>
      <div className="command-stack">
        <Button variant="primary" onClick={onStartAll} icon={<Play size={20} />}>
          {t("Start All")}
        </Button>
        <Button onClick={onStopAll} icon={<Square size={16} />}>
          {t("Stop")}
        </Button>
        <Button onClick={onRestartAll} icon={<RefreshCw size={17} />}>
          {t("Restart")}
        </Button>
        <Button onClick={onOpenSite} icon={<Globe2 size={18} />}>
          {t("Open Site")}
        </Button>
      </div>
      <Panel title="System Info" className="system-info">
        <div className="kv">
          <span>LocalStack Pro</span>
          <strong>{state.system.appVersion}</strong>
          <span>{state.system.os}</span>
          <span />
          <span>{t("Uptime")}</span>
          <strong>{formatUptime(state.system.uptimeSeconds)} <i className="status-dot green" /></strong>
        </div>
      </Panel>
    </div>
  );
}

export function formatUptime(seconds: number) {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${minutes}m`;
}
