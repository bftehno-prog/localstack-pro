import { Plus, RefreshCw, Save, Star, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../ui/api";
import type { AppRun, AppSnapshot, PhpVersion } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { TopServices } from "../components/TopServices";
import { useT } from "../ui/i18n";

export function PhpPage({
  state,
  run
}: {
  state: AppSnapshot;
  run: AppRun;
}) {
  const t = useT();
  const fallbackPhp: PhpVersion = {
    version: "8.1.23",
    label: "8.1",
    status: "active",
    default: true,
    cliPath: "C:\\tools\\php\\8.1.23\\php.exe",
    sapiMode: "FPM",
    extensions: [],
    ini: {
      memory_limit: "512M",
      upload_max_filesize: "64M",
      post_max_size: "64M",
      max_execution_time: "120"
    },
    compatibility: "Full"
  };
  const [selected, setSelected] = useState<PhpVersion>(state.phpVersions[0] ?? fallbackPhp);
  const php = state.phpVersions.find((item) => item.version === selected.version) ?? state.phpVersions[0] ?? fallbackPhp;
  const [ini, setIni] = useState(php.ini);
  useEffect(() => {
    setIni(php.ini);
  }, [php.ini, php.version]);
  const installVersion = () => {
    void run(() => api.installPhpVersion("8.4"), { label: "Installing PHP 8.4..." });
  };
  return (
    <>
      <TopServices state={state} onStartAll={() => void run(api.startAll)} onStopAll={() => void run(api.stopAll)} onRestartAll={() => void run(api.restartAll)} onOpenSite={() => state.hosts[0] && void run(() => api.openHost(state.hosts[0].id))} onToggleService={(serviceId, running) => void run(() => running ? api.stopService(serviceId) : api.startService(serviceId))} />
      <div className="page-grid">
        <section>
          <Panel title="PHP" action={<><Button variant="primary" icon={<Plus size={16} />} onClick={installVersion}>{t("Install Version")}</Button><Button icon={<RefreshCw size={16} />} onClick={() => void run(api.getState, { label: "Refreshing PHP versions..." })}>{t("Refresh")}</Button></>}>
            <h2>{t("PHP Versions")}</h2>
            <table className="data-table">
              <thead><tr><th>{t("Version")}</th><th>{t("Status")}</th><th>{t("Default")}</th><th>{t("CLI Path")}</th><th>{t("SAPI Mode")}</th><th>{t("Extensions")}</th><th>{t("Compatibility")}</th><th></th></tr></thead>
              <tbody>
                {state.phpVersions.map((row) => (
                  <tr key={row.version} className={php.version === row.version ? "selected" : ""} onClick={() => { setSelected(row); setIni(row.ini); }}>
                    <td><span className="php-badge">{row.label}</span> {row.version}</td>
                    <td><span className="pill green">{t(row.status === "active" ? "Active" : "Installed")}</span></td>
                    <td>{row.default ? <Star size={16} className="green-text" /> : "-"}</td>
                    <td className="link">{row.cliPath}</td>
                    <td><span className="pill blue">{row.sapiMode}</span></td>
                    <td><span className="pill green">{row.extensions.filter((ext) => ext.enabled).length} / {row.extensions.length}</span></td>
                    <td><span className={row.compatibility === "Full" ? "green-text" : "orange-text"}>{t(row.compatibility === "Full" ? "Full Compatibility" : row.compatibility === "Legacy" ? "Legacy Compatibility" : row.compatibility)}</span></td>
                    <td><Button variant="icon" icon={<Trash2 size={15} />} onClick={(event) => { event.stopPropagation(); void run(() => api.removePhpVersion(row.version)); }} /></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Panel>
          <Panel title={`Extensions (PHP ${php.version})`}>
            <div className="extension-grid">
              {php.extensions.map((extension) => (
                <div className="extension-card" key={extension.name}>
                  <div><strong>{extension.name}</strong><small>{extension.version}</small></div>
                  <span className={`toggle ${extension.enabled ? "on" : ""}`} onClick={() => {
                    const next = { ...php, extensions: php.extensions.map((item) => item.name === extension.name ? { ...item, enabled: !item.enabled } : item) };
                    void run(() => api.savePhpVersion(next));
                  }} />
                </div>
              ))}
            </div>
          </Panel>
          <Panel title="PHP Actions">
            <div className="toolbar">
              <Button onClick={() => void run(() => api.openPath(`${state.appDataDir}\\configs\\php\\php-${php.version}.ini`))}> Edit php.ini</Button>
              <Button onClick={() => void run(() => api.openPath(php.cliPath))}>Open CLI</Button>
              <Button icon={<Star size={16} />} onClick={() => void run(() => api.setDefaultPhp(php.version))}>{t("Switch Default")}</Button>
              <Button variant="danger" icon={<Trash2 size={16} />} onClick={() => void run(() => api.removePhpVersion(php.version))}>{t("Remove Version")}</Button>
            </div>
          </Panel>
        </section>
        <aside className="detail-rail">
          <Panel title={`PHP ${php.version} ${t("Settings")}`} action={<span className="pill green">{t("Active")}</span>}>
            <div className="settings-form">
              {Object.entries(ini).map(([key, value]) => (
                <label key={key}>{key}<input value={value} onChange={(event) => setIni((current) => ({ ...current, [key]: event.target.value }))} /></label>
              ))}
            </div>
            <Button variant="primary" icon={<Save size={16} />} onClick={() => void run(() => api.savePhpVersion({ ...php, ini }))}>{t("Save Changes")}</Button>
          </Panel>
        </aside>
      </div>
    </>
  );
}
