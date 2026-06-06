import {
  Box,
  Database,
  ExternalLink,
  Globe2,
  Layers,
  Play,
  Power,
  RefreshCw,
  Settings,
  Square,
  SquareArrowOutUpRight,
  Terminal
} from "lucide-react";
import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../ui/api";
import { useT } from "../ui/i18n";
import type { AppRun, AppSnapshot, HostInfo, ServiceInfo } from "../ui/types";

export function TrayPanel({
  state,
  run,
  refresh,
  busy,
  actionLabel
}: {
  state: AppSnapshot;
  run: AppRun;
  refresh: (silent?: boolean) => Promise<void>;
  busy?: boolean;
  actionLabel?: string | null;
}) {
  const t = useT();
  const shellRef = useRef<HTMLDivElement>(null);
  const hideTimer = useRef<number | undefined>(undefined);
  const lastRefresh = useRef(0);
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let unsubscribe: (() => void) | undefined;
    void listen("tray-panel-opened", () => {
      shellRef.current?.focus();
      const now = Date.now();
      if (now - lastRefresh.current > 15000) {
        lastRefresh.current = now;
        void refresh(true);
      }
    }).then((value) => {
      unsubscribe = value;
    });
    return () => unsubscribe?.();
  }, [refresh]);
  useEffect(() => () => {
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
  }, []);

  const runningServices = state.services.filter((service) => service.status === "running").length;
  const runningHosts = state.hosts.filter((host) => host.status === "running").length;
  const allRunning = runningServices > 0 && runningServices === state.services.length;
  const statusText = allRunning ? "All services running" : runningServices > 0 ? `${runningServices} services running` : "All services stopped";
  const activeHost = state.hosts.find((host) => host.status === "running") ?? state.hosts[0];
  const services = preferredServices(state.services);

  const openMain = (page?: string) => void api.openMainPage(page);
  const closeTray = () => {
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    if ("__TAURI_INTERNALS__" in window) void api.hideTrayPanel();
  };
  const keepTrayOpen = () => {
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
  };
  const scheduleTrayClose = () => {
    if (hideTimer.current) window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(closeTray, 700);
  };
  const openHost = (host?: HostInfo) => {
    if (host) {
      void run(() => api.openHost(host.id), { label: `Opening ${host.domain}...` }).finally(closeTray);
    }
  };
  const openDatabaseTool = () => {
    void run(() => api.openDatabaseAdmin("phpmyadmin"), { label: "Opening phpMyAdmin..." }).finally(closeTray);
  };

  return (
    <div
      ref={shellRef}
      className="tray-shell"
      onMouseEnter={keepTrayOpen}
      onMouseLeave={scheduleTrayClose}
      onKeyDown={(event) => event.key === "Escape" && closeTray()}
      tabIndex={-1}
    >
      <section className="tray-card">
        <header className="tray-head">
          <div className="tray-brand">
            <div className="tray-logo" aria-hidden="true">
              <span />
              <span />
              <span />
            </div>
            <div>
              <strong>LocalStack Pro</strong>
              <p>{t(statusText)} <span className={`tray-dot ${allRunning ? "green" : runningServices ? "orange" : "gray"}`} /></p>
            </div>
          </div>
          <button className="tray-icon-btn" aria-label={String(t("Settings") ?? "Settings")} onClick={() => openMain("settings")}>
            <Settings size={22} />
          </button>
        </header>

        <div className="tray-actions">
          <button className="tray-action primary" onClick={() => void run(api.startAll, { label: "Starting all services..." })}>
            <Play size={20} />
            <span>{t("Start All")}</span>
          </button>
          <button className="tray-action" onClick={() => void run(api.stopAll, { label: "Stopping all services..." })}>
            <Square size={18} />
            <span>{t("Stop All")}</span>
          </button>
          <button className="tray-action" onClick={() => void run(api.restartAll, { label: "Restarting all services..." })}>
            <RefreshCw size={20} />
            <span>{t("Restart")}</span>
          </button>
          <button className="tray-action" onClick={() => openMain()}>
            <Box size={20} />
            <span>{t("Open Main Window")}</span>
          </button>
        </div>

        {busy && (
          <div className="tray-progress" role="status">
            <span />
            <strong>{t(actionLabel ?? "Action in progress...")}</strong>
          </div>
        )}

        <TraySection title="Active Hosts" action={<button className="tray-chip" onClick={() => openMain("hosts")}><span className="tray-dot green" />{runningHosts} <ExternalLink size={14} /></button>}>
          <div className="tray-host-list">
            {state.hosts.slice(0, 2).map((host) => (
              <button key={host.id} className="tray-host-row" onClick={() => openHost(host)}>
                <Globe2 size={17} />
                <strong>{host.domain}</strong>
                <span className={`tray-status ${host.status}`}>
                  <span className={`tray-dot ${host.status === "running" ? "green" : host.status === "error" ? "red" : "gray"}`} />
                  {t(titleStatus(host.status))}
                </span>
                <SquareArrowOutUpRight size={15} />
              </button>
            ))}
          </div>
        </TraySection>

        <TraySection title="Services" action={<button className="tray-more" onClick={() => openMain("services")}>{t("More")} <ExternalLink size={14} /></button>}>
          <div className="tray-services">
            {services.map((service) => (
              <div className="tray-service" key={service.id}>
                <ServiceMark service={service} />
                <strong>{service.name}</strong>
                <span>{service.version}</span>
                <button
                  className={`tray-toggle ${service.status === "running" ? "on" : ""}`}
                  aria-label={service.status === "running" ? `Stop ${service.name}` : `Start ${service.name}`}
                  onClick={() => void run(
                    service.status === "running" ? () => api.stopService(service.id) : () => api.startService(service.id),
                    { label: `${service.status === "running" ? "Stopping" : "Starting"} ${service.name}...` }
                  )}
                />
              </div>
            ))}
          </div>
        </TraySection>

        <TraySection title="Quick Actions">
          <div className="tray-quick">
            <button onClick={() => openHost(activeHost)}><Globe2 size={17} />{activeHost?.domain ?? t("Site")}</button>
            <button onClick={openDatabaseTool}><Database size={17} />phpMyAdmin</button>
            <button onClick={() => openMain("settings")}><Settings size={17} />{t("Settings")}</button>
            <button onClick={() => void api.quit()}><Power size={17} />{t("Quit")}</button>
          </div>
        </TraySection>

      </section>
    </div>
  );
}

function TraySection({ title, action, children }: { title: string; action?: ReactNode; children: ReactNode }) {
  const t = useT();
  return (
    <section className="tray-section">
      <div className="tray-section-head">
        <h2>{t(title)}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}

function preferredServices(services: ServiceInfo[]) {
  const order = ["apache", "nginx", "mysql", "redis"];
  return order
    .map((id) => services.find((service) => service.id === id))
    .filter(Boolean) as ServiceInfo[];
}

function ServiceMark({ service }: { service: ServiceInfo }) {
  if (service.id === "nginx") return <div className="tray-service-mark nginx">N</div>;
  if (service.id === "mysql") return <Database className="tray-service-mark plain" size={22} />;
  if (service.id === "redis") return <Layers className="tray-service-mark redis" size={22} />;
  return <Terminal className="tray-service-mark plain" size={22} />;
}

function titleStatus(status: string) {
  return status.slice(0, 1).toUpperCase() + status.slice(1);
}
