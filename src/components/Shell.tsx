import {
  Box,
  CircleHelp,
  Database,
  FileText,
  Gauge,
  Home,
  Layers,
  Minus,
  PanelLeft,
  Settings,
  Shield,
  Square,
  X
} from "lucide-react";
import type { ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api } from "../ui/api";
import { Button } from "./Button";
import { Logo } from "./Logo";
import type { AppSnapshot, PageKey } from "../ui/types";
import { useT } from "../ui/i18n";

const nav: Array<{ key: PageKey; label: string; icon: ReactNode }> = [
  { key: "overview", label: "Overview", icon: <Home size={20} /> },
  { key: "hosts", label: "Hosts", icon: <PanelLeft size={20} /> },
  { key: "services", label: "Services", icon: <Settings size={20} /> },
  { key: "php", label: "PHP", icon: <Gauge size={20} /> },
  { key: "database", label: "Database", icon: <Database size={20} /> },
  { key: "cms", label: "CMS", icon: <Box size={20} /> },
  { key: "ssl", label: "SSL", icon: <Shield size={20} /> },
  { key: "logs", label: "Logs", icon: <FileText size={20} /> },
  { key: "settings", label: "Settings", icon: <Settings size={20} /> }
];

export function Shell({
  page,
  setPage,
  state,
  children
}: {
  page: PageKey;
  setPage: (page: PageKey) => void;
  state: AppSnapshot;
  children: ReactNode;
}) {
  const t = useT();
  const runningCount = state.services.filter((service) => service.status === "running").length;
  const allRunning = runningCount === state.services.length && state.services.length > 0;
  const theme = state.settings.theme.toLowerCase().replace(/[^a-z0-9]+/g, "-") || "light";
  const density = state.settings.uiDensity.toLowerCase().includes("compact") ? "density-compact" : "density-comfortable";

  return (
    <div className={`app-frame theme-${theme} ${density}`}>
      <header className="window-bar" data-tauri-drag-region>
        <div className="window-title" data-tauri-drag-region>
          <div className="tiny-mark">
            <Layers size={12} />
          </div>
          <span>LocalStack Pro</span>
        </div>
        <div className="window-controls">
          <button aria-label="Minimize" onClick={() => void invoke("minimize_window")}>
            <Minus size={16} />
          </button>
          <button aria-label="Maximize" onClick={() => void invoke("toggle_window_maximize")}>
            <Square size={13} />
          </button>
          <button aria-label="Close" onClick={() => void invoke("request_window_close")}>
            <X size={17} />
          </button>
        </div>
      </header>
      <div className="app-body">
        <aside className="sidebar">
          <Logo />
          <nav className="nav">
            {nav.map((item) => (
              <button key={item.key} className={page === item.key ? "active" : ""} onClick={() => setPage(item.key)}>
                {item.icon}
                <span>{t(item.label)}</span>
              </button>
            ))}
          </nav>
          <div className="system-card">
            <div className="system-title">
              <span className={`status-dot ${allRunning ? "green" : runningCount > 0 ? "orange" : "gray"}`} />
              {t("System Status")}
            </div>
            <strong>{allRunning ? t("All services running") : runningCount > 0 ? `${runningCount} ${t("services running")}` : t("All services stopped")}</strong>
            <div className="metrics">
              <span>
                <small>CPU</small>
                {state.system.cpu}%
              </span>
              <span>
                <small>{t("Memory")}</small>
                {state.system.memoryGb.toFixed(1)} GB
              </span>
              <span>
                <small>{t("Disk")}</small>
                {Math.round(state.system.diskGb)} GB
              </span>
            </div>
          </div>
          <div className="sidebar-footer">
            <Button variant="icon" aria-label="Theme" icon={<Settings size={16} />} onClick={() => setPage("settings")} />
            <Button variant="icon" aria-label="Help" icon={<CircleHelp size={16} />} onClick={() => void api.openDocumentation()} />
          </div>
        </aside>
        <main className="main">{children}</main>
      </div>
    </div>
  );
}
