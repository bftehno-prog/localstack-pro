import { Download, MoreVertical, RefreshCw, Shield, Upload } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../ui/api";
import { pickFolder } from "../ui/dialogs";
import type { AppRun, AppSnapshot, CertificateInfo } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { TopServices } from "../components/TopServices";

export function SslPage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const [selectedId, setSelectedId] = useState<string | null>(state.certificates[0]?.id ?? null);
  const cert = state.certificates.find((item) => item.id === selectedId) ?? state.certificates[0] ?? null;
  useEffect(() => {
    if (!cert && state.certificates[0]) setSelectedId(state.certificates[0].id);
  }, [cert, state.certificates]);
  const firstDomain = state.hosts.find((host) => host.ssl)?.domain ?? "shop.test";
  const exportCert = async (certificate: CertificateInfo) => {
    const folder = await pickFolder();
    if (folder) {
      await run(() => api.exportCertificate(certificate.id, folder), { label: `Exporting certificate for ${certificate.domain}...` });
    }
  };
  return (
    <>
      <TopServices state={state} onStartAll={() => void run(api.startAll, { label: "Starting all services..." })} onStopAll={() => void run(api.stopAll, { label: "Stopping all services..." })} onRestartAll={() => void run(api.restartAll, { label: "Restarting all services..." })} onOpenSite={() => state.hosts[0] && void run(() => api.openHost(state.hosts[0].id), { label: `Opening ${state.hosts[0].domain}...` })} onToggleService={(serviceId, running) => void run(() => running ? api.stopService(serviceId) : api.startService(serviceId), { label: `${running ? "Stopping" : "Starting"} ${serviceId}...` })} />
      <div className="page-grid">
        <section>
          <Panel title="SSL Certificates" action={<><Button variant="primary" icon={<Shield size={16} />} onClick={() => void run(() => api.generateCertificate(firstDomain, [firstDomain, `www.${firstDomain}`]), { label: `Generating certificate for ${firstDomain}...` })}>Generate Certificate</Button><Button icon={<Upload size={16} />} disabled={!cert} onClick={() => cert && void run(() => api.trustCertificate(cert.id), { label: `Trusting certificate for ${cert.domain}...` })}>Trust Certificate</Button><Button variant="danger" disabled={!cert} onClick={() => cert && void run(() => api.revokeCertificate(cert.id), { label: `Revoking certificate for ${cert.domain}...` })}>Revoke</Button><Button icon={<Download size={16} />} disabled={!cert} onClick={() => cert && void exportCert(cert)}>Export</Button></>}>
            <table className="data-table">
              <thead><tr><th>Domain</th><th>Status</th><th>Trust</th><th>Expires</th><th>Issuer</th><th>SAN Domains</th><th>Auto-Renew</th><th>Actions</th></tr></thead>
              <tbody>
                {state.certificates.length === 0 && <tr><td colSpan={8}>No certificates yet. Generate a local certificate to continue.</td></tr>}
                {state.certificates.map((row) => <tr key={row.id} className={cert?.id === row.id ? "selected" : ""} onClick={() => setSelectedId(row.id)}><td><strong>{row.domain}</strong></td><td><span className={row.status === "Valid" ? "green-text" : "orange-text"}>{row.status}</span></td><td>{row.trusted ? "Trusted" : "Untrusted"}</td><td>{new Date(row.expiresAt).toLocaleDateString()}</td><td>{row.issuer}</td><td>{row.sanDomains.join(", ")}</td><td><span className={`toggle ${row.autoRenew ? "on" : ""}`} onClick={(event) => { event.stopPropagation(); void run(() => api.saveCertificate({ ...row, autoRenew: !row.autoRenew }), { label: `Saving certificate for ${row.domain}...` }); }} /></td><td><Button variant="icon" icon={<MoreVertical size={15} />} onClick={(event) => { event.stopPropagation(); void exportCert(row); }} /></td></tr>)}
              </tbody>
            </table>
          </Panel>
          <Panel title="Browser Trust Status">
            <div className="trust-status"><Shield size={52} /><div><h2>Your LocalStack CA is trusted by this system.</h2><p>Web browsers on this machine will trust certificates issued by LocalStack CA for the domains listed above.</p></div><Button onClick={() => void run(api.openCertificateStore, { label: "Opening Windows Certificate Store..." })}>Open Certificate Store</Button></div>
          </Panel>
        </section>
        {cert && <aside className="detail-rail">
          <Panel title={cert.domain} action={<span className="pill green">{cert.status}</span>}>
            <div className="kv detail-kv"><span>Issued To</span><strong>{cert.domain}</strong><span>Issued By</span><strong>{cert.issuer}</strong><span>Fingerprint (SHA-256)</span><code>{cert.fingerprint}</code><span>Valid Until</span><strong>{new Date(cert.expiresAt).toLocaleString()}</strong></div>
          </Panel>
          <Panel title="Certificate Files">
            <div className="kv form-kv"><span>Certificate (CRT)</span><button onClick={() => void run(() => api.openPath(cert.certPath), { label: `Opening certificate for ${cert.domain}...` })}>{cert.certPath}</button><span>Private Key (KEY)</span><button onClick={() => void run(() => api.openPath(cert.keyPath), { label: `Opening key for ${cert.domain}...` })}>{cert.keyPath}</button><span>Full Chain (PEM)</span><button onClick={() => void run(() => api.openPath(cert.pemPath), { label: `Opening PEM for ${cert.domain}...` })}>{cert.pemPath}</button></div>
          </Panel>
          <Panel title="Troubleshooting Tips">
            <p className="muted">If the site shows a certificate warning, click Repair Trust.</p>
            <Button icon={<RefreshCw size={16} />} onClick={() => void run(() => api.trustCertificate(cert.id), { label: `Repairing trust for ${cert.domain}...` })}>Repair Trust</Button>
          </Panel>
        </aside>}
      </div>
    </>
  );
}
