import { Download, ExternalLink, Folder, Globe2, KeyRound, PackagePlus, Play, RefreshCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Button } from "../components/Button";
import { Panel } from "../components/Panel";
import { StatusDot } from "../components/StatusDot";
import { api } from "../ui/api";
import { pickFolder } from "../ui/dialogs";
import type { AppRun, AppSnapshot, CmsInstallRequest, CmsTemplate, DatabaseInfo, HostInfo, ProjectInspection } from "../ui/types";

export function CmsPage({
  state,
  run
}: {
  state: AppSnapshot;
  run: AppRun;
}) {
  const [templates, setTemplates] = useState<CmsTemplate[]>([]);
  const [selectedId, setSelectedId] = useState("wordpress");
  const [domain, setDomain] = useState("wordpress.test");
  const [rootFolder, setRootFolder] = useState(`${state.settings.projectsFolder}\\wordpress`);
  const [phpVersion, setPhpVersion] = useState(state.phpVersions.find((item) => item.default)?.version ?? state.phpVersions[0]?.version ?? "");
  const [webServer, setWebServer] = useState("Apache");
  const [ssl, setSsl] = useState(false);
  const [databaseEngine, setDatabaseEngine] = useState<DatabaseInfo["engine"]>("MySQL");
  const [createDatabase, setCreateDatabase] = useState(true);
  const [databaseName, setDatabaseName] = useState("wordpress");
  const [databaseUser, setDatabaseUser] = useState("wordpress_user");
  const [databasePassword, setDatabasePassword] = useState(() => generatePassword());
  const [overwrite, setOverwrite] = useState(false);
  const [formError, setFormError] = useState("");
  const [lastInstallDomain, setLastInstallDomain] = useState("");
  const [envPreset, setEnvPreset] = useState("Laravel");
  const [envPreview, setEnvPreview] = useState("");
  const [projectChecks, setProjectChecks] = useState<string[]>([]);

  useEffect(() => {
    void api.getCmsTemplates().then((items) => {
      setTemplates(items);
      const first = items[0];
      if (first) {
        setSelectedId(first.id);
        setCreateDatabase(first.requiresDatabase);
        setDatabaseEngine(first.defaultDatabaseEngine);
      }
    });
  }, []);

  const selected = useMemo(() => templates.find((item) => item.id === selectedId) ?? templates[0], [selectedId, templates]);
  const selectedIsNode = selected?.category === "Node.js";
  const servicesReady = state.services.some((service) => service.id === "apache" && service.status === "running")
    || state.services.some((service) => service.id === "nginx" && service.status === "running");

  const selectTemplate = (template: CmsTemplate) => {
    setSelectedId(template.id);
    setCreateDatabase(template.requiresDatabase);
    setDatabaseEngine(template.defaultDatabaseEngine);
    setWebServer("Apache");
    const slug = template.id.replace(/[^a-z0-9]+/g, "-");
    setDomain(`${slug}.test`);
    setRootFolder(`${state.settings.projectsFolder}\\${slug}`);
    setDatabaseName(sanitizeDatabaseName(slug));
    setDatabaseUser(`${sanitizeDatabaseName(slug)}_user`);
  };

  const install = async () => {
    setFormError("");
    if (!selected) {
      setFormError("Select CMS template.");
      return;
    }
    if (!domain.includes(".") || domain.includes(" ")) {
      setFormError("Domain must look like cms.test.");
      return;
    }
    if (!rootFolder.trim()) {
      setFormError("Project folder is required.");
      return;
    }
    if (selected.requiresDatabase && createDatabase) {
      const dbName = sanitizeDatabaseName(databaseName);
      const dbUser = sanitizeDatabaseName(databaseUser);
      if (!dbName || !dbUser) {
        setFormError("Database name and user are required.");
        return;
      }
      if (databasePassword.length < 8) {
        setFormError("Database password must be at least 8 characters.");
        return;
      }
    }
    const request: CmsInstallRequest = {
      templateId: selected.id,
      domain: domain.trim(),
      rootFolder: rootFolder.trim(),
      phpVersion,
      webServer,
      ssl,
      databaseEngine,
      createDatabase,
      databaseName: sanitizeDatabaseName(databaseName),
      databaseUser: sanitizeDatabaseName(databaseUser),
      databasePassword,
      overwrite
    };
    await run(() => api.installCms(request), { label: `Installing ${selected.name} at ${request.domain}...` });
    setLastInstallDomain(request.domain);
    await run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file...", silent: true }).catch(() => undefined);
  };
  const buildEnvPreset = (preset = envPreset) => {
    const appUrl = `${ssl ? "https" : "http"}://${domain.trim()}`;
    const rows = preset === "Next.js"
      ? [
        `NEXT_PUBLIC_APP_URL=${appUrl}`,
        `DATABASE_URL=mysql://${databaseUser}:${databasePassword}@127.0.0.1:3306/${databaseName}`,
        `MYSQL_HOST=127.0.0.1`,
        `MYSQL_PORT=3306`,
        `MYSQL_DATABASE=${databaseName}`,
        `MYSQL_USER=${databaseUser}`,
        `MYSQL_PASSWORD=${databasePassword}`
      ]
      : preset === "WordPress"
        ? [
          `WORDPRESS_DB_HOST=127.0.0.1:3306`,
          `WORDPRESS_DB_NAME=${databaseName}`,
          `WORDPRESS_DB_USER=${databaseUser}`,
          `WORDPRESS_DB_PASSWORD=${databasePassword}`,
          `WP_HOME=${appUrl}`,
          `WP_SITEURL=${appUrl}`
        ]
        : [
          `APP_NAME=${domain.split(".")[0] || "LocalStack"}`,
          `APP_ENV=local`,
          `APP_DEBUG=true`,
          `APP_URL=${appUrl}`,
          `DB_CONNECTION=mysql`,
          `DB_HOST=127.0.0.1`,
          `DB_PORT=3306`,
          `DB_DATABASE=${databaseName}`,
          `DB_USERNAME=${databaseUser}`,
          `DB_PASSWORD=${databasePassword}`,
          `MAIL_MAILER=smtp`,
          `MAIL_HOST=127.0.0.1`,
          `MAIL_PORT=1025`
        ];
    const text = `${rows.join("\n")}\n`;
    setEnvPreset(preset);
    setEnvPreview(text);
    return text;
  };
  const writeEnvFile = async () => {
    const content = envPreview || buildEnvPreset();
    await run(() => api.writeFileWithEncoding(`${rootFolder.trim()}\\.env`, content, "utf-8"), { label: "Writing .env file..." });
  };
  const deployExistingProject = async () => {
    const slug = domain.split(".")[0].replace(/[^a-z0-9_-]+/gi, "").toLowerCase() || "project";
    const now = new Date().toISOString();
    const needsDb = createDatabase && databaseName.trim();
    if (needsDb && !state.databases.some((item) => item.name === databaseName || item.id === databaseName)) {
      await run(() => api.createDatabase({
        id: databaseName,
        name: databaseName,
        description: `${domain} database`,
        engine: databaseEngine,
        version: databaseEngine === "PostgreSQL" ? "15.3" : databaseEngine === "MariaDB" ? "10.11.6" : "8.0.36",
        schemas: 1,
        user: databaseUser,
        password: databasePassword,
        port: databaseEngine === "PostgreSQL" ? 5432 : databaseEngine === "MariaDB" ? 3307 : 3306,
        status: "stopped",
        sizeMb: 0,
        createdAt: now
      }), { label: `Creating database ${databaseName}...` });
    }
    const host: HostInfo = {
      id: domain.trim().toLowerCase(),
      domain: domain.trim().toLowerCase(),
      rootFolder: rootFolder.trim(),
      documentRoot: selectedIsNode ? "." : selected?.documentRoot ?? "public",
      phpVersion,
      webServer,
      ssl,
      environment: "Development",
      httpPort: 80,
      httpsPort: 443,
      database: databaseName,
      mailService: "Mailpit",
      envVariables: {
        APP_ENV: "local",
        APP_DEBUG: "true",
        APP_URL: `${ssl ? "https" : "http"}://${domain.trim().toLowerCase()}`,
        DB_DATABASE: databaseName,
        DB_USERNAME: databaseUser,
        DB_PASSWORD: databasePassword
      },
      rewriteRules: "",
      notes: `Existing project deployed from ${rootFolder}`,
      status: "stopped",
      tags: [selectedIsNode ? "node" : "php", slug],
      createdAt: now,
      updatedAt: now
    };
    await writeEnvFile().catch(() => undefined);
    await run(() => api.saveHost(host), { label: `Creating host ${host.domain}...` });
    await run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file...", silent: true }).catch(() => undefined);
    setLastInstallDomain(host.domain);
  };
  const importExistingProject = async () => {
    const folder = await pickFolder();
    if (!folder) return;
    setRootFolder(folder);
    const slug = folder.split(/[\\/]/).pop()?.replace(/[^a-z0-9_-]+/gi, "-").toLowerCase() || "project";
    setDomain(`${slug}.test`);
    setDatabaseName(sanitizeDatabaseName(slug));
    setDatabaseUser(`${sanitizeDatabaseName(slug)}_user`);
    const result = await run(() => api.inspectProject(folder), { label: "Inspecting existing project..." });
    if (result && typeof result === "object" && "kind" in result && "checks" in result) {
      const inspection = result as ProjectInspection;
      setProjectChecks(inspection.checks.map((check) => `${check.severity}: ${check.title} - ${check.message}`));
      if (inspection.kind.toLowerCase().includes("node")) {
        setSelectedId("nextjs");
        setCreateDatabase(false);
      }
      if (inspection.documentRoot) {
        const match = templates.find((template) => template.documentRoot === inspection.documentRoot || inspection.kind.toLowerCase().includes(template.id));
        if (match) setSelectedId(match.id);
      }
    }
    const envExample = await api.readFileWithEncoding(`${folder}\\.env.example`, "utf-8").catch(() => undefined);
    if (envExample?.content) setEnvPreview(envExample.content);
  };
  const refreshTemplates = async () => {
    const result = await run(api.getCmsTemplates, { label: "Refreshing CMS templates..." });
    if (Array.isArray(result)) {
      setTemplates(result as CmsTemplate[]);
    }
  };

  return (
    <div className="cms-page">
      <div className="page-title">
        <h1>CMS <small>{templates.length}</small></h1>
        <div className="toolbar">
          <Button icon={<RefreshCw size={16} />} onClick={() => void refreshTemplates()}>Refresh</Button>
          <Button icon={<Folder size={16} />} onClick={() => void importExistingProject()}>Import Project</Button>
          <Button variant="primary" icon={<PackagePlus size={16} />} disabled={!selected} onClick={() => void install()}>
            Install CMS
          </Button>
        </div>
      </div>
      {formError && <div className="error-banner">{formError}</div>}
      <div className="cms-grid">
        <section className="stack-left">
          <Panel title="Preset Marketplace">
            <div className="preset-grid">
              {["WordPress", "Laravel", "Symfony", "Next.js", "Nuxt", "Astro", "Vite", "Express", "NestJS", "Custom PHP"].map((preset) => (
                <button key={preset} onClick={() => {
                  const template = templates.find((item) => item.name.toLowerCase().includes(preset.toLowerCase()) || item.id.toLowerCase().includes(preset.toLowerCase().replace(".", "")));
                  if (template) {
                    selectTemplate(template);
                  } else {
                    const slug = preset.toLowerCase().replace(/[^a-z0-9]+/g, "-");
                    setDomain(`${slug}.test`);
                    setRootFolder(`${state.settings.projectsFolder}\\${slug}`);
                    setDatabaseName(sanitizeDatabaseName(slug));
                    setDatabaseUser(`${sanitizeDatabaseName(slug)}_user`);
                    setCreateDatabase(!["next-js", "nuxt", "astro", "vite", "express", "nestjs"].includes(slug));
                  }
                }}>
                  <PackagePlus size={16} />
                  <strong>{preset}</strong>
                  <small>{preset.includes("js") || ["Nuxt", "Astro", "Vite", "Express", "NestJS"].includes(preset) ? "Node.js" : "PHP"}</small>
                </button>
              ))}
            </div>
          </Panel>
          <Panel title="Popular CMS">
            <div className="cms-template-grid">
              {templates.map((template) => (
                <button
                  key={template.id}
                  className={`cms-card ${selected?.id === template.id ? "active" : ""}`}
                  onClick={() => selectTemplate(template)}
                >
                  <strong>{template.name}</strong>
                  <span>{template.category}</span>
                  <small>{template.description}</small>
                </button>
              ))}
            </div>
          </Panel>
          <Panel title="Installed CMS">
            <table className="data-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Domain</th>
                  <th>Folder</th>
                  <th>Database</th>
                  <th>Actions</th>
                </tr>
              </thead>
              <tbody>
                {state.cmsInstallations.map((item) => (
                  <tr key={item.id}>
                    <td><strong>{item.name}</strong><small>{new Date(item.installedAt).toLocaleString()}</small></td>
                    <td><StatusDot status={item.status === "installed" ? "running" : "stopped"} /> {item.domain}</td>
                    <td>{item.rootFolder}</td>
                    <td>{item.database || "None"}</td>
                    <td className="row-actions">
                      <Button variant="icon" icon={<Play size={15} />} onClick={() => void run(() => api.openHost(item.domain), { label: `Opening ${item.domain}...` })} />
                      <Button variant="icon" icon={<Folder size={15} />} onClick={() => void run(() => api.openPath(item.rootFolder), { label: `Opening ${item.rootFolder}...` })} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {state.cmsInstallations.length === 0 && <p className="muted">No CMS installations yet.</p>}
          </Panel>
        </section>
        <aside className="detail-rail">
          <Panel title="Installation">
            <div className="form-grid">
              <label>
                CMS
                <select value={selectedId} onChange={(event) => {
                  const template = templates.find((item) => item.id === event.target.value);
                  if (template) selectTemplate(template);
                }}>
                  {templates.map((template) => <option key={template.id} value={template.id}>{template.name}</option>)}
                </select>
              </label>
              <label>
                Domain
                <input value={domain} onChange={(event) => {
                  const value = event.target.value;
                  const previousName = sanitizeDatabaseName(domain.split(".")[0] ?? "");
                  setDomain(value);
                  if (databaseName === previousName) {
                    const nextName = sanitizeDatabaseName(value.split(".")[0] ?? "");
                    setDatabaseName(nextName);
                    if (databaseUser === `${previousName}_user`) setDatabaseUser(`${nextName}_user`);
                  }
                }} placeholder="cms.test" />
              </label>
              <label>
                Project Folder
                <input value={rootFolder} onChange={(event) => setRootFolder(event.target.value)} />
              </label>
              <button className="input-button" type="button" disabled={!rootFolder.trim()} onClick={() => void run(() => api.openPath(rootFolder), { label: `Opening ${rootFolder}...` })}>
                Open Folder <Folder size={15} />
              </button>
              <div className="form-grid two">
                <label>
                  PHP
                  <select value={phpVersion} onChange={(event) => setPhpVersion(event.target.value)}>
                    {state.phpVersions.map((item) => <option key={item.version}>{item.version}</option>)}
                  </select>
                </label>
                <label>
                  Web Server
                  <select value={webServer} onChange={(event) => setWebServer(event.target.value)} disabled>
                    <option>Apache</option>
                  </select>
                </label>
              </div>
              <div className="form-grid two">
                <label>
                  Database
                  <select value={databaseEngine} onChange={(event) => setDatabaseEngine(event.target.value as DatabaseInfo["engine"])} disabled={!selected?.requiresDatabase}>
                    <option>MySQL</option>
                    <option>MariaDB</option>
                    <option>PostgreSQL</option>
                  </select>
                </label>
                <label>
                  Source
                  <button className="input-button" type="button" disabled={selectedIsNode} onClick={() => selected && !selectedIsNode && void run(() => api.openUrl(selected.downloadUrl), { label: `Opening ${selected.name} package...` })}>
                    {selectedIsNode ? "Generated locally" : "Official package"} <ExternalLink size={15} />
                  </button>
                </label>
              </div>
              <label className="toggle-line">
                Create database
                <span className={`toggle ${createDatabase ? "on" : ""}`} onClick={() => selected?.requiresDatabase && setCreateDatabase((value) => !value)} />
              </label>
              {selected?.requiresDatabase && createDatabase && (
                <div className="form-grid two">
                  <label>
                    Database Name
                    <input value={databaseName} onChange={(event) => {
                      const previousUser = `${sanitizeDatabaseName(databaseName)}_user`;
                      const nextName = sanitizeDatabaseName(event.target.value);
                      setDatabaseName(nextName);
                      if (databaseUser === previousUser) setDatabaseUser(`${nextName}_user`);
                    }} />
                  </label>
                  <label>
                    Database User
                    <input value={databaseUser} onChange={(event) => setDatabaseUser(sanitizeDatabaseName(event.target.value))} />
                  </label>
                  <label>
                    Database Password
                    <input type="text" value={databasePassword} onChange={(event) => setDatabasePassword(event.target.value)} />
                  </label>
                  <label>
                    Password Tool
                    <button className="input-button" type="button" onClick={() => setDatabasePassword(generatePassword())}>Generate password <KeyRound size={15} /></button>
                  </label>
                </div>
              )}
              <label className="toggle-line">
                SSL
                <span className={`toggle ${ssl ? "on" : ""}`} onClick={() => setSsl((value) => !value)} />
              </label>
              <label className="toggle-line">
                Overwrite existing files
                <span className={`toggle ${overwrite ? "on" : ""}`} onClick={() => setOverwrite((value) => !value)} />
              </label>
            </div>
          </Panel>
          <Panel title="Install Flow">
            <div className="wizard-steps">
              {[
                ["Project", rootFolder.trim() ? "ready" : "waiting"],
                ["Database", selected?.requiresDatabase && createDatabase ? databaseName || "waiting" : "skip"],
                ["Services", servicesReady ? "ready" : "start needed"],
                ["SSL", ssl ? "enabled" : "off"],
                ["Install", selected ? "ready" : "waiting"]
              ].map(([name, value]) => <span key={name}><strong>{name}</strong><small>{value}</small></span>)}
            </div>
            <div className="kv form-kv detail-kv">
              <span>Download</span><strong>{selected?.name ?? "CMS package"}</strong>
              <span>Extract to</span><strong>{rootFolder}\\{selected?.documentRoot ?? "public"}</strong>
              <span>Host</span><strong>{domain}</strong>
              <span>Web Server</span><StatusDot status={servicesReady ? "running" : "stopped"} />
            </div>
            <div className="stack-buttons">
              <Button variant="primary" icon={<Download size={16} />} disabled={!selected} onClick={() => void install()}>
                Download and Install
              </Button>
              <Button icon={<Globe2 size={16} />} onClick={() => void run(() => api.syncHostsFile(), { label: "Synchronizing Windows hosts file..." })}>Sync Hosts File</Button>
              {lastInstallDomain && <Button icon={<ExternalLink size={16} />} onClick={() => {
                const installed = state.hosts.find((host) => host.domain === lastInstallDomain);
                void run(() => installed ? api.openHost(installed.id) : api.openUrl(`http://${lastInstallDomain}`), { label: `Opening ${lastInstallDomain}...` });
              }}>Open Installed Site</Button>}
            </div>
          </Panel>
          <Panel title=".env Presets">
            <div className="preset-grid compact-preset-grid">
              {["Laravel", "WordPress", "Next.js"].map((preset) => (
                <button key={preset} className={envPreset === preset ? "active" : ""} onClick={() => buildEnvPreset(preset)}>
                  <KeyRound size={15} />
                  <strong>{preset}</strong>
                  <small>.env</small>
                </button>
              ))}
            </div>
            <textarea className="config-editor env-editor" value={envPreview} onChange={(event) => setEnvPreview(event.target.value)} placeholder="APP_ENV=local" />
            <div className="stack-buttons">
              <Button icon={<KeyRound size={16} />} onClick={() => buildEnvPreset()}>Generate .env</Button>
              <Button variant="primary" icon={<Download size={16} />} onClick={() => void writeEnvFile()}>Write .env</Button>
              <Button variant="primary" icon={<PackagePlus size={16} />} onClick={() => void deployExistingProject()}>Deploy Existing Project</Button>
            </div>
            {projectChecks.length > 0 && (
              <div className="content-results compact-results">
                {projectChecks.map((check) => <button key={check}><strong>{check}</strong></button>)}
              </div>
            )}
          </Panel>
        </aside>
      </div>
    </div>
  );
}

function sanitizeDatabaseName(value: string) {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .replace(/_{2,}/g, "_");
}

function generatePassword() {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  const bytes = new Uint8Array(18);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => alphabet[byte % alphabet.length]).join("");
}
