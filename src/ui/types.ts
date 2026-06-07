export type ServiceStatus = "running" | "stopped" | "starting" | "error";
export type LogLevel = "INFO" | "WARNING" | "ERROR" | "DEBUG";
export type PageKey = "overview" | "hosts" | "host-editor" | "services" | "php" | "database" | "cms" | "ssl" | "logs" | "files" | "settings";
export type MainPageKey = Exclude<PageKey, "host-editor">;

export interface AppError {
  message: string;
  detail?: string;
}

export interface ServiceInfo {
  id: string;
  name: string;
  version: string;
  executablePath: string;
  configPath: string;
  logPath: string;
  ports: number[];
  status: ServiceStatus;
  pid?: number;
  uptimeSeconds: number;
  cpu: number;
  memoryMb: number;
  autostart: boolean;
  lastError?: string;
}

export interface HostInfo {
  id: string;
  domain: string;
  rootFolder: string;
  documentRoot: string;
  phpVersion: string;
  webServer: string;
  ssl: boolean;
  environment: string;
  httpPort: number;
  httpsPort: number;
  database: string;
  mailService: string;
  envVariables: Record<string, string>;
  rewriteRules: string;
  notes: string;
  status: ServiceStatus;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface PhpExtension {
  name: string;
  version: string;
  enabled: boolean;
  category: string;
}

export interface PhpVersion {
  version: string;
  label: string;
  status: "active" | "installed";
  default: boolean;
  cliPath: string;
  sapiMode: "Apache" | "FPM" | "CGI";
  extensions: PhpExtension[];
  ini: Record<string, string>;
  compatibility: "Full" | "Legacy" | "Unsupported";
}

export interface DatabaseInfo {
  id: string;
  name: string;
  description: string;
  engine: "MySQL" | "MariaDB" | "PostgreSQL";
  version: string;
  schemas: number;
  user: string;
  password: string;
  port: number;
  status: ServiceStatus;
  sizeMb: number;
  createdAt: string;
}

export interface CertificateInfo {
  id: string;
  domain: string;
  status: "Valid" | "Expiring Soon" | "Invalid";
  trusted: boolean;
  expiresAt: string;
  issuer: string;
  sanDomains: string[];
  autoRenew: boolean;
  certPath: string;
  keyPath: string;
  pemPath: string;
  fingerprint: string;
}

export interface CmsTemplate {
  id: string;
  name: string;
  description: string;
  category: string;
  downloadUrl: string;
  documentRoot: string;
  requiresDatabase: boolean;
  defaultDatabaseEngine: DatabaseInfo["engine"];
}

export interface CmsInstallInfo {
  id: string;
  templateId: string;
  name: string;
  domain: string;
  rootFolder: string;
  documentRoot: string;
  database?: string;
  installedAt: string;
  status: string;
}

export interface CmsInstallRequest {
  templateId: string;
  domain: string;
  rootFolder: string;
  phpVersion: string;
  webServer: string;
  ssl: boolean;
  databaseEngine: DatabaseInfo["engine"];
  createDatabase: boolean;
  databaseName?: string;
  databaseUser?: string;
  databasePassword?: string;
  overwrite: boolean;
}

export interface LogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  service: string;
  host?: string;
  processId?: number;
  source?: string;
  line?: number;
  message: string;
  detail?: string;
}

export interface AppSettings {
  language: string;
  preferredBrowser: string;
  minimizeToTray: boolean;
  closeToTray: boolean;
  launchOnStartup: boolean;
  showNotifications: boolean;
  playSound: boolean;
  checkUpdatesOnStartup: boolean;
  telemetry: boolean;
  uiDensity: string;
  theme: string;
  logLevel: string;
  maxLogFileSize: string;
  retainLogsDays: number;
  showTimestamps: boolean;
  projectsFolder: string;
  servicesFolder: string;
  backupsFolder: string;
  httpPortStart: number;
  httpPortEnd: number;
  proxyEnabled: boolean;
  backupRetentionDays: number;
}

export interface SystemInfo {
  appVersion: string;
  os: string;
  uptimeSeconds: number;
  cpu: number;
  memoryGb: number;
  diskGb: number;
}

export interface HealthCheck {
  id: string;
  title: string;
  severity: "ok" | "warning" | "error";
  message: string;
  detail?: string;
  action?: string;
}

export interface HealthReport {
  generatedAt: string;
  summary: string;
  ok: number;
  warnings: number;
  errors: number;
  checks: HealthCheck[];
}

export interface HostDiagnosticCheck {
  id: string;
  title: string;
  severity: "ok" | "warning" | "error";
  message: string;
  detail?: string;
  action?: string;
}

export interface HostDiagnosticReport {
  hostId: string;
  domain: string;
  generatedAt: string;
  summary: string;
  ok: number;
  warnings: number;
  errors: number;
  checks: HostDiagnosticCheck[];
}

export interface DatabaseDiagnosticCheck {
  id: string;
  title: string;
  severity: "ok" | "warning" | "error";
  message: string;
  detail?: string;
  action?: string;
}

export interface DatabaseDiagnosticReport {
  databaseId: string;
  database: string;
  generatedAt: string;
  summary: string;
  ok: number;
  warnings: number;
  errors: number;
  checks: DatabaseDiagnosticCheck[];
}

export interface PortInspection {
  port: number;
  status: string;
  service?: string;
  pid?: number;
  process?: string;
  action: string;
}

export interface ProjectInspection {
  kind: string;
  root: string;
  documentRoot: string;
  envTemplate: string;
  commands: string[];
  checks: Array<{ title: string; severity: string; message: string }>;
}

export interface SitePreview {
  url: string;
  status: string;
  responseTimeMs: number;
  contentType: string;
  redirectedTo?: string;
  message: string;
}

export interface ReleaseInfo {
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  url: string;
}

export interface ConfigFile {
  path: string;
  content: string;
}

export interface SslDiagnostic {
  domain: string;
  caTrusted: boolean;
  certExists: boolean;
  keyExists: boolean;
  sanCorrect: boolean;
  vhostConfigured: boolean;
  summary: string;
}

export interface InstalledTool {
  id: string;
  name: string;
  command: string;
  path?: string;
  version?: string;
  status: string;
}

export interface FileEntry {
  name: string;
  path: string;
  kind: "file" | "folder";
  size: number;
  modified?: string;
}

export interface EnvironmentSnapshotInfo {
  id: string;
  name: string;
  path: string;
  createdAt: string;
  hosts: number;
  services: number;
  databases: number;
}

export interface ResourceProcess {
  pid: number;
  name: string;
  cpu: number;
  memoryMb: number;
  command: string;
}

export interface NodeScript {
  name: string;
  command: string;
}

export interface LogFileTail {
  source: string;
  path: string;
  generatedAt: string;
  lines: string[];
}

export type AppRunResult = AppSnapshot | HealthReport | HostDiagnosticReport | DatabaseDiagnosticReport | LogFileTail | CmsTemplate[] | PortInspection[] | ProjectInspection | SitePreview | ReleaseInfo | ConfigFile | SslDiagnostic | InstalledTool[] | FileEntry[] | EnvironmentSnapshotInfo[] | EnvironmentSnapshotInfo | ResourceProcess[] | NodeScript[] | string | void;

export type AppRun = (
  action: () => Promise<AppRunResult>,
  options?: { silent?: boolean; label?: string; successLabel?: string; serial?: boolean }
) => Promise<AppRunResult>;

export interface OperationEntry {
  id: string;
  label: string;
  status: "running" | "success" | "error";
  startedAt: string;
  finishedAt?: string;
  durationMs?: number;
  message?: string;
}

export interface AppSnapshot {
  services: ServiceInfo[];
  hosts: HostInfo[];
  phpVersions: PhpVersion[];
  databases: DatabaseInfo[];
  certificates: CertificateInfo[];
  cmsInstallations: CmsInstallInfo[];
  logs: LogEntry[];
  settings: AppSettings;
  system: SystemInfo;
  appDataDir: string;
}
