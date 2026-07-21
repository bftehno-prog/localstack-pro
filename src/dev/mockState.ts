import type {
  AppSnapshot,
  ConfigFile,
  DatabaseDiagnosticReport,
  DatabaseInfo,
  EnvironmentSnapshotInfo,
  FileEntry,
  HealthReport,
  HostDiagnosticReport,
  HostInfo,
  LogFileTail,
  ProjectInspection,
  ReleaseInfo,
  ServiceInfo,
  SitePreview,
  SslDiagnostic
} from "../ui/types";

function nowIso() {
  return new Date().toISOString();
}

export function createMockState(): AppSnapshot {
  return {
    appDataDir: "Tauri AppData",
    system: { appVersion: "1.0.4", os: "Windows 11 Pro 23H2", uptimeSeconds: 8040, cpu: 12, memoryGb: 2.1, diskGb: 127 },
    services: [
      service("apache", "Apache", "2.4.58", [80, 443], "C:\\LocalStack\\services\\apache\\bin\\httpd.exe"),
      service("nginx", "Nginx", "1.25.4", [8080, 8443], "C:\\LocalStack\\services\\nginx\\nginx.exe"),
      service("mysql", "MySQL", "8.0.36", [3306], "C:\\LocalStack\\services\\mysql\\bin\\mysqld.exe"),
      service("mariadb", "MariaDB", "10.11.6", [3307], "C:\\LocalStack\\services\\mariadb\\bin\\mariadbd.exe"),
      service("postgresql", "PostgreSQL", "15.3", [5432], "C:\\LocalStack\\services\\postgresql\\bin\\postgres.exe"),
      service("redis", "Redis", "7.2.4", [6379], "C:\\LocalStack\\services\\redis\\redis-server.exe"),
      service("mailpit", "Mailpit", "1.20.1", [1025, 8025], "C:\\LocalStack\\services\\mailpit\\mailpit.exe"),
      service("node-proxy", "Node.js Proxy", "20.11.1", [3000], "C:\\Program Files\\nodejs\\node.exe"),
      service("dns-helper", "DNS Helper", "1.0.4", [5353], "C:\\LocalStack\\services\\dns-helper\\dns-helper.exe")
    ],
    hosts: [
      host("shop.test", "C:\\Projects\\shop", "8.1.23", true, "Production", "running", ["ecommerce", "main"]),
      host("acme.test", "C:\\Projects\\acme", "8.2.10", true, "Production", "running", ["corporate", "primary"]),
      host("blog.test", "C:\\Projects\\blog", "8.3.6", true, "Staging", "running", ["blog", "headless"]),
      host("api.test", "C:\\Projects\\api", "8.2.10", false, "Development", "stopped", ["api"]),
      host("crm.test", "C:\\Projects\\crm", "7.4.33", true, "Production", "running", ["internal"]),
      host("legacy.test", "C:\\Projects\\legacy", "7.3.31", false, "Development", "stopped", ["legacy"])
    ],
    phpVersions: ["8.1.23", "8.2.10", "8.3.6", "7.4.33", "7.3.31", "7.2.34"].map((version, index) => ({
      version,
      label: version.slice(0, 3),
      status: index === 0 ? "active" : "installed",
      default: index === 0,
      cliPath: `C:\\tools\\php\\${version}\\php.exe`,
      sapiMode: index < 3 ? "FPM" : "Apache",
      compatibility: index < 3 ? "Full" : "Legacy",
      extensions: ["xdebug", "intl", "gd", "imagick", "opcache", "pdo_mysql", "redis", "soap", "zip"].map((name, extIndex) => ({
        name,
        version: name === "xdebug" ? "3.2.1" : version,
        enabled: extIndex !== 3 || index === 0,
        category: extIndex < 2 ? "Debug" : extIndex < 5 ? "Core" : "Database"
      })),
      ini: {
        memory_limit: "512M",
        upload_max_filesize: "64M",
        post_max_size: "64M",
        max_execution_time: "120",
        max_input_time: "120",
        display_errors: "On",
        display_startup_errors: "On",
        log_errors: "On",
        error_reporting: "E_ALL",
        date_timezone: "UTC",
        "xdebug.mode": "develop,debug",
        "opcache.enable": "On",
        "opcache.memory_consumption": "128"
      }
    })),
    databases: [
      db("shop", "Main e-commerce DB", "MySQL", "8.0.36", "shop_user", 3306, "running", 128.6),
      db("blog", "Blog application DB", "MySQL", "8.0.36", "blog_user", 3306, "running", 64.2),
      db("cms", "CMS application DB", "MariaDB", "10.6.18", "cms_user", 3307, "running", 93.1),
      db("test", "Testing & development", "MySQL", "8.0.36", "test_user", 3306, "stopped", 12.4),
      db("analytics", "Analytics reporting", "PostgreSQL", "15.3", "analytics_user", 5432, "running", 256.7)
    ],
    certificates: ["shop.test", "api.test", "crm.test", "blog.test", "legacy.test", "internal.test"].map((domain, index) => ({
      id: domain,
      domain,
      status: index === 3 ? "Expiring Soon" : "Valid",
      trusted: index !== 4,
      expiresAt: new Date(Date.now() + (80 + index * 8) * 86400000).toISOString(),
      issuer: "LocalStack CA",
      sanDomains: [domain, `www.${domain}`],
      autoRenew: index !== 4,
      certPath: `C:\\LocalStack\\certs\\${domain}.crt`,
      keyPath: `C:\\LocalStack\\keys\\${domain}.key`,
      pemPath: `C:\\LocalStack\\certs\\${domain}.pem`,
      fingerprint: "A3:6F:2B:9C:8D:33:1E:4A:7F:1C:6D:2E:9B:7E:77:2F:6C:4E:5F:8B:2A:09:3D:8F:5A:1B:2C:7D:9E:3F"
    })),
    cmsInstallations: [],
    logs: [
      log("INFO", "Apache", "Apache/2.4.58 (Win64) PHP/8.1.23 started"),
      log("INFO", "MySQL", "MySQL 8.0.36 Community Server started on port 3306"),
      log("DEBUG", "Nginx", "127.0.0.1 - GET /assets/app.css HTTP/1.1 304"),
      log("WARNING", "PHP", "Undefined variable $user in /var/www/html/login.php on line 42"),
      log("ERROR", "PHP", "Uncaught Exception: Invalid credentials in /var/www/html/auth.php:88"),
      log("INFO", "Mailpit", "Email queued to test@example.com")
    ],
    settings: {
      language: "English (US)",
      preferredBrowser: "Default System Browser",
      minimizeToTray: true,
      closeToTray: true,
      launchOnStartup: false,
      showNotifications: true,
      playSound: false,
      checkUpdatesOnStartup: true,
      telemetry: false,
      uiDensity: "Comfortable",
      theme: "Wet Asphalt",
      logLevel: "Information",
      maxLogFileSize: "50 MB",
      retainLogsDays: 30,
      showTimestamps: true,
      projectsFolder: "C:\\Projects",
      servicesFolder: "C:\\LocalStack\\services",
      backupsFolder: "C:\\LocalStack\\backups",
      httpPortStart: 80,
      httpPortEnd: 8999,
      proxyEnabled: false,
      backupRetentionDays: 30
    }
  };
}

function service(id: string, name: string, version: string, ports: number[], executablePath: string): ServiceInfo {
  return {
    id,
    name,
    version,
    executablePath,
    configPath: executablePath.replace(/\\[^\\]+$/, "\\conf\\service.conf"),
    logPath: executablePath.replace(/\\[^\\]+$/, "\\logs\\service.log"),
    ports,
    status: "stopped",
    uptimeSeconds: 0,
    cpu: 0,
    memoryMb: 0,
    autostart: true
  };
}

function host(domain: string, rootFolder: string, phpVersion: string, ssl: boolean, environment: string, status: ServiceInfo["status"], tags: string[]): HostInfo {
  const now = nowIso();
  return {
    id: domain,
    domain,
    rootFolder,
    documentRoot: "public",
    phpVersion,
    webServer: "Apache",
    ssl,
    environment,
    httpPort: 80,
    httpsPort: 443,
    database: domain.split(".")[0],
    mailService: "Mailpit",
    envVariables: { APP_ENV: "local", APP_DEBUG: "true", APP_URL: `${ssl ? "https" : "http"}://${domain}` },
    rewriteRules: "",
    notes: `Primary ${environment.toLowerCase()} environment.`,
    status,
    tags,
    createdAt: now,
    updatedAt: now
  };
}

function db(name: string, description: string, engine: DatabaseInfo["engine"], version: string, user: string, port: number, status: ServiceInfo["status"], sizeMb: number): DatabaseInfo {
  return { id: name, name, description, engine, version, schemas: 4, user, password: "localstack", port, status, sizeMb, createdAt: nowIso() };
}

function log(level: "INFO" | "WARNING" | "ERROR" | "DEBUG", service: string, message: string) {
  return { id: crypto.randomUUID(), timestamp: nowIso(), level, service, host: "shop.test", processId: 1234, source: `${service.toLowerCase()}.log`, line: 88, message };
}

export function mockFiles(path: string): FileEntry[] {
  const base = path.replace(/[\\/]+$/, "");
  const now = nowIso();
  return [
    { name: "public", path: `${base}\\public`, kind: "folder", size: 0, modified: now },
    { name: "index.php", path: `${base}\\index.php`, kind: "file", size: 68, modified: now },
    { name: ".env", path: `${base}\\.env`, kind: "file", size: 86, modified: now },
    { name: "package.json", path: `${base}\\package.json`, kind: "file", size: 128, modified: now }
  ];
}

export function mockConfigFile(path: string): ConfigFile {
  return {
    path,
    content: "<?php\n// LocalStack Pro preview file\necho 'LocalStack Pro';\n",
    size: 58,
    modified: nowIso(),
    language: path.endsWith(".json") ? "JSON" : path.endsWith(".env") ? "Environment" : "PHP",
    readOnly: false,
    encoding: "utf-8"
  };
}

export function mockHealthReport(summary = "Browser preview is running with mock data."): HealthReport {
  return { generatedAt: nowIso(), summary, ok: 1, warnings: 0, errors: 0, checks: [{ id: "preview", title: "Browser preview", severity: "ok", message: summary }] };
}

export function mockHostReport(hostId: string): HostDiagnosticReport {
  return { hostId, domain: hostId, generatedAt: nowIso(), summary: "Browser preview host diagnostics are mocked.", ok: 1, warnings: 0, errors: 0, checks: [] };
}

export function mockDatabaseReport(databaseId: string): DatabaseDiagnosticReport {
  return { databaseId, database: databaseId, generatedAt: nowIso(), summary: "Browser preview database diagnostics are mocked.", ok: 1, warnings: 0, errors: 0, checks: [] };
}

export function mockLogTail(source: string): LogFileTail {
  return { source, path: source, generatedAt: nowIso(), lines: ["Browser preview log line"] };
}

export function mockProjectInspection(path: string): ProjectInspection {
  return { kind: "Preview", root: path, documentRoot: "public", envTemplate: "", commands: [], checks: [] };
}

export function mockSitePreview(hostId: string): SitePreview {
  return { url: `http://${hostId}`, status: "preview", responseTimeMs: 0, contentType: "text/html", message: "Browser preview only." };
}

export function mockReleaseInfo(): ReleaseInfo {
  return { currentVersion: "1.0.4", latestVersion: "1.0.4", updateAvailable: false, url: "" };
}

export function mockSslDiagnostic(domain: string): SslDiagnostic {
  return { domain, caTrusted: true, certExists: true, keyExists: true, sanCorrect: true, vhostConfigured: true, summary: "Browser preview SSL diagnostics are mocked." };
}

export function mockEnvironmentSnapshot(name: string): EnvironmentSnapshotInfo {
  return { id: name || "preview", name: name || "Preview", path: "browser-preview", createdAt: nowIso(), hosts: 0, services: 0, databases: 0 };
}
