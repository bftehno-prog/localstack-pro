import { ArrowLeft, ExternalLink, GitBranch, KeyRound, Plus, Save, TestTube2, Trash2, X } from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../ui/api";
import type { AppRun, AppSnapshot, DatabaseInfo, HostInfo, ProjectInspection } from "../ui/types";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { hostUrl } from "./Overview";

export function HostEditorPage({
  state,
  initial,
  back,
  run
}: {
  state: AppSnapshot;
  initial?: HostInfo;
  back: () => void;
  run: AppRun;
}) {
  const blank = useMemo<HostInfo>(() => ({
    id: crypto.randomUUID(),
    domain: "new.test",
    rootFolder: projectRootForDomain(state.settings.projectsFolder, "new.test"),
    documentRoot: "public",
    phpVersion: state.phpVersions[0]?.version ?? "8.1.23",
    webServer: "Apache",
    ssl: false,
    environment: "Development",
    httpPort: 80,
    httpsPort: 443,
    database: "new_db",
    mailService: "Mailpit",
    envVariables: { APP_ENV: "local", APP_DEBUG: "true" },
    rewriteRules: "",
    notes: "",
    status: "stopped",
    tags: [],
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString()
  }), [state]);
  const [host, setHost] = useState<HostInfo>(initial ?? blank);
  const [databaseEngine, setDatabaseEngine] = useState<DatabaseInfo["engine"]>(() => databaseEngineFromPreset(databasePreset((initial ?? blank).database)));
  const [createDatabaseWithHost, setCreateDatabaseWithHost] = useState(!initial);
  const [databaseUser, setDatabaseUser] = useState(() => `${normalizeDatabaseName((initial ?? blank).database)}_user`);
  const [databasePassword, setDatabasePassword] = useState(() => generatePassword());
  const [environmentPreset, setEnvironmentPreset] = useState("PHP CMS");
  const [gitUrl, setGitUrl] = useState("");
  const [projectInfo, setProjectInfo] = useState<ProjectInspection>();
  const [vaultName, setVaultName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [configOk, setConfigOk] = useState<string | null>(null);
  const forceHttps = host.rewriteRules.includes("FORCE_HTTPS=1");

  const update = <K extends keyof HostInfo>(key: K, value: HostInfo[K]) => {
    setError(null);
    setConfigOk(null);
    setHost((current) => ({ ...current, [key]: value, updatedAt: new Date().toISOString() }));
  };
  const updateDomain = (domain: string) => {
    setError(null);
    setConfigOk(null);
    setHost((current) => {
      const previousDefault = projectRootForDomain(state.settings.projectsFolder, current.domain);
      const rootWasAutomatic = !initial && (
        current.rootFolder === previousDefault ||
        current.rootFolder === state.settings.projectsFolder
      );
      const previousDb = `${domainSlug(current.domain)}_db`;
      const nextDb = `${domainSlug(domain)}_db`;
      const databaseWasAutomatic = !initial && (
        current.database === previousDb ||
        current.database === "new_db" ||
        current.database === "mysql_local"
      );
      if (databaseWasAutomatic) {
        setDatabaseUser(`${normalizeDatabaseName(nextDb)}_user`);
      }
      return {
        ...current,
        domain,
        database: databaseWasAutomatic ? nextDb : current.database,
        rootFolder: rootWasAutomatic ? projectRootForDomain(state.settings.projectsFolder, domain) : current.rootFolder,
        updatedAt: new Date().toISOString()
      };
    });
  };
  const addVariable = () => {
    const name = window.prompt("Environment variable name", "APP_KEY");
    if (!name) return;
    const value = window.prompt("Environment variable value", "");
    update("envVariables", { ...host.envVariables, [name]: value ?? "" });
  };
  const removeVariable = (name: string) => {
    const next = { ...host.envVariables };
    delete next[name];
    update("envVariables", next);
  };
  const toggleForceHttps = () => update("rewriteRules", forceHttps ? host.rewriteRules.replace(/\n?FORCE_HTTPS=1/g, "") : `${host.rewriteRules}${host.rewriteRules ? "\n" : ""}FORCE_HTTPS=1`);
  const applyPreset = (preset: string) => {
    setEnvironmentPreset(preset);
    const presetSlug = preset.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
    setHost((current) => ({
      ...current,
      documentRoot: preset === "Next.js" || preset === "Static" ? "." : "public",
      webServer: preset === "Next.js" || preset === "Static" ? "Nginx" : "Apache",
      ssl: preset === "Production" || current.ssl,
      database: preset === "Static" || preset === "Next.js" ? current.database : current.database || `${presetSlug}_db`,
      tags: Array.from(new Set([...current.tags, presetSlug])),
      updatedAt: new Date().toISOString()
    }));
    if (preset === "Next.js" || preset === "Static") {
      setCreateDatabaseWithHost(false);
    }
  };
  const updateDatabaseName = (value: string) => {
    const previousUser = `${normalizeDatabaseName(host.database)}_user`;
    update("database", value);
    const nextName = normalizeDatabaseName(value);
    if (!databaseUser.trim() || databaseUser === previousUser) {
      setDatabaseUser(`${nextName}_user`);
    }
  };
  const inspectProject = async () => {
    const result = await run(() => api.inspectProject(host.rootFolder), { label: `Inspecting ${host.domain} project...` });
    if (result && typeof result === "object" && "kind" in result) {
      const info = result as ProjectInspection;
      setProjectInfo(info);
      applyPreset(info.kind);
      update("documentRoot", info.documentRoot);
    }
  };
  const saveVault = () => {
    const key = vaultName.trim() || host.domain;
    const current = JSON.parse(localStorage.getItem("localstack.dbVault") ?? "[]") as Array<Record<string, string>>;
    const next = current.filter((item) => item.name !== key);
    next.unshift({ name: key, database: host.database, user: databaseUser, password: databasePassword, engine: databaseEngine });
    localStorage.setItem("localstack.dbVault", JSON.stringify(next.slice(0, 20)));
    setVaultName(key);
  };
  const applyVault = (name: string) => {
    const items = JSON.parse(localStorage.getItem("localstack.dbVault") ?? "[]") as Array<Record<string, string>>;
    const item = items.find((entry) => entry.name === name);
    if (!item) return;
    update("database", item.database ?? host.database);
    setDatabaseUser(item.user ?? databaseUser);
    setDatabasePassword(item.password ?? databasePassword);
    setDatabaseEngine((item.engine as DatabaseInfo["engine"]) ?? databaseEngine);
  };
  const vaultItems = (() => {
    try { return JSON.parse(localStorage.getItem("localstack.dbVault") ?? "[]") as Array<Record<string, string>>; } catch { return []; }
  })();
  const save = async (start: boolean) => {
    const validation = validateHost(host, state.hosts.filter((item) => item.id !== host.id));
    if (validation) {
      setError(validation);
      return;
    }
    const databaseName = normalizeDatabaseName(host.database);
    const databaseValidation = createDatabaseWithHost ? validateDatabase(databaseName, databaseUser, databasePassword, state.databases) : null;
    if (databaseValidation) {
      setError(databaseValidation);
      return;
    }
    const hostToSave: HostInfo = {
      ...host,
      database: databaseName,
      envVariables: createDatabaseWithHost
        ? databaseEnvVariables(host.envVariables, databaseEngine, databaseName, databaseUser.trim(), databasePassword)
        : host.envVariables
    };
    if (createDatabaseWithHost) {
      const serviceId = databaseServiceId(databaseEngine);
      const service = state.services.find((item) => item.id === serviceId);
      if (service?.status !== "running") {
        await run(() => api.startService(serviceId), { label: `Starting ${databaseEngine}...` });
      }
      await run(() => api.createDatabase({
        id: databaseName,
        name: databaseName,
        description: `${host.domain} database`,
        engine: databaseEngine,
        version: databaseVersion(databaseEngine),
        schemas: 1,
        user: databaseUser.trim(),
        password: databasePassword,
        port: databasePort(databaseEngine),
        status: "stopped",
        sizeMb: 0,
        createdAt: new Date().toISOString()
      }), { label: `Creating database ${databaseName}...` });
    }
    await run(() => api.saveHost(hostToSave), { label: `Saving ${hostToSave.domain}...` });
    if (start) {
      const serviceId = host.webServer.toLowerCase() === "nginx" ? "nginx" : "apache";
      await run(() => api.startService(serviceId), { label: `Starting ${host.webServer}...` });
    }
    back();
  };

  return (
    <div className="editor-page">
      <div className="editor-head">
        <Button variant="icon" icon={<ArrowLeft size={20} />} onClick={back} />
        <div><h1>New Host / Edit Host</h1><p>Create a new local site or edit an existing one</p></div>
      </div>
      {error && <div className="error-banner">{error}</div>}
      {configOk && <div className="success-banner">{configOk}</div>}
      <div className="editor-grid">
        <section className="editor-main">
          <Panel title="Basic Information">
            <div className="form-grid">
              <label>Environment Preset<select value={environmentPreset} onChange={(event) => applyPreset(event.target.value)}><option>PHP CMS</option><option>WordPress</option><option>Laravel</option><option>Next.js</option><option>Static</option><option>Production</option></select></label>
              <label>Host Name *<input value={host.domain} onChange={(event) => updateDomain(event.target.value)} /></label>
              <label>Domain *<input value={host.domain} onChange={(event) => updateDomain(event.target.value)} /></label>
              <label>Description<input value={host.notes} onChange={(event) => update("notes", event.target.value)} /></label>
            </div>
          </Panel>
          <Panel title="Git Import">
            <div className="form-grid">
              <label>Repository URL<input value={gitUrl} onChange={(event) => setGitUrl(event.target.value)} placeholder="https://github.com/user/project.git" /></label>
              <Button icon={<GitBranch size={16} />} disabled={!gitUrl.trim()} onClick={() => void run(() => api.cloneProjectRepository(gitUrl, host.rootFolder), { label: `Cloning repository into ${host.rootFolder}...` })}>Import from Git</Button>
            </div>
          </Panel>
          <Panel title="Project Doctor" action={<Button icon={<TestTube2 size={16} />} onClick={() => void inspectProject()}>Detect Project</Button>}>
            {projectInfo ? (
              <>
                <div className="kv detail-kv"><span>Type</span><strong>{projectInfo.kind}</strong><span>Document Root</span><strong>{projectInfo.documentRoot}</strong><span>Commands</span><strong>{projectInfo.commands.join(", ") || "None"}</strong></div>
                <table className="mini-table"><tbody>{projectInfo.checks.map((check) => <tr key={check.title}><td>{check.title}</td><td>{check.severity}</td><td>{check.message}</td></tr>)}</tbody></table>
                <Button onClick={() => void run(() => api.generateEnvTemplate(host.rootFolder, projectInfo.kind, host.database, databaseUser, databasePassword, host.domain), { label: `Generating .env for ${host.domain}...` })}>Generate .env</Button>
              </>
            ) : <p className="muted">Detect project type, document root, commands and .env template.</p>}
          </Panel>
          <Panel title="Paths">
            <div className="form-grid two">
              <label>Root Folder *<input value={host.rootFolder} onChange={(event) => update("rootFolder", event.target.value)} /></label>
              <label>Document Root *<input value={host.documentRoot} onChange={(event) => update("documentRoot", event.target.value)} /></label>
            </div>
            <p className="hint">Full path: {host.rootFolder}\\{host.documentRoot}</p>
          </Panel>
          <Panel title="PHP & Web Server">
            <div className="form-grid two">
              <label>PHP Version<select value={host.phpVersion} onChange={(event) => update("phpVersion", event.target.value)}>{state.phpVersions.map((php) => <option key={php.version}>{php.version}</option>)}</select></label>
              <label>Web Server<select value={host.webServer} onChange={(event) => update("webServer", event.target.value)}><option>Apache</option><option>Nginx</option></select></label>
              <label className="toggle-line">Enable SSL<span className={`toggle ${host.ssl ? "on" : ""}`} onClick={() => update("ssl", !host.ssl)} /></label>
              <label className="toggle-line">Force HTTPS<span className={`toggle ${forceHttps ? "on" : ""}`} onClick={toggleForceHttps} /></label>
            </div>
          </Panel>
          <Panel title="Custom Ports">
            <div className="form-grid two">
              <label>HTTP Port<input type="number" value={host.httpPort} onChange={(event) => update("httpPort", Number(event.target.value))} /></label>
              <label>HTTPS Port<input type="number" value={host.httpsPort} onChange={(event) => update("httpsPort", Number(event.target.value))} /></label>
            </div>
          </Panel>
          <Panel title="Database">
            <div className="form-grid two">
              <label>Credential Vault<select value="" onChange={(event) => applyVault(event.target.value)}><option value="">Select saved credentials</option>{vaultItems.map((item) => <option key={item.name} value={item.name}>{item.name}</option>)}</select></label>
              <label>Vault Name<input value={vaultName} onChange={(event) => setVaultName(event.target.value)} placeholder={host.domain} /></label>
              <label>Database Preset<select value={databasePresetFromEngine(databaseEngine)} onChange={(event) => setDatabaseEngine(databaseEngineFromPreset(event.target.value))}><option>MySQL 8.0</option><option>MariaDB 10.6</option><option>PostgreSQL 15</option></select></label>
              <label className="toggle-line">Create Database<span className={`toggle ${createDatabaseWithHost ? "on" : ""}`} onClick={() => setCreateDatabaseWithHost((value) => !value)} /></label>
              <label>Database Name<input value={host.database} onChange={(event) => updateDatabaseName(event.target.value)} /></label>
              <label>Database User<input value={databaseUser} disabled={!createDatabaseWithHost} onChange={(event) => setDatabaseUser(event.target.value)} /></label>
              <label>Database Password<input type="text" value={databasePassword} disabled={!createDatabaseWithHost} onChange={(event) => setDatabasePassword(event.target.value)} /></label>
              <label>Password Tool<button className="input-button" type="button" disabled={!createDatabaseWithHost} onClick={() => setDatabasePassword(generatePassword())}>Generate password <KeyRound size={15} /></button></label>
              <label>Vault Tool<button className="input-button" type="button" onClick={saveVault}>Save credentials <KeyRound size={15} /></button></label>
            </div>
          </Panel>
          <Panel title="Environment Variables" action={<Button icon={<Plus size={15} />} onClick={addVariable}>Add Variable</Button>}>
            <table className="mini-table"><tbody>{Object.entries(host.envVariables).map(([key, value]) => <tr key={key}><td>{key}</td><td>{value}</td><td><Button variant="icon" icon={<Trash2 size={15} />} onClick={() => removeVariable(key)} /></td></tr>)}</tbody></table>
          </Panel>
          <Panel title="Mail Testing">
            <div className="form-grid two">
              <label>Mail Service<select value={host.mailService} onChange={(event) => update("mailService", event.target.value)}><option>Mailpit</option><option>Disabled</option></select></label>
              <label>SMTP From Address<input value={`noreply@${host.domain}`} readOnly /></label>
            </div>
          </Panel>
          <Panel title="Notes">
            <textarea value={host.notes} onChange={(event) => update("notes", event.target.value)} />
          </Panel>
        </section>
        <aside className="detail-rail">
          <Panel title="Live Preview" action={<StatusDot status={host.status} />}>
            <div className="preview-url">{hostUrl(host)} <ExternalLink size={16} /></div>
            <Button icon={<ExternalLink size={17} />} onClick={() => void run(() => initial ? api.openHost(host.id) : api.openUrl(hostUrl(host)))}>Open in Browser</Button>
          </Panel>
          <Panel title="Status Summary">
            <div className="kv detail-kv">
              <span>Document Root</span><strong>{host.rootFolder}\\{host.documentRoot}</strong>
              <span>Web Root URL</span><strong>{hostUrl(host)}</strong>
              <span>SSL</span><strong>{host.ssl ? "Enabled" : "Disabled"}</strong>
              <span>HTTP Port</span><strong>{host.httpPort}</strong>
              <span>HTTPS Port</span><strong>{host.httpsPort}</strong>
              <span>Database</span><strong>{host.database}</strong>
            </div>
          </Panel>
          <Panel title="Actions">
            <div className="stack-buttons">
              <Button variant="primary" icon={<Save size={16} />} onClick={() => void save(false)}>Save</Button>
              <Button variant="primary" icon={<Save size={16} />} onClick={() => void save(true)}>Save & Start</Button>
              <Button icon={<TestTube2 size={16} />} onClick={() => {
                const validation = validateHost(host, state.hosts.filter((item) => item.id !== host.id));
                setError(validation);
                setConfigOk(validation ? null : "Configuration test passed.");
              }}>Test Configuration</Button>
              <Button icon={<X size={16} />} onClick={back}>Cancel</Button>
            </div>
          </Panel>
        </aside>
      </div>
    </div>
  );
}

function validateHost(host: HostInfo, others: HostInfo[]) {
  if (!/^[a-z0-9][a-z0-9.-]*\.[a-z]{2,}$/i.test(host.domain)) return "Domain must look like local.test.";
  if (!host.rootFolder.trim()) return "Root folder is required.";
  if (!host.documentRoot.trim()) return "Document root is required.";
  if (others.some((item) => item.domain === host.domain)) return "A host with this domain already exists.";
  if (host.httpPort < 1 || host.httpPort > 65535 || host.httpsPort < 1 || host.httpsPort > 65535) return "Ports must be between 1 and 65535.";
  return null;
}

function validateDatabase(databaseName: string, user: string, password: string, databases: DatabaseInfo[]) {
  if (!databaseName) return "Database name is required.";
  if (!/^[a-zA-Z0-9_]+$/.test(databaseName)) return "Database name can contain only letters, numbers and underscores.";
  if (!/^[a-zA-Z0-9_]+$/.test(user.trim())) return "Database user can contain only letters, numbers and underscores.";
  if (password.length < 8) return "Database password must be at least 8 characters.";
  if (databases.some((item) => item.id.toLowerCase() === databaseName.toLowerCase() || item.name.toLowerCase() === databaseName.toLowerCase())) {
    return "Database with this name already exists.";
  }
  return null;
}

function databasePreset(database: string) {
  const value = database.toLowerCase();
  if (value.includes("post") || value.includes("pg")) return "PostgreSQL 15";
  if (value.includes("maria")) return "MariaDB 10.6";
  return "MySQL 8.0";
}

function databaseEngineFromPreset(preset: string): DatabaseInfo["engine"] {
  if (preset.includes("PostgreSQL")) return "PostgreSQL";
  if (preset.includes("MariaDB")) return "MariaDB";
  return "MySQL";
}

function databasePresetFromEngine(engine: DatabaseInfo["engine"]) {
  if (engine === "PostgreSQL") return "PostgreSQL 15";
  if (engine === "MariaDB") return "MariaDB 10.6";
  return "MySQL 8.0";
}

function databaseVersion(engine: DatabaseInfo["engine"]) {
  if (engine === "PostgreSQL") return "15.3";
  if (engine === "MariaDB") return "10.11.6";
  return "8.0.36";
}

function databasePort(engine: DatabaseInfo["engine"]) {
  if (engine === "PostgreSQL") return 5432;
  if (engine === "MariaDB") return 3307;
  return 3306;
}

function databaseServiceId(engine: DatabaseInfo["engine"]) {
  if (engine === "PostgreSQL") return "postgresql";
  if (engine === "MariaDB") return "mariadb";
  return "mysql";
}

function databaseConnection(engine: DatabaseInfo["engine"]) {
  return engine === "PostgreSQL" ? "pgsql" : "mysql";
}

function databaseUrl(engine: DatabaseInfo["engine"], name: string, user: string, password: string) {
  const scheme = engine === "PostgreSQL" ? "postgresql" : "mysql";
  return `${scheme}://${encodeURIComponent(user)}:${encodeURIComponent(password)}@127.0.0.1:${databasePort(engine)}/${encodeURIComponent(name)}`;
}

function databaseEnvVariables(current: Record<string, string>, engine: DatabaseInfo["engine"], name: string, user: string, password: string) {
  const host = "127.0.0.1";
  const port = String(databasePort(engine));
  return {
    ...current,
    DB_CONNECTION: databaseConnection(engine),
    DB_HOST: host,
    DB_PORT: port,
    DB_DATABASE: name,
    DB_NAME: name,
    DB_USERNAME: user,
    DB_USER: user,
    DB_PASSWORD: password,
    DB_PASS: password,
    MYSQL_HOST: host,
    MYSQL_PORT: port,
    MYSQL_DATABASE: name,
    MYSQL_USER: user,
    MYSQL_PASSWORD: password,
    DATABASE_URL: databaseUrl(engine, name, user, password)
  };
}

function normalizeDatabaseName(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_{2,}/g, "_") || "local_db";
}

function generatePassword() {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  const bytes = new Uint8Array(18);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => alphabet[byte % alphabet.length]).join("");
}

function projectRootForDomain(projectsFolder: string, domain: string) {
  const base = projectsFolder.replace(/[\\/]+$/, "");
  const folder = domainSlug(domain);
  return base ? `${base}\\${folder}` : folder;
}

function domainSlug(domain: string) {
  return domain
    .trim()
    .toLowerCase()
    .replace(/\.[^.]+$/, "")
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "") || "site";
}
