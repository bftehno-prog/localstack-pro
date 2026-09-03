import { Download, MoreVertical, RefreshCw, Shield, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../ui/api";
import { pickFolder } from "../ui/dialogs";
import type { AppRun, AppSnapshot, CertificateInfo, SslDiagnostic } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { TopServices } from "../components/TopServices";
import { useT } from "../ui/i18n";

export function SslPage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const t = useT();
  const [selectedId, setSelectedId] = useState<string | null>(state.certificates[0]?.id ?? null);
  const [selectedHostDomain, setSelectedHostDomain] = useState(() => state.hosts.find((host) => host.ssl)?.domain ?? state.hosts[0]?.domain ?? "");
  const [diagnostic, setDiagnostic] = useState<SslDiagnostic>();
  const cert = state.certificates.find((item) => item.id === selectedId) ?? state.certificates[0] ?? null;
  useEffect(() => {
    if (!cert && state.certificates[0]) setSelectedId(state.certificates[0].id);
  }, [cert, state.certificates]);
  useEffect(() => {
    if (!state.hosts.some((host) => host.domain === selectedHostDomain)) {
      setSelectedHostDomain(state.hosts.find((host) => host.ssl)?.domain ?? state.hosts[0]?.domain ?? "");
    }
  }, [selectedHostDomain, state.hosts]);
  const exportCert = async (certificate: CertificateInfo) => {
    const folder = await pickFolder();
    if (folder) {
      await run(() => api.exportCertificate(certificate.id, folder), { label: `Exporting certificate for ${certificate.domain}...` });
    }
  };
  const diagnose = async (certificate: CertificateInfo) => {
    const result = await run(() => api.diagnoseSsl(certificate.domain), { label: `Diagnosing SSL for ${certificate.domain}...` });
    if (result && typeof result === "object" && "summary" in result) setDiagnostic(result as SslDiagnostic);
  };
  const autoRepair = async (domain: string) => {
    const host = state.hosts.find((item) => item.domain === domain);
    await run(() => api.generateCertificate(domain, [domain, `www.${domain}`]), { label: `Generating certificate for ${domain}...` });
    await run(() => api.trustCertificate(domain), { label: `Trusting certificate for ${domain}...` });
    if (host && !host.ssl) {
      await run(() => api.saveHost({ ...host, ssl: true }), { label: `Enabling SSL for ${domain}...` });
    }
    if (host) await run(() => api.repairHost(host.id), { label: `Repairing SSL host ${domain}...` });
    setSelectedId(domain);
  };
  return (
    <>
      <TopServices state={state} onStartAll={() => void run(api.startAll, { label: "Starting all services..." })} onStopAll={() => void run(api.stopAll, { label: "Stopping all services..." })} onRestartAll={() => void run(api.restartAll, { label: "Restarting all services..." })} onOpenSite={() => state.hosts[0] && void run(() => api.openHost(state.hosts[0].id), { label: `Opening ${state.hosts[0].domain}...` })} onToggleService={(serviceId, running) => void run(() => running ? api.stopService(serviceId) : api.startService(serviceId), { label: `${running ? "Stopping" : "Starting"} ${serviceId}...` })} />
      <div className="page-grid">
        <section>
          <Panel title={t("SSL Certificates")} action={<><select aria-label={t("Host for SSL certificate")} value={selectedHostDomain} onChange={(event) => setSelectedHostDomain(event.target.value)} disabled={state.hosts.length === 0}>{state.hosts.map((host) => <option key={host.id} value={host.domain}>{host.domain}</option>)}</select><Button variant="primary" icon={<Shield size={16} />} disabled={!selectedHostDomain} onClick={() => void autoRepair(selectedHostDomain)}>{t("Generate Certificate")}</Button><Button icon={<Upload size={16} />} disabled={!cert} onClick={() => cert && void run(() => api.trustCertificate(cert.id), { label: `Trusting certificate for ${cert.domain}...` })}>{t("Trust Certificate")}</Button><Button icon={<RefreshCw size={16} />} disabled={!cert} onClick={() => cert && void autoRepair(cert.domain)}>{t("SSL Auto Repair")}</Button><Button icon={<RefreshCw size={16} />} disabled={!cert} onClick={() => cert && void diagnose(cert)}>{t("Diagnose SSL")}</Button><Button variant="danger" disabled={!cert} onClick={() => cert && void run(() => api.revokeCertificate(cert.id), { label: `Revoking certificate for ${cert.domain}...` })}>{t("Revoke")}</Button><Button icon={<Download size={16} />} disabled={!cert} onClick={() => cert && void exportCert(cert)}>{t("Export")}</Button></>}>
            <table className="data-table">
              <thead><tr><th>{t("Domain")}</th><th>{t("Status")}</th><th>{t("Trust")}</th><th>{t("Expires")}</th><th>{t("Issuer")}</th><th>{t("SAN Domains")}</th><th>{t("Auto-Renew")}</th><th>{t("Actions")}</th></tr></thead>
              <tbody>
                {state.certificates.length === 0 && <tr><td colSpan={8}>{t("No certificates yet. Generate a local certificate to continue.")}</td></tr>}
                {state.certificates.map((row) => <tr key={row.id} className={cert?.id === row.id ? "selected" : ""} onClick={() => setSelectedId(row.id)}><td><strong>{row.domain}</strong></td><td><span className={row.status === "Valid" ? "green-text" : "orange-text"}>{t(row.status)}</span></td><td>{t(row.trusted ? "Trusted" : "Untrusted")}</td><td>{new Date(row.expiresAt).toLocaleDateString()}</td><td>{row.issuer}</td><td>{row.sanDomains.join(", ")}</td><td><span className={`toggle ${row.autoRenew ? "on" : ""}`} onClick={(event) => { event.stopPropagation(); void run(() => api.saveCertificate({ ...row, autoRenew: !row.autoRenew }), { label: `Saving certificate for ${row.domain}...` }); }} /></td><td><Button variant="icon" icon={<MoreVertical size={15} />} onClick={(event) => { event.stopPropagation(); void exportCert(row); }} /></td></tr>)}
              </tbody>
            </table>
          </Panel>
          {diagnostic && <Panel title={t("SSL Diagnostics")}>
            <div className="health-mini-grid">
              <span><strong>{diagnostic.caTrusted ? "OK" : t("Fix")}</strong><small>{t("CA trusted")}</small></span>
              <span><strong>{diagnostic.certExists ? "OK" : t("Missing")}</strong><small>{t("Certificate")}</small></span>
              <span><strong>{diagnostic.keyExists ? "OK" : t("Missing")}</strong><small>{t("Private key")}</small></span>
              <span><strong>{diagnostic.sanCorrect ? "OK" : t("Fix")}</strong><small>{t("SAN domain")}</small></span>
              <span><strong>{diagnostic.vhostConfigured ? "OK" : t("Fix")}</strong><small>{t("Virtual host")}</small></span>
            </div>
            <p className="muted">{diagnostic.summary}</p>
          </Panel>}
          <Panel title={t("Browser Trust Status")}>
            <div className="trust-status"><Shield size={52} /><div><h2>{t("Your LocalStack CA is trusted by this system.")}</h2><p>{t("Web browsers on this machine will trust certificates issued by LocalStack CA for the domains listed above.")}</p></div><Button onClick={() => void run(api.openCertificateStore, { label: "Opening Windows Certificate Store..." })}>{t("Open Certificate Store")}</Button></div>
          </Panel>
        </section>
        {cert && <aside className="detail-rail">
          <Panel title={cert.domain} action={<span className="pill green">{cert.status}</span>}>
            <div className="kv detail-kv"><span>{t("Issued To")}</span><strong>{cert.domain}</strong><span>{t("Issued By")}</span><strong>{cert.issuer}</strong><span>{t("Fingerprint (SHA-256)")}</span><code>{cert.fingerprint}</code><span>{t("Valid Until")}</span><strong>{new Date(cert.expiresAt).toLocaleString()}</strong></div>
          </Panel>
          <Panel title={t("Certificate Files")}>
            <div className="kv form-kv"><span>{t("Certificate (CRT)")}</span><button onClick={() => void run(() => api.openPath(cert.certPath), { label: `Opening certificate for ${cert.domain}...` })}>{cert.certPath}</button><span>{t("Private Key (KEY)")}</span><button onClick={() => void run(() => api.openPath(cert.keyPath), { label: `Opening key for ${cert.domain}...` })}>{cert.keyPath}</button><span>{t("Full Chain (PEM)")}</span><button onClick={() => void run(() => api.openPath(cert.pemPath), { label: `Opening PEM for ${cert.domain}...` })}>{cert.pemPath}</button></div>
          </Panel>
          <Panel title={t("Troubleshooting Tips")}>
            <p className="muted">{t("If the site shows a certificate warning, click Repair Trust.")}</p>
            <Button icon={<RefreshCw size={16} />} onClick={() => void autoRepair(cert.domain)}>{t("SSL Auto Repair")}</Button>
          </Panel>
        </aside>}
      </div>
    </>
  );
}
