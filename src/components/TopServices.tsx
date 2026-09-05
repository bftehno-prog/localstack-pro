import { Database, Feather, Globe2, Mail, Play, RefreshCw, Square, Box } from "lucide-react";
import { Button } from "./Button";
import { Panel } from "./Panel";
import type { AppSnapshot } from "../ui/types";

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
  const order = ["apache", "nginx", "mysql", "redis", "mailpit"];
  const visible = order
    .map((id) => state.services.find((service) => service.id === id))
    .filter((service): service is NonNullable<typeof service> => Boolean(service));
  return (
    <div className="top-row minimal-top-row">
      <Panel
        title="Services"
        className="services-strip"
        action={
          <div className="toolbar">
            <Button variant="icon" aria-label="Start All" onClick={onStartAll} icon={<Play size={17} />} />
            <Button variant="icon" aria-label="Stop All" onClick={onStopAll} icon={<Square size={15} />} />
            <Button variant="icon" aria-label="Restart All" onClick={onRestartAll} icon={<RefreshCw size={16} />} />
            <Button variant="icon" aria-label="Open Site" onClick={onOpenSite} icon={<Globe2 size={17} />} />
          </div>
        }
      >
        <div className="strip-list">
          {visible.map((service) => (
            <div className="strip-service" key={service.id}>
              <div className={`service-icon icon-${service.id}`}>{icons[service.id] ?? <Box size={32} />}</div>
              <div>
                <strong>{service.name}</strong>
              </div>
              <span className={`toggle ${service.status === "running" ? "on" : ""}`} onClick={() => onToggleService?.(service.id, service.status === "running")} />
            </div>
          ))}
        </div>
      </Panel>
    </div>
  );
}
