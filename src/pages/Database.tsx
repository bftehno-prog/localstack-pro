import { Copy, Download, Eye, KeyRound, Plus, ShieldCheck, Trash2, Upload } from "lucide-react";
import { useState } from "react";
import { api } from "../ui/api";
import { pickSqlFile } from "../ui/dialogs";
import type { AppRun, AppSnapshot, DatabaseDiagnosticReport, DatabaseInfo } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { TopServices } from "../components/TopServices";

export function DatabasePage({ state, run }: { state: AppSnapshot; run: AppRun }) {
  const fallbackDb: DatabaseInfo = { id: "shop", name: "shop", description: "Main e-commerce DB", engine: "MySQL", version: "8.0.36", schemas: 0, user: "shop_user", password: "localstack", port: 3306, status: "stopped", sizeMb: 0, createdAt: new Date().toISOString() };
  const [selected, setSelected] = useState<DatabaseInfo>(state.databases[0] ?? fallbackDb);
  const [preset, setPreset] = useState<DatabaseInfo["engine"]>("MySQL");
  const [newName, setNewName] = useState("site_db");
  const [newUser, setNewUser] = useState("site_user");
  const [newPassword, setNewPassword] = useState(() => generatePassword());
  const [formError, setFormError] = useState("");
  const [diagnostics, setDiagnostics] = useState<DatabaseDiagnosticReport | null>(null);
  const db = state.databases.find((item) => item.id === selected.id) ?? state.databases[0] ?? fallbackDb;
  const total = state.databases.reduce((sum, item) => sum + item.sizeMb, 0);
  const createDb = () => {
    setFormError("");
    const name = newName.trim();
    const user = newUser.trim();
    const validation = validateDatabaseForm(name, user, newPassword, state.databases);
    if (validation) {
      setFormError(validation);
      return;
    }
    const port = preset === "PostgreSQL" ? 5432 : preset === "MariaDB" ? 3307 : 3306;
    const version = preset === "PostgreSQL" ? "15.3" : preset === "MariaDB" ? "10.11.6" : "8.0.36";
    void run(() => api.createDatabase({
      ...fallbackDb,
      id: name,
      name,
      description: `${name} database`,
      engine: preset,
      version,
      port,
      user,
      password: newPassword,
      status: "stopped",
      createdAt: new Date().toISOString()
    }), { label: `Creating ${preset} database ${name}...` });
  };
  const importSql = async () => {
    const path = await pickSqlFile();
    if (path) {
      await run(() => api.importDatabaseSql(db.id, path), { label: `Importing SQL into ${db.name}...` });
    }
  };
  const testConnection = async () => {
    const report = await run(() => api.testDatabaseConnection(db.id), { label: `Testing ${db.name} connection...` });
    if (report && typeof report === "object" && "databaseId" in report) {
      setDiagnostics(report);
    }
  };
  const copy = (value: string | number) => void navigator.clipboard?.writeText(String(value));
  return (
    <>
      <TopServices state={state} onStartAll={() => void run(api.startAll, { label: "Starting all services..." })} onStopAll={() => void run(api.stopAll, { label: "Stopping all services..." })} onRestartAll={() => void run(api.restartAll, { label: "Restarting all services..." })} onOpenSite={() => state.hosts[0] && void run(() => api.openHost(state.hosts[0].id), { label: `Opening ${state.hosts[0].domain}...` })} onToggleService={(serviceId, running) => void run(() => running ? api.stopService(serviceId) : api.startService(serviceId), { label: `${running ? "Stopping" : "Starting"} ${serviceId}...` })} />
      {formError && <div className="error-banner">{formError}</div>}
      <div className="page-grid">
        <section>
          <Panel title="Databases" action={<><select value={preset} onChange={(event) => setPreset(event.target.value as DatabaseInfo["engine"])}><option>MySQL</option><option>MariaDB</option><option>PostgreSQL</option></select><Button variant="primary" icon={<Plus size={16} />} onClick={createDb}>Create Database</Button><Button icon={<ShieldCheck size={16} />} onClick={() => void testConnection()}>Test Connection</Button><Button icon={<Download size={16} />} onClick={() => void importSql()}>Import SQL</Button><Button icon={<Upload size={16} />} onClick={() => void run(() => api.backupDatabase(db.id), { label: `Exporting ${db.name}...` })}>Export</Button></>}>
            <table className="data-table">
              <thead><tr><th>Database</th><th>Engine</th><th>Schemas</th><th>User</th><th>Password</th><th>Port</th><th>Status</th><th>Size</th></tr></thead>
              <tbody>{state.databases.map((row) => <tr key={row.id} className={db.id === row.id ? "selected" : ""} onClick={() => setSelected(row)}><td><strong>{row.name}</strong><small>{row.description}</small></td><td>{row.engine}<small>{row.version}</small></td><td>{row.schemas}</td><td>{row.user}</td><td>•••••••• <Eye size={14} /></td><td>{row.port}</td><td><StatusDot status={row.status} /></td><td>{row.sizeMb.toFixed(1)} MB</td></tr>)}</tbody>
            </table>
          </Panel>
          <div className="split">
          <Panel title="Quick Actions"><div className="quick-grid"><Button icon={<Plus size={16} />} onClick={createDb}>Create Database</Button><Button icon={<ShieldCheck size={16} />} onClick={() => void testConnection()}>Test Connection</Button><Button icon={<Download size={16} />} onClick={() => void importSql()}>Import SQL</Button><Button icon={<Upload size={16} />} onClick={() => void run(() => api.backupDatabase(db.id), { label: `Exporting ${db.name}...` })}>Export</Button><Button onClick={() => void run(() => api.openDatabaseAdmin("phpmyadmin"), { label: "Opening phpMyAdmin..." })}>Open phpMyAdmin</Button><Button onClick={() => void run(() => api.openDatabaseAdmin("adminer"), { label: "Opening Adminer..." })}>Open Adminer</Button><Button icon={<Trash2 size={16} />} variant="danger" onClick={() => void run(() => api.deleteDatabase(db.id), { label: `Deleting ${db.name}...` })}>Delete</Button></div></Panel>
            <Panel title="Database Usage"><div className="donut" style={{ background: "conic-gradient(var(--blue) 0 30%, var(--green) 30% 55%, var(--brand-3) 55% 100%)" }}><strong>{Math.round(total)} MB</strong><small>Total</small></div></Panel>
          </div>
          <Panel title="Recent Activity / Query Log"><div className="log-box compact-log">{state.logs.filter((log) => log.service === "MySQL").map((log) => <pre key={log.id}>[{new Date(log.timestamp).toLocaleTimeString()}] &gt; {log.message}</pre>)}</div></Panel>
        </section>
        <aside className="detail-rail">
          <Panel title="Create Database">
            <div className="form-grid">
              <label>
                Engine
                <select value={preset} onChange={(event) => setPreset(event.target.value as DatabaseInfo["engine"])}>
                  <option>MySQL</option>
                  <option>MariaDB</option>
                  <option>PostgreSQL</option>
                </select>
              </label>
              <label>
                Database Name
                <input value={newName} onChange={(event) => {
                  const previousUser = `${newName.trim()}_user`;
                  const nextName = event.target.value;
                  setNewName(nextName);
                  if (newUser === previousUser) setNewUser(`${nextName.trim()}_user`);
                }} placeholder="site_db" />
              </label>
              <label>
                Database User
                <input value={newUser} onChange={(event) => setNewUser(event.target.value)} placeholder="site_user" />
              </label>
              <label>
                Database Password
                <input type="text" value={newPassword} onChange={(event) => setNewPassword(event.target.value)} />
              </label>
              <button className="input-button" type="button" onClick={() => setNewPassword(generatePassword())}>Generate password <KeyRound size={15} /></button>
              <Button variant="primary" icon={<Plus size={16} />} onClick={createDb}>Create Database</Button>
            </div>
          </Panel>
          <Panel title={db.name} action={<StatusDot status={db.status} />}>
            <div className="kv form-kv"><span>Engine</span><strong>{db.engine} {db.version}</strong><span>Host</span><button onClick={() => copy("127.0.0.1")}>127.0.0.1<Copy size={15} /></button><span>Port</span><button onClick={() => copy(db.port)}>{db.port}<Copy size={15} /></button><span>Database</span><button onClick={() => copy(db.name)}>{db.name}<Copy size={15} /></button><span>Username</span><button onClick={() => copy(db.user)}>{db.user}<Copy size={15} /></button><span>Password</span><button onClick={() => copy(db.password)}>••••••••<Copy size={15} /></button><span>Connection String</span><button onClick={() => copy(`${db.engine.toLowerCase()}://${db.user}:${db.password}@127.0.0.1:${db.port}/${db.name}`)}>{`${db.engine.toLowerCase()}://${db.user}:********@127.0.0.1:${db.port}/${db.name}`}<Copy size={15} /></button></div>
          </Panel>
          {diagnostics?.databaseId === db.id && (
            <Panel title="Connection Diagnostics" action={<StatusDot status={diagnostics.errors > 0 ? "error" : diagnostics.warnings > 0 ? "warning" : "valid"} label={diagnostics.errors > 0 ? "Issues" : diagnostics.warnings > 0 ? "Warnings" : "Healthy"} />}>
              <div className="kv detail-kv">
                <span>Summary</span><strong>{diagnostics.summary}</strong>
                <span>Checks</span><strong>{diagnostics.ok} OK / {diagnostics.warnings} Warning / {diagnostics.errors} Error</strong>
              </div>
              <table className="data-table diagnostics-table">
                <tbody>
                  {diagnostics.checks.map((check) => (
                    <tr key={check.id}>
                      <td><StatusDot status={check.severity === "ok" ? "valid" : check.severity} label={check.title} /></td>
                      <td><strong>{check.message}</strong>{check.detail && <small>{check.detail}</small>}{check.action && <small>{check.action}</small>}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Panel>
          )}
          <Panel title="Recent Backups"><p className="muted">{db.name}_backup.sql.gz</p><p className="muted">{db.name}_yesterday.sql.gz</p></Panel>
        </aside>
      </div>
    </>
  );
}

function validateDatabaseForm(name: string, user: string, password: string, databases: DatabaseInfo[]) {
  if (!name) return "Database name is required.";
  if (!user) return "Database user is required.";
  if (!/^[a-zA-Z0-9_]+$/.test(name)) return "Database name can contain only letters, numbers and underscores.";
  if (!/^[a-zA-Z0-9_]+$/.test(user)) return "Database user can contain only letters, numbers and underscores.";
  if (password.length < 8) return "Database password must be at least 8 characters.";
  if (databases.some((item) => item.id.toLowerCase() === name.toLowerCase() || item.name.toLowerCase() === name.toLowerCase())) {
    return "Database with this name already exists.";
  }
  return "";
}

function generatePassword() {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  const bytes = new Uint8Array(18);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => alphabet[byte % alphabet.length]).join("");
}
