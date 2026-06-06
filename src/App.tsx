import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Shell } from "./components/Shell";
import { TrayPanel } from "./components/TrayPanel";
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

export default function App() {
  const { state, loading, error, notice, busy, actionLabel, run, refresh } = useAppState();
  const [page, setPageState] = useState<PageKey>(() => pageFromHash());
  const [selectedHost, setSelectedHost] = useState<HostInfo | undefined>();
  const [editingHost, setEditingHost] = useState<HostInfo | undefined>();
  const [selectedService, setSelectedService] = useState<ServiceInfo | undefined>();

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

const pageKeys: PageKey[] = ["overview", "hosts", "host-editor", "services", "php", "database", "cms", "ssl", "logs", "settings"];

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
      {error && <div className="error-banner">{t(error)}</div>}
      {page === "overview" && <OverviewPage state={state} run={run} selectedHost={selectedHost} selectHost={setSelectedHost} editHost={editHost} />}
      {page === "hosts" && <HostsPage state={state} run={run} selected={selectedHost} setSelected={setSelectedHost} editHost={editHost} />}
      {page === "host-editor" && <HostEditorPage state={state} initial={editingHost} run={run} back={() => setPage("hosts")} />}
      {page === "services" && <ServicesPage state={state} run={run} selected={selectedService} setSelected={setSelectedService} />}
      {page === "php" && <PhpPage state={state} run={run} />}
      {page === "database" && <DatabasePage state={state} run={run} />}
      {page === "cms" && <CmsPage state={state} run={run} />}
      {page === "ssl" && <SslPage state={state} run={run} />}
      {page === "logs" && <LogsPage state={state} run={run} />}
      {page === "settings" && <SettingsPage state={state} run={run} />}
    </Shell>
  );
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
