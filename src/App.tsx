import { Suspense, lazy, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Shell } from "./components/Shell";
import { TrayPanel } from "./components/TrayPanel";
import { Button } from "./components/Button";
import { useAppState } from "./stores/useAppState";
import type { HostInfo, PageKey, ServiceInfo } from "./ui/types";
import { OverviewPage } from "./pages/Overview";
import { HostsPage } from "./pages/Hosts";
import { HostEditorPage } from "./pages/HostEditor";
import { ServicesPage } from "./pages/Services";
import { PhpPage } from "./pages/Php";
import { DatabasePage } from "./pages/Database";
import { CmsPage } from "./pages/Cms";
import { SslPage } from "./pages/Ssl";
import { LogsPage } from "./pages/Logs";
import { SettingsPage } from "./pages/Settings";
import { I18nProvider, useT } from "./ui/i18n";
import { api } from "./ui/api";

const FilesPage = lazy(() => import("./pages/Files").then((module) => ({ default: module.FilesPage })));

export default function App() {
  const { state, loading, error, notice, busy, actionLabel, operations, run, refresh } = useAppState();
  const [page, setPageState] = useState<PageKey>(() => pageFromHash());
  const [selectedHost, setSelectedHost] = useState<HostInfo | undefined>();
  const [editingHost, setEditingHost] = useState<HostInfo | undefined>();
  const [selectedService, setSelectedService] = useState<ServiceInfo | undefined>();
  const [showFirstRun, setShowFirstRun] = useState(() => localStorage.getItem("localstack.firstRunDone") !== "true");

  useEffect(() => {
    const handleHash = () => setPageState(pageFromHash());
    window.addEventListener("hashchange", handleHash);
    return () => window.removeEventListener("hashchange", handleHash);
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
  }, []);
  const setPage = (next: PageKey) => {
    setPageState(next);
    const hash = `#${next}`;
    if (window.location.hash !== hash) window.history.replaceState(null, "", hash);
  };

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
        operations={operations}
        run={run}
        selectedHost={selectedHost}
        setSelectedHost={setSelectedHost}
        editingHost={editingHost}
        selectedService={selectedService}
        setSelectedService={setSelectedService}
        editHost={editHost}
        showFirstRun={showFirstRun}
        finishFirstRun={() => {
          localStorage.setItem("localstack.firstRunDone", "true");
          setShowFirstRun(false);
        }}
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
  operations,
  run,
  selectedHost,
  setSelectedHost,
  editingHost,
  selectedService,
  setSelectedService,
  editHost,
  showFirstRun,
  finishFirstRun
}: {
  page: PageKey;
  setPage: (page: PageKey) => void;
  state: NonNullable<ReturnType<typeof useAppState>["state"]>;
  error: string | null;
  notice: string | null;
  busy: boolean;
  actionLabel: string | null;
  operations: ReturnType<typeof useAppState>["operations"];
  run: ReturnType<typeof useAppState>["run"];
  selectedHost?: HostInfo;
  setSelectedHost: (host?: HostInfo) => void;
  editingHost?: HostInfo;
  selectedService?: ServiceInfo;
  setSelectedService: (service: ServiceInfo) => void;
  editHost: (host?: HostInfo) => void;
  showFirstRun: boolean;
  finishFirstRun: () => void;
}) {
  const t = useT();
  useNativeTooltips(t, page);
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
      {operations.length > 0 && <OperationCenter operations={operations} />}
      {showFirstRun && <FirstRunWizard setPage={setPage} run={run} finish={finishFirstRun} />}
      {page === "overview" && <OverviewPage state={state} run={run} selectedHost={selectedHost} selectHost={setSelectedHost} editHost={editHost} />}
      {page === "hosts" && <HostsPage state={state} run={run} selected={selectedHost} setSelected={setSelectedHost} editHost={editHost} />}
      {page === "host-editor" && <HostEditorPage state={state} initial={editingHost} run={run} back={() => setPage("hosts")} />}
      {page === "services" && <ServicesPage state={state} run={run} selected={selectedService} setSelected={setSelectedService} />}
      {page === "php" && <PhpPage state={state} run={run} />}
      {page === "database" && <DatabasePage state={state} run={run} />}
      {page === "cms" && <CmsPage state={state} run={run} />}
      {page === "ssl" && <SslPage state={state} run={run} />}
      {page === "logs" && <LogsPage state={state} run={run} />}
      {page === "files" && <Suspense fallback={<div className="boot-screen">File Manager</div>}><FilesPage state={state} run={run} /></Suspense>}
      {page === "settings" && <SettingsPage state={state} run={run} />}
    </Shell>
  );
}

function FirstRunWizard({ setPage, run, finish }: { setPage: (page: PageKey) => void; run: ReturnType<typeof useAppState>["run"]; finish: () => void }) {
  const t = useT();
  return (
    <div className="first-run">
      <div>
        <strong>{t("First Run Wizard")}</strong>
        <span>{t("Choose project folders, check ports, trust SSL, and start base services.")}</span>
      </div>
      <Button onClick={() => { setPage("settings"); }}>{t("Open Settings")}</Button>
      <Button onClick={() => void run(() => api.repairEnvironment(), { label: "Repairing environment..." })}>{t("Prepare Environment")}</Button>
      <Button variant="primary" onClick={finish}>{t("Done")}</Button>
    </div>
  );
}

function OperationCenter({ operations }: { operations: ReturnType<typeof useAppState>["operations"] }) {
  const t = useT();
  return (
    <div className="operation-center" title={String(t("Recent actions"))}>
      <div className="operation-title">
        <strong>{t("Action Center")}</strong>
        <span>{operations.filter((item) => item.status === "running").length} {t("active")}</span>
      </div>
      <div className="operation-list">
        {operations.slice(0, 3).map((item) => (
          <div className={`operation-item operation-${item.status}`} key={item.id} title={item.message ? String(t(item.message)) : String(t(item.label))}>
            <i />
            <strong>{t(item.label)}</strong>
            <small>{item.status === "running" ? t("Running") : item.durationMs ? `${Math.max(1, Math.round(item.durationMs / 1000))}s` : t(item.status)}</small>
          </div>
        ))}
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
    const apply = () => {
      document.querySelectorAll<HTMLElement>("button, a, input, select, textarea, [role='button']").forEach((element) => {
        if (element.title && element.dataset.autoTooltip !== "1") return;
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
    apply();
    const observer = new MutationObserver(apply);
    observer.observe(document.body, { childList: true, subtree: true, characterData: true });
    return () => observer.disconnect();
  }, [page, t]);
}
