import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Shell } from "./components/Shell";
import { TrayPanel } from "./components/TrayPanel";
import { Button } from "./components/Button";
import { useAppState } from "./stores/useAppState";
import type { HostInfo, PageKey, ServiceInfo } from "./ui/types";
import { I18nProvider, useT } from "./ui/i18n";
import { api } from "./ui/api";

const OverviewPage = lazy(() => import("./pages/Overview").then((module) => ({ default: module.OverviewPage })));
const HostsPage = lazy(() => import("./pages/Hosts").then((module) => ({ default: module.HostsPage })));
const HostEditorPage = lazy(() => import("./pages/HostEditor").then((module) => ({ default: module.HostEditorPage })));
const ServicesPage = lazy(() => import("./pages/Services").then((module) => ({ default: module.ServicesPage })));
const PhpPage = lazy(() => import("./pages/Php").then((module) => ({ default: module.PhpPage })));
const DatabasePage = lazy(() => import("./pages/Database").then((module) => ({ default: module.DatabasePage })));
const CmsPage = lazy(() => import("./pages/Cms").then((module) => ({ default: module.CmsPage })));
const SslPage = lazy(() => import("./pages/Ssl").then((module) => ({ default: module.SslPage })));
const LogsPage = lazy(() => import("./pages/Logs").then((module) => ({ default: module.LogsPage })));
const SettingsPage = lazy(() => import("./pages/Settings").then((module) => ({ default: module.SettingsPage })));
const FilesPage = lazy(() => import("./pages/Files").then((module) => ({ default: module.FilesPage })));

export default function App() {
  const { state, loading, error, notice, busy, actionLabel, run, refresh } = useAppState();
  const [page, setPageState] = useState<PageKey>(() => pageFromHash());
  const [selectedHost, setSelectedHost] = useState<HostInfo | undefined>();
  const [editingHost, setEditingHost] = useState<HostInfo | undefined>();
  const [selectedService, setSelectedService] = useState<ServiceInfo | undefined>();
  const setPage = useCallback((next: PageKey) => {
    setPageState(next);
    const hash = `#${next}`;
    if (window.location.hash !== hash) window.history.replaceState(null, "", hash);
  }, []);

  useEffect(() => {
    const handleHash = () => setPageState(pageFromHash());
    window.addEventListener("hashchange", handleHash);
    return () => {
      window.removeEventListener("hashchange", handleHash);
    };
  }, []);
  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let unsubscribe: (() => void) | undefined;
    void listen<string>("navigate", (event) => {
      const next = event.payload as PageKey;
      if (pageKeys.includes(next)) setPage(next);
    }).then((value) => {
      unsubscribe = value;
    });
    return () => unsubscribe?.();
  }, [setPage]);

  if (loading || !state) return <div className="boot-screen">LocalStack Pro</div>;
  if (window.location.hash.replace(/^#/, "") === "tray") {
    const theme = state.settings.theme.toLowerCase().replace(/[^a-z0-9]+/g, "-") || "light";
    return (
      <I18nProvider language={state.settings.language}>
        <div className={`tray-frame theme-${theme}`}>
          <TrayPanel state={state} run={run} refresh={refresh} busy={busy} actionLabel={actionLabel} />
        </div>
      </I18nProvider>
    );
  }

  const editHost = (host?: HostInfo) => {
    setEditingHost(host);
    setPage("host-editor");
  };

  return (
    <I18nProvider language={state.settings.language}>
      <AppContent
        page={page}
        setPage={setPage}
        state={state}
        error={error}
        notice={notice}
        busy={busy}
        actionLabel={actionLabel}
        run={run}
        selectedHost={selectedHost}
        setSelectedHost={setSelectedHost}
        editingHost={editingHost}
        selectedService={selectedService}
        setSelectedService={setSelectedService}
        editHost={editHost}
      />
    </I18nProvider>
  );
}

const pageKeys: PageKey[] = ["overview", "hosts", "host-editor", "services", "php", "database", "cms", "ssl", "logs", "files", "settings"];

function pageFromHash(): PageKey {
  const key = window.location.hash.replace(/^#/, "") as PageKey;
  return pageKeys.includes(key) ? key : "overview";
}

function AppContent({
  page,
  setPage,
  state,
  error,
  notice,
  busy,
  actionLabel,
  run,
  selectedHost,
  setSelectedHost,
  editingHost,
  selectedService,
  setSelectedService,
  editHost
}: {
  page: PageKey;
  setPage: (page: PageKey) => void;
  state: NonNullable<ReturnType<typeof useAppState>["state"]>;
  error: string | null;
  notice: string | null;
  busy: boolean;
  actionLabel: string | null;
  run: ReturnType<typeof useAppState>["run"];
  selectedHost?: HostInfo;
  setSelectedHost: (host?: HostInfo) => void;
  editingHost?: HostInfo;
  selectedService?: ServiceInfo;
  setSelectedService: (service: ServiceInfo) => void;
  editHost: (host?: HostInfo) => void;
}) {
  const t = useT();
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  useNativeTooltips(t, page);
  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey)) return;
      const key = event.key.toLowerCase();
      if (key === "k" || key === "p") {
        event.preventDefault();
        setPaletteOpen((value) => !value);
      } else if (/^[1-9]$/.test(key)) {
        const next = pageKeys.filter((item) => item !== "host-editor")[Number(key) - 1];
        if (next) {
          event.preventDefault();
          setPage(next);
        }
      } else if (event.shiftKey && key === "s") {
        event.preventDefault();
        void run(() => api.startAll(), { label: "Starting all services..." });
      } else if (event.shiftKey && key === "x") {
        event.preventDefault();
        void run(() => api.stopAll(), { label: "Stopping all services..." });
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [run, setPage]);
  const commands = useMemo(() => [
    ...pageKeys.filter((key) => key !== "host-editor").map((key) => ({
      label: key === "php" ? "PHP" : key.charAt(0).toUpperCase() + key.slice(1),
      hint: "Open page",
      action: () => setPage(key)
    })),
    { label: "Start All", hint: "Services", action: () => void run(() => api.startAll(), { label: "Starting all services..." }) },
    { label: "Stop All", hint: "Services", action: () => void run(() => api.stopAll(), { label: "Stopping all services..." }) },
    { label: "Restart All", hint: "Services", action: () => void run(() => api.restartAll(), { label: "Restarting all services..." }) },
    { label: "Open Documentation", hint: "Help", action: () => void run(() => api.openDocumentation(), { label: "Opening documentation..." }) },
    { label: "Health Check", hint: "Diagnostics", action: () => void run(() => api.runHealthCheck(), { label: "Running health check..." }) }
  ], [run, setPage]);
  const visibleCommands = useMemo(() => {
    const query = paletteQuery.trim().toLowerCase();
    return query ? commands.filter((command) => `${command.label} ${command.hint}`.toLowerCase().includes(query)).slice(0, 12) : commands.slice(0, 12);
  }, [commands, paletteQuery]);
  return (
    <Shell page={page} setPage={setPage} state={state}>
      {busy && (
        <div className="action-progress" role="status" aria-live="polite">
          <span />
          <strong>{t(actionLabel ?? "Action in progress...")}</strong>
        </div>
      )}
      {notice && <div className="success-banner">{t(notice)}</div>}
      {error && <SmartError message={error} run={run} />}
      {paletteOpen && (
        <CommandPalette
          query={paletteQuery}
          setQuery={setPaletteQuery}
          commands={visibleCommands}
          close={() => setPaletteOpen(false)}
        />
      )}
      <Suspense fallback={<div className="boot-screen">LocalStack Pro</div>}>
        {page === "overview" && <OverviewPage state={state} run={run} selectedHost={selectedHost} selectHost={setSelectedHost} editHost={editHost} openDatabases={() => setPage("database")} />}
        {page === "hosts" && <HostsPage state={state} run={run} selected={selectedHost} setSelected={setSelectedHost} editHost={editHost} />}
        {page === "host-editor" && <HostEditorPage state={state} initial={editingHost} run={run} back={() => setPage("hosts")} />}
        {page === "services" && <ServicesPage state={state} run={run} selected={selectedService} setSelected={setSelectedService} />}
        {page === "php" && <PhpPage state={state} run={run} />}
        {page === "database" && <DatabasePage state={state} run={run} />}
        {page === "cms" && <CmsPage state={state} run={run} />}
        {page === "ssl" && <SslPage state={state} run={run} />}
        {page === "logs" && <LogsPage state={state} run={run} />}
        {page === "files" && <FilesPage state={state} run={run} />}
        {page === "settings" && <SettingsPage state={state} run={run} />}
      </Suspense>
    </Shell>
  );
}

function CommandPalette({
  query,
  setQuery,
  commands,
  close
}: {
  query: string;
  setQuery: (value: string) => void;
  commands: Array<{ label: string; hint: string; action: () => void }>;
  close: () => void;
}) {
  const t = useT();
  return (
    <div className="command-backdrop" onMouseDown={(event) => {
      if (event.target === event.currentTarget) close();
    }}>
      <div className="command-palette">
        <input
          autoFocus
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape") close();
            if (event.key === "Enter" && commands[0]) {
              commands[0].action();
              close();
            }
          }}
          placeholder={String(t("Search commands..."))}
        />
        <div>
          {commands.map((command) => (
            <button key={`${command.hint}-${command.label}`} onClick={() => { command.action(); close(); }}>
              <strong>{t(command.label)}</strong>
              <span>{t(command.hint)}</span>
            </button>
          ))}
          {!commands.length && <small>{t("No matches found.")}</small>}
        </div>
      </div>
    </div>
  );
}

function SmartError({ message, run }: { message: string; run: ReturnType<typeof useAppState>["run"] }) {
  const t = useT();
  const help = errorHelp(message);
  return (
    <div className="smart-error">
      <div>
        <strong>{t(help.title)}</strong>
        <span>{t(message)}</span>
        <small>{t(help.hint)}</small>
      </div>
      {help.action === "sync-hosts" && <Button onClick={() => void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." })}>{t("Fix")}</Button>}
      {help.action === "repair" && <Button onClick={() => void run(() => api.repairEnvironment(), { label: "Repairing environment..." })}>{t("Fix")}</Button>}
      {help.action === "install" && <Button onClick={() => void run(() => api.installAllMissingDependencies(), { label: "Installing missing service dependencies..." })}>{t("Install Missing")}</Button>}
      {help.action === "ssl" && <Button onClick={() => void run(() => api.openMainPage("ssl"), { label: "Opening SSL..." })}>{t("Fix")}</Button>}
    </div>
  );
}

function errorHelp(message: string) {
  const text = message.toLowerCase();
  if (text.includes("hosts file") || text.includes("not mapped")) {
    return { title: "Hosts file issue", hint: "Sync the Windows hosts file and approve administrator access.", action: "sync-hosts" };
  }
  if (text.includes("cert") || text.includes("ssl") || text.includes("authority_invalid")) {
    return { title: "SSL trust issue", hint: "Open SSL and run SSL Auto Repair.", action: "ssl" };
  }
  if (text.includes("executable was not found") || text.includes("not found or installed")) {
    return { title: "Missing dependency", hint: "Install missing service files or detect installed tools.", action: "install" };
  }
  if (text.includes("503") || text.includes("gateway") || text.includes("cannot start") || text.includes("did not start") || text.includes("port")) {
    return { title: "Service startup issue", hint: "Run automatic repair to refresh configs, ports and service state.", action: "repair" };
  }
  return { title: "Action failed", hint: "Check the details, then retry the action or open Logs.", action: undefined };
}

function useNativeTooltips(t: (value: string) => string | number | null | undefined, page: PageKey) {
  useEffect(() => {
    const root = document.querySelector(".app-frame") ?? document.body;
    const apply = () => {
      root.querySelectorAll<HTMLElement>("button, a, input, select, textarea, [role='button']").forEach((element) => {
        if (element.title) return;
        const aria = element.getAttribute("aria-label")?.trim();
        const placeholder = element.getAttribute("placeholder")?.trim();
        const text = element.textContent?.replace(/\s+/g, " ").trim();
        const label = aria || placeholder || text;
        if (label) {
          element.title = String(t(label) ?? label);
          element.dataset.autoTooltip = "1";
        }
      });
    };
    let scheduled = false;
    let frame = 0;
    const scheduleApply = () => {
      if (scheduled) return;
      scheduled = true;
      frame = window.requestAnimationFrame(() => {
        scheduled = false;
        apply();
      });
    };
    scheduleApply();
    const observer = new MutationObserver(scheduleApply);
    observer.observe(root, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [page, t]);
}
