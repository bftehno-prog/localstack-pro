import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  AppSnapshot,
  CertificateInfo,
  CmsInstallRequest,
  CmsTemplate,
  ConfigFile,
  DatabaseInfo,
  DatabaseDiagnosticReport,
  EnvironmentSnapshotInfo,
  FileEntry,
  FileSearchResult,
  HealthReport,
  HostDiagnosticReport,
  HostInfo,
  InstalledTool,
  LogFileTail,
  NodeScript,
  PhpVersion,
  PortInspection,
  ProjectInspection,
  ReleaseInfo,
  ResourceProcess,
  ServiceInfo,
  SitePreview,
  SslDiagnostic
} from "./types";

const now = new Date().toISOString();

const mockState: AppSnapshot = {
  appDataDir: "Tauri AppData",
  system: { appVersion: "1.0.0", os: "Windows 11 Pro 23H2", uptimeSeconds: 8040, cpu: 12, memoryGb: 2.1, diskGb: 127 },
  services: [
    service("apache", "Apache", "2.4.58", [80, 443], "C:\\LocalStack\\services\\apache\\bin\\httpd.exe"),
    service("nginx", "Nginx", "1.25.4", [8080, 8443], "C:\\LocalStack\\services\\nginx\\nginx.exe"),
    service("mysql", "MySQL", "8.0.36", [3306], "C:\\LocalStack\\services\\mysql\\bin\\mysqld.exe"),
    service("mariadb", "MariaDB", "10.11.6", [3307], "C:\\LocalStack\\services\\mariadb\\bin\\mariadbd.exe"),
    service("postgresql", "PostgreSQL", "15.3", [5432], "C:\\LocalStack\\services\\postgresql\\bin\\postgres.exe"),
    service("redis", "Redis", "7.2.4", [6379], "C:\\LocalStack\\services\\redis\\redis-server.exe"),
    service("mailpit", "Mailpit", "1.20.1", [1025, 8025], "C:\\LocalStack\\services\\mailpit\\mailpit.exe"),
    service("node-proxy", "Node.js Proxy", "20.11.1", [3000], "C:\\Program Files\\nodejs\\node.exe"),
    service("dns-helper", "DNS Helper", "1.0.0", [5353], "C:\\LocalStack\\services\\dns-helper\\dns-helper.exe"),
    service("mongodb", "MongoDB", "7.0", [27017], "C:\\LocalStack\\services\\mongodb\\bin\\mongod.exe"),
    service("memcached", "Memcached", "1.6", [11211], "C:\\LocalStack\\services\\memcached\\memcached.exe"),
    service("minio", "MinIO", "latest", [9000, 9001], "C:\\LocalStack\\services\\minio\\minio.exe"),
    service("elasticsearch", "Elasticsearch", "8.x", [9200], "C:\\LocalStack\\services\\elasticsearch\\bin\\elasticsearch.bat"),
    service("rabbitmq", "RabbitMQ", "3.x", [5672, 15672], "C:\\LocalStack\\services\\rabbitmq\\sbin\\rabbitmq-server.bat"),
    service("caddy", "Caddy", "2.x", [2019, 8081, 8444], "C:\\LocalStack\\services\\caddy\\caddy.exe")
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
    theme: "Light",
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
  return { id: name, name, description, engine, version, schemas: 4, user, password: "localstack", port, status, sizeMb, createdAt: now };
}

function log(level: "INFO" | "WARNING" | "ERROR" | "DEBUG", service: string, message: string) {
  return { id: crypto.randomUUID(), timestamp: now, level, service, host: "shop.test", processId: 1234, source: `${service.toLowerCase()}.log`, line: 88, message };
}

function mockFiles(path: string): FileEntry[] {
  const base = path.replace(/[\\/]+$/, "");
  return [
    { name: "public", path: `${base}\\public`, kind: "folder", size: 0, modified: now },
    { name: "index.php", path: `${base}\\index.php`, kind: "file", size: 68, modified: now },
    { name: ".env", path: `${base}\\.env`, kind: "file", size: 86, modified: now },
    { name: "package.json", path: `${base}\\package.json`, kind: "file", size: 128, modified: now }
  ];
}

function mockConfigFile(path: string): ConfigFile {
  return {
    path,
    content: "<?php\n// LocalStack Pro preview file\necho 'LocalStack Pro';\n",
    size: 58,
    modified: now,
    language: path.endsWith(".json") ? "JSON" : path.endsWith(".env") ? "Environment" : "PHP",
    readOnly: false,
    encoding: "utf-8"
  };
}

async function call<T>(command: string, args?: Record<string, unknown>, fallback?: T): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    if (fallback !== undefined && !("__TAURI_INTERNALS__" in window)) return fallback;
    throw error instanceof Error ? error : new Error(String(error));
  }
}

export const api = {
  getState: () => call<AppSnapshot>("get_app_state", undefined, mockState),
  startAll: () => call<AppSnapshot>("start_all"),
  stopAll: () => call<AppSnapshot>("stop_all"),
  restartAll: () => call<AppSnapshot>("restart_all"),
  startService: (serviceId: string) => call<AppSnapshot>("start_service", { serviceId }),
  startServiceProfile: (serviceIds: string[]) => call<AppSnapshot>("start_service_profile", { serviceIds }),
  stopService: (serviceId: string) => call<AppSnapshot>("stop_service", { serviceId }),
  restartService: (serviceId: string) => call<AppSnapshot>("restart_service", { serviceId }),
  saveService: (service: ServiceInfo) => call<AppSnapshot>("save_service", { service }),
  installServiceDependency: (serviceId: string) => call<AppSnapshot>("install_service_dependency", { serviceId }),
  installAllMissingDependencies: () => call<AppSnapshot>("install_all_missing_dependencies"),
  detectDependencies: () => call<AppSnapshot>("detect_dependencies"),
  runHealthCheck: () => call<HealthReport>("run_health_check"),
  repairEnvironment: () => call<HealthReport>("repair_environment"),
  saveHost: (host: HostInfo) => call<AppSnapshot>("save_host", { host }),
  deleteHost: (hostId: string) => call<AppSnapshot>("delete_host", { hostId }),
  duplicateHost: (hostId: string) => call<AppSnapshot>("duplicate_host", { hostId }),
  syncHostsFile: () => call<AppSnapshot>("sync_hosts_file"),
  diagnoseHost: (hostId: string) => call<HostDiagnosticReport>("diagnose_host", { hostId }),
  repairHost: (hostId: string) => call<HostDiagnosticReport>("repair_host", { hostId }),
  importHosts: (path: string) => call<AppSnapshot>("import_hosts", { path }),
  exportHosts: (path: string) => call<string>("export_hosts", { path }),
  savePhpVersion: (php: PhpVersion) => call<AppSnapshot>("save_php_version", { php }),
  installPhpVersion: (version: string) => call<AppSnapshot>("install_php_version", { version }),
  removePhpVersion: (version: string) => call<AppSnapshot>("remove_php_version", { version }),
  setDefaultPhp: (version: string) => call<AppSnapshot>("set_default_php", { version }),
  createDatabase: (database: DatabaseInfo) => call<AppSnapshot>("create_database", { database }),
  deleteDatabase: (databaseId: string) => call<AppSnapshot>("delete_database", { databaseId }),
  backupDatabase: (databaseId: string) => call<string>("backup_database", { databaseId }),
  importDatabaseSql: (databaseId: string, path = "") => call<string>("import_database_sql", { databaseId, path }),
  testDatabaseConnection: (databaseId: string) => call<DatabaseDiagnosticReport>("test_database_connection", { databaseId }),
  getCmsTemplates: () => call<CmsTemplate[]>("get_cms_templates", undefined, [
    {
      id: "nextjs",
      name: "Next.js",
      description: "React full-stack application with App Router and local dev server.",
      category: "Node.js",
      downloadUrl: "localstack://node/nextjs",
      documentRoot: ".",
      requiresDatabase: false,
      defaultDatabaseEngine: "MySQL"
    },
    {
      id: "node-express",
      name: "Node.js Express",
      description: "Minimal Express application with local API routes.",
      category: "Node.js",
      downloadUrl: "localstack://node/express",
      documentRoot: ".",
      requiresDatabase: false,
      defaultDatabaseEngine: "MySQL"
    },
    {
      id: "vite-react",
      name: "Vite React",
      description: "Fast React single-page application powered by Vite.",
      category: "Node.js",
      downloadUrl: "localstack://node/vite-react",
      documentRoot: ".",
      requiresDatabase: false,
      defaultDatabaseEngine: "MySQL"
    },
    {
      id: "wordpress",
      name: "WordPress",
      description: "Classic PHP CMS for blogs, shops and company sites.",
      category: "CMS",
      downloadUrl: "https://wordpress.org/latest.zip",
      documentRoot: "public",
      requiresDatabase: true,
      defaultDatabaseEngine: "MySQL"
    }
  ]),
  installCms: (request: CmsInstallRequest) => call<AppSnapshot>("install_cms", { request }),
  generateCertificate: (domain: string, sanDomains: string[]) => call<AppSnapshot>("generate_certificate", { domain, sanDomains }),
  trustCertificate: (certificateId: string) => call<AppSnapshot>("trust_certificate", { certificateId }),
  revokeCertificate: (certificateId: string) => call<AppSnapshot>("revoke_certificate", { certificateId }),
  saveCertificate: (certificate: CertificateInfo) => call<AppSnapshot>("save_certificate", { certificate }),
  exportCertificate: (certificateId: string, folder: string) => call<string>("export_certificate", { certificateId, folder }),
  clearLogs: () => call<AppSnapshot>("clear_logs"),
  exportLogs: (path: string) => call<string>("export_logs", { path }),
  tailLogFile: (source: string, lines = 200) => call<LogFileTail>("tail_log_file", { source, lines }),
  saveSettings: (settings: AppSettings) => call<AppSnapshot>("save_settings", { settings }),
  exportSettings: (path: string) => call<string>("export_settings", { path }),
  importSettings: (path: string) => call<AppSnapshot>("import_settings", { path }),
  resetSettings: () => call<AppSnapshot>("reset_settings"),
  createAppBackup: (path: string) => call<string>("create_app_backup", { path }),
  restoreAppBackup: (path: string) => call<AppSnapshot>("restore_app_backup", { path }),
  openCertificateStore: () => call<void>("open_certificate_store"),
  openDocumentation: () => call<void>("open_documentation"),
  openPath: (path: string) => call<void>("open_path", { path }),
  openUrl: (url: string) => call<void>("open_url", { url }),
  openTerminal: (path: string) => call<void>("open_terminal", { path }),
  openHost: (hostId: string) => call<AppSnapshot>("open_host", { hostId }),
  openDatabaseAdmin: (kind: "phpmyadmin" | "adminer") => call<void>("open_database_admin", { kind }),
  scanPorts: () => call<PortInspection[]>("scan_ports"),
  runProjectCommand: (path: string, commandKey: string) => call<string>("run_project_command", { path, commandKey }),
  cloneProjectRepository: (url: string, folder: string) => call<string>("clone_project_repository", { url, folder }),
  inspectProject: (path: string) => call<ProjectInspection>("inspect_project", { path }),
  generateEnvTemplate: (path: string, kind: string, database: string, user: string, password: string, domain: string) => call<string>("generate_env_template", { path, kind, database, user, password, domain }),
  previewHost: (hostId: string) => call<SitePreview>("preview_host", { hostId }),
  exportPortableHost: (hostId: string, target: string) => call<string>("export_portable_host", { hostId, target }),
  backupHost: (hostId: string, target: string) => call<string>("backup_host", { hostId, target }),
  restoreHostBackup: (path: string) => call<AppSnapshot>("restore_host_backup", { path }),
  checkLatestRelease: () => call<ReleaseInfo>("check_latest_release"),
  downloadLatestReleaseInstaller: () => call<string>("download_latest_release_installer"),
  installDownloadedUpdate: (path: string) => call<string>("install_downloaded_update", { path }),
  readConfigFile: (path: string) => call<ConfigFile>("read_config_file", { path }),
  saveConfigFile: (path: string, content: string) => call<string>("save_config_file", { path, content }),
  createDiagnosticBundle: (target: string) => call<string>("create_diagnostic_bundle", { target }),
  diagnoseSsl: (domain: string) => call<SslDiagnostic>("diagnose_ssl", { domain }),
  inspectInstalledTools: () => call<InstalledTool[]>("inspect_installed_tools"),
  listFiles: (path: string) => call<FileEntry[]>("list_files", { path }, mockFiles(path)),
  readFile: (path: string) => call<ConfigFile>("read_file", { path }, mockConfigFile(path)),
  readFileWithEncoding: (path: string, encoding: string) => call<ConfigFile>("read_file_with_encoding", { path, encoding }, { ...mockConfigFile(path), encoding }),
  writeFile: (path: string, content: string) => call<string>("write_file", { path, content }, path),
  writeFileWithEncoding: (path: string, content: string, encoding: string) => call<string>("write_file_with_encoding", { path, content, encoding }, path),
  createFile: (path: string) => call<string>("create_file", { path }, path),
  createFolder: (path: string) => call<string>("create_folder", { path }, path),
  deletePath: (path: string) => call<string>("delete_path", { path }, path),
  renamePath: (path: string, newName: string) => call<string>("rename_path", { path, newName }, path.replace(/[^\\/]+$/, newName)),
  duplicatePath: (path: string) => call<string>("duplicate_path", { path }, `${path}.copy`),
  copyPath: (source: string, target: string, overwrite: boolean) => call<string>("copy_path", { source, target, overwrite }, target),
  movePath: (source: string, target: string, overwrite: boolean) => call<string>("move_path", { source, target, overwrite }, target),
  chmodPath: (path: string, mode: string, readOnly: boolean) => call<string>("chmod_path", { path, mode, readOnly }, path),
  uploadFiles: (sources: string[], destination: string, overwrite: boolean) => call<string[]>("upload_files", { sources, destination, overwrite }, sources.map((source) => `${destination}\\${source.split(/[\\/]/).pop() ?? "file"}`)),
  extractArchiveTo: (path: string, destination: string) => call<string>("extract_archive_to", { path, destination }, destination),
  createArchive: (paths: string[], target: string) => call<string>("create_archive", { paths, target }, target),
  searchFileContents: (root: string, query: string, regexp: boolean, caseSensitive: boolean) => call<FileSearchResult[]>("search_file_contents", { root, query, regexp, caseSensitive }, []),
  listEnvironmentSnapshots: () => call<EnvironmentSnapshotInfo[]>("list_environment_snapshots"),
  createEnvironmentSnapshot: (name: string) => call<EnvironmentSnapshotInfo>("create_environment_snapshot", { name }),
  restoreEnvironmentSnapshot: (id: string) => call<AppSnapshot>("restore_environment_snapshot", { id }),
  listNodeScripts: (path: string) => call<NodeScript[]>("list_node_scripts", { path }),
  runNodeScript: (path: string, script: string) => call<string>("run_node_script", { path, script }),
  resourceMonitor: () => call<ResourceProcess[]>("resource_monitor"),
  killProcess: (pid: number) => call<string>("kill_process", { pid }),
  hideTrayPanel: () => call<void>("hide_tray_panel"),
  openMainPage: (page?: string) => call<void>("open_main_page", { page }),
  quit: () => call<void>("quit_app")
};
