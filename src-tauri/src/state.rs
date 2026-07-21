use chrono::Utc;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
use sysinfo::{Disks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

static BUNDLE_EXTRACTION_LOCK: Mutex<()> = Mutex::new(());
static SYSTEM_METRICS: Mutex<Option<System>> = Mutex::new(None);
static DISK_METRICS: Mutex<Option<(Instant, f64)>> = Mutex::new(None);

pub type AppResult<T> = Result<T, String>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundledServicesManifest {
    #[serde(default)]
    packages: Vec<BundledPackageEntry>,
    #[serde(default)]
    services: Vec<BundledServiceEntry>,
    #[serde(default)]
    php_versions: Vec<BundledPhpEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundledPackageEntry {
    archive: String,
    extract_to: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundledServiceEntry {
    id: String,
    version: String,
    executable: String,
    #[serde(default)]
    arguments: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BundledPhpEntry {
    version: String,
    label: String,
    executable: String,
    #[serde(default)]
    default: bool,
    #[serde(default)]
    sapi_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub services: Vec<ServiceInfo>,
    pub hosts: Vec<HostInfo>,
    pub php_versions: Vec<PhpVersion>,
    pub databases: Vec<DatabaseInfo>,
    pub certificates: Vec<CertificateInfo>,
    #[serde(default)]
    pub cms_installations: Vec<CmsInstallInfo>,
    pub logs: Vec<LogEntry>,
    pub settings: AppSettings,
    pub system: SystemInfo,
    pub app_data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Running,
    Stopped,
    Starting,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub executable_path: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub config_path: String,
    pub log_path: String,
    pub ports: Vec<u16>,
    pub status: ServiceStatus,
    pub pid: Option<u32>,
    pub uptime_seconds: u64,
    pub cpu: f32,
    pub memory_mb: u64,
    pub autostart: bool,
    pub last_error: Option<String>,
    #[serde(default)]
    pub started_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub id: String,
    pub domain: String,
    pub root_folder: String,
    pub document_root: String,
    pub php_version: String,
    pub web_server: String,
    pub ssl: bool,
    pub environment: String,
    pub http_port: u16,
    pub https_port: u16,
    pub database: String,
    pub mail_service: String,
    pub env_variables: HashMap<String, String>,
    pub rewrite_rules: String,
    pub notes: String,
    pub status: ServiceStatus,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpExtension {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhpVersion {
    pub version: String,
    pub label: String,
    pub status: String,
    pub default: bool,
    pub cli_path: String,
    pub sapi_mode: String,
    pub extensions: Vec<PhpExtension>,
    pub ini: HashMap<String, String>,
    pub compatibility: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub engine: String,
    pub version: String,
    pub schemas: u32,
    pub user: String,
    pub password: String,
    pub port: u16,
    pub status: ServiceStatus,
    pub size_mb: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateInfo {
    pub id: String,
    pub domain: String,
    pub status: String,
    pub trusted: bool,
    pub expires_at: String,
    pub issuer: String,
    pub san_domains: Vec<String>,
    pub auto_renew: bool,
    pub cert_path: String,
    pub key_path: String,
    pub pem_path: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmsInstallInfo {
    pub id: String,
    pub template_id: String,
    pub name: String,
    pub domain: String,
    pub root_folder: String,
    pub document_root: String,
    pub database: Option<String>,
    pub installed_at: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: LogLevel,
    pub service: String,
    pub host: Option<String>,
    pub process_id: Option<u32>,
    pub source: Option<String>,
    pub line: Option<u32>,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    pub preferred_browser: String,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub launch_on_startup: bool,
    pub show_notifications: bool,
    pub play_sound: bool,
    pub check_updates_on_startup: bool,
    pub telemetry: bool,
    pub ui_density: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    pub log_level: String,
    pub max_log_file_size: String,
    pub retain_logs_days: u32,
    pub show_timestamps: bool,
    pub projects_folder: String,
    pub services_folder: String,
    pub backups_folder: String,
    pub http_port_start: u16,
    pub http_port_end: u16,
    pub proxy_enabled: bool,
    pub backup_retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub app_version: String,
    pub os: String,
    pub uptime_seconds: u64,
    pub cpu: f32,
    pub memory_gb: f64,
    pub disk_gb: f64,
}

pub struct Store {
    pub dir: PathBuf,
    state_file: PathBuf,
}

impl Store {
    pub fn new() -> AppResult<Self> {
        let dirs = ProjectDirs::from("com", "LocalStack", "LocalStack Pro")
            .ok_or_else(|| "Cannot resolve Windows AppData directory.".to_string())?;
        let dir = dirs.data_dir().to_path_buf();
        fs::create_dir_all(dir.join("configs"))
            .map_err(|err| format!("Cannot create configs folder: {err}"))?;
        fs::create_dir_all(dir.join("hosts"))
            .map_err(|err| format!("Cannot create hosts folder: {err}"))?;
        fs::create_dir_all(dir.join("logs"))
            .map_err(|err| format!("Cannot create logs folder: {err}"))?;
        fs::create_dir_all(dir.join("backups"))
            .map_err(|err| format!("Cannot create backups folder: {err}"))?;
        fs::create_dir_all(dir.join("certs"))
            .map_err(|err| format!("Cannot create certs folder: {err}"))?;
        fs::create_dir_all(dir.join("keys"))
            .map_err(|err| format!("Cannot create keys folder: {err}"))?;
        Ok(Self {
            state_file: dir.join("state.json"),
            dir,
        })
    }

    pub fn load(&self) -> AppResult<AppSnapshot> {
        if self.state_file.exists() {
            let mut snapshot = self.load_static()?;
            snapshot = self.refresh_runtime(snapshot);
            Ok(snapshot)
        } else {
            let snapshot = self.default_snapshot();
            self.ensure_host_files(&snapshot);
            self.ensure_service_files(&snapshot);
            self.save(&snapshot)?;
            Ok(snapshot)
        }
    }

    pub fn load_static(&self) -> AppResult<AppSnapshot> {
        if self.state_file.exists() {
            let text = fs::read_to_string(&self.state_file)
                .map_err(|err| format!("Cannot read application state: {err}"))?;
            let mut snapshot: AppSnapshot = serde_json::from_str(&text)
                .map_err(|err| format!("Cannot parse application state: {err}"))?;
            snapshot.app_data_dir = self.dir.display().to_string();
            let needs_save = snapshot.services.iter().any(|service| {
                service.id == "mysql" && service.arguments.iter().any(|arg| arg == "--console")
            });
            self.ensure_defaults(&mut snapshot);
            let changed = (!self.bundled_snapshot_ready(&snapshot)
                && self.apply_bundled_services(&mut snapshot, false))
                || needs_save;
            if changed {
                let _ = self.save(&snapshot);
            }
            Ok(snapshot)
        } else {
            let mut snapshot = self.default_snapshot();
            self.apply_bundled_services(&mut snapshot, false);
            self.migrate_service_paths(&mut snapshot);
            self.migrate_php_paths(&mut snapshot);
            Ok(snapshot)
        }
    }

    fn ensure_defaults(&self, snapshot: &mut AppSnapshot) {
        let now = Utc::now().to_rfc3339();
        let base = self.dir.display().to_string();
        let services_dir = format!("{base}\\services");
        for service in extended_default_services(&services_dir) {
            if !snapshot
                .services
                .iter()
                .any(|existing| existing.id == service.id)
            {
                snapshot.services.push(service);
            }
        }
        if snapshot.hosts.is_empty() {
            let mut env = HashMap::new();
            env.insert("APP_ENV".to_string(), "local".to_string());
            env.insert("APP_DEBUG".to_string(), "true".to_string());
            snapshot.hosts.push(host(HostSeed {
                domain: "shop.test",
                root: "C:\\Projects\\shop",
                php_version: "8.1.23",
                ssl: true,
                environment: "Production",
                tags: vec!["ecommerce", "main"],
                env_variables: env,
                now: &now,
            }));
        }
        if snapshot.php_versions.is_empty() {
            let mut ini = HashMap::new();
            for (key, value) in default_ini_pairs() {
                ini.insert(key.to_string(), value.to_string());
            }
            snapshot.php_versions.push(php("8.1.23", true, ini));
        }
        if snapshot.databases.is_empty() {
            snapshot.databases.push(database(DatabaseSeed {
                name: "shop",
                description: "Main e-commerce DB",
                engine: "MySQL",
                version: "8.0.36",
                user: "shop_user",
                port: 3306,
                size_mb: 128.6,
                now: &now,
            }));
        }
        if snapshot.certificates.is_empty() {
            snapshot.certificates = snapshot
                .hosts
                .iter()
                .filter(|host| host.ssl)
                .map(|host| certificate_for(&base, &host.domain))
                .collect();
            if snapshot.certificates.is_empty() {
                snapshot
                    .certificates
                    .push(certificate_for(&base, "shop.test"));
            }
        }
        if snapshot.settings.services_folder.trim().is_empty() {
            snapshot.settings.services_folder = format!("{base}\\services");
        }
        if snapshot.settings.backups_folder.trim().is_empty() {
            snapshot.settings.backups_folder = format!("{base}\\backups");
        }
        if snapshot.settings.theme.trim().is_empty() {
            snapshot.settings.theme = default_theme();
        }
        if let Some(dns_helper) = snapshot
            .services
            .iter_mut()
            .find(|service| service.id == "dns-helper")
        {
            dns_helper.autostart = false;
            if dns_helper.status == ServiceStatus::Running && dns_helper.pid.is_none() {
                dns_helper.status = ServiceStatus::Stopped;
            }
        }
        for service in &mut snapshot.services {
            if service.id == "mysql" {
                service.arguments.retain(|arg| arg != "--console");
            }
        }
        self.ensure_cms_hosts(snapshot);
    }

    fn bundled_snapshot_ready(&self, snapshot: &AppSnapshot) -> bool {
        if !self
            .dir
            .join("services")
            .join(".bundled-services-ready")
            .is_file()
        {
            return false;
        }
        let services_root = self.dir.join("services");
        let required = [
            "apache",
            "nginx",
            "mysql",
            "mariadb",
            "postgresql",
            "redis",
            "mailpit",
            "node-proxy",
        ];
        required.iter().all(|id| {
            snapshot
                .services
                .iter()
                .find(|service| service.id == *id)
                .map(|service| {
                    let path = Path::new(&service.executable_path);
                    path.is_file() && path.starts_with(&services_root)
                })
                .unwrap_or(false)
        })
    }

    fn apply_bundled_services(&self, snapshot: &mut AppSnapshot, extract_missing: bool) -> bool {
        let Some(root) = bundled_services_root() else {
            return false;
        };
        let manifest_path = root.join("manifest.json");
        let Ok(text) = fs::read_to_string(&manifest_path) else {
            return false;
        };
        let Ok(manifest) = serde_json::from_str::<BundledServicesManifest>(&text) else {
            return false;
        };
        let resolved_root = self
            .resolve_bundled_services_root(&root, &manifest, extract_missing)
            .unwrap_or_else(|| root.clone());
        let mut changed = false;
        for bundled in manifest.services {
            let executable = resolved_root.join(normalize_relative_path(&bundled.executable));
            if !executable.is_file() {
                continue;
            }
            let Some(service) = snapshot
                .services
                .iter_mut()
                .find(|service| service.id == bundled.id)
            else {
                continue;
            };
            let executable_path = executable.display().to_string();
            if service.version != bundled.version || service.executable_path != executable_path {
                changed = true;
            }
            service.version = bundled.version;
            service.executable_path = executable_path;
            service.arguments = if bundled.arguments.is_empty() {
                default_service_arguments(&service.id, &executable)
            } else {
                bundled.arguments.clone()
            };
            let service_root = executable
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.join(&service.id));
            service.config_path = service_root
                .join("conf")
                .join("service.conf")
                .display()
                .to_string();
            service.log_path = self
                .dir
                .join("logs")
                .join(format!("{}.log", service.id))
                .display()
                .to_string();
            service.last_error = None;
            if service.status == ServiceStatus::Error {
                service.status = ServiceStatus::Stopped;
            }
        }
        if !manifest.php_versions.is_empty() {
            let mut existing_default = false;
            for bundled in manifest.php_versions {
                let executable = resolved_root.join(normalize_relative_path(&bundled.executable));
                if !executable.is_file() {
                    continue;
                }
                let mut ini = HashMap::new();
                for (key, value) in default_ini_pairs() {
                    ini.insert(key.to_string(), value.to_string());
                }
                let mut php_version = php(&bundled.version, bundled.default, ini);
                php_version.label = bundled.label;
                php_version.cli_path = executable.display().to_string();
                php_version.sapi_mode = if bundled.sapi_mode.trim().is_empty() {
                    "CGI".to_string()
                } else {
                    bundled.sapi_mode
                };
                php_version.status = "installed".to_string();
                if bundled.default && !existing_default {
                    existing_default = true;
                    php_version.default = true;
                    php_version.status = "active".to_string();
                }
                if let Some(existing) = snapshot
                    .php_versions
                    .iter_mut()
                    .find(|php| php.version == php_version.version)
                {
                    if existing.cli_path != php_version.cli_path
                        || existing.default != php_version.default
                        || existing.status != php_version.status
                    {
                        changed = true;
                    }
                    *existing = php_version;
                } else {
                    changed = true;
                    snapshot.php_versions.push(php_version);
                }
            }
            if existing_default {
                let mut default_seen = false;
                for php in &mut snapshot.php_versions {
                    if php.default && !default_seen {
                        default_seen = true;
                    } else {
                        php.default = false;
                    }
                }
            }
        }
        changed
    }

    fn resolve_bundled_services_root(
        &self,
        root: &Path,
        manifest: &BundledServicesManifest,
        extract_missing: bool,
    ) -> Option<PathBuf> {
        if manifest.services.iter().all(|service| {
            root.join(normalize_relative_path(&service.executable))
                .is_file()
        }) {
            return Some(root.to_path_buf());
        }
        let target = self.dir.join("services");
        let marker = target.join(".bundled-services-ready");
        let already_extracted = manifest.services.iter().all(|service| {
            target
                .join(normalize_relative_path(&service.executable))
                .is_file()
        });
        if already_extracted {
            let _ = fs::write(&marker, Utc::now().to_rfc3339());
            return Some(target);
        }
        if !extract_missing {
            return None;
        }
        let Ok(_guard) = BUNDLE_EXTRACTION_LOCK.lock() else {
            return None;
        };
        let already_extracted = manifest.services.iter().all(|service| {
            target
                .join(normalize_relative_path(&service.executable))
                .is_file()
        });
        if already_extracted {
            let _ = fs::write(&marker, Utc::now().to_rfc3339());
            return Some(target);
        }
        let _ = fs::create_dir_all(&target);
        let mut extracted_any = false;
        for package in &manifest.packages {
            let archive = root.join(normalize_relative_path(&package.archive));
            if !archive.is_file() {
                continue;
            }
            let destination = target.join(normalize_relative_path(&package.extract_to));
            let _ = fs::create_dir_all(&destination);
            if expand_zip_archive(&archive, &destination).is_ok() {
                extracted_any = true;
            }
        }
        let ready = manifest.services.iter().all(|service| {
            target
                .join(normalize_relative_path(&service.executable))
                .is_file()
        });
        if ready {
            let _ = fs::write(&marker, Utc::now().to_rfc3339());
            Some(target)
        } else if extracted_any {
            Some(target)
        } else {
            None
        }
    }

    fn ensure_cms_hosts(&self, snapshot: &mut AppSnapshot) {
        let now = Utc::now().to_rfc3339();
        let default_php = snapshot
            .php_versions
            .iter()
            .find(|php| php.default)
            .or_else(|| snapshot.php_versions.first())
            .map(|php| php.version.clone())
            .unwrap_or_else(|| "8.3".to_string());
        for install in snapshot.cms_installations.clone() {
            if snapshot
                .hosts
                .iter()
                .any(|host| host.domain.eq_ignore_ascii_case(&install.domain))
            {
                continue;
            }
            let mut env = HashMap::new();
            env.insert("APP_ENV".to_string(), "local".to_string());
            env.insert("APP_DEBUG".to_string(), "true".to_string());
            env.insert("APP_URL".to_string(), format!("http://{}", install.domain));
            if let Some(database) = &install.database {
                env.insert("DB_DATABASE".to_string(), database.clone());
                env.insert("DB_USERNAME".to_string(), format!("{}_user", database));
            }
            snapshot.hosts.push(HostInfo {
                id: install.domain.clone(),
                domain: install.domain.clone(),
                root_folder: install.root_folder.clone(),
                document_root: install.document_root.clone(),
                php_version: default_php.clone(),
                web_server: "Apache".to_string(),
                ssl: false,
                environment: "Development".to_string(),
                http_port: 80,
                https_port: 443,
                database: install.database.clone().unwrap_or_default(),
                mail_service: "Mailpit".to_string(),
                env_variables: env,
                rewrite_rules: String::new(),
                notes: format!("{} restored from CMS installation.", install.name),
                status: ServiceStatus::Stopped,
                tags: vec!["cms".to_string(), install.template_id.clone()],
                created_at: install.installed_at.clone(),
                updated_at: now.clone(),
            });
        }
    }

    fn migrate_service_paths(&self, snapshot: &mut AppSnapshot) {
        for service in &mut snapshot.services {
            if Path::new(&service.executable_path).exists() {
                let clean = clean_windows_path(&service.executable_path);
                if clean != service.executable_path {
                    service.executable_path = clean;
                }
                continue;
            }
            let Some(path) = fast_service_executable(&service.id) else {
                continue;
            };
            service.executable_path = path.display().to_string();
            service.arguments = default_service_arguments(&service.id, &path);
            let root = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.dir.join("services").join(&service.id));
            service.config_path = root.join("conf").join("service.conf").display().to_string();
            service.log_path = root.join("logs").join("service.log").display().to_string();
            service.last_error = None;
            if service.status == ServiceStatus::Error {
                service.status = ServiceStatus::Stopped;
            }
        }
    }

    fn migrate_php_paths(&self, snapshot: &mut AppSnapshot) {
        let Some(detected_cli) = fast_php_executable("php.exe") else {
            return;
        };
        let detected_version = detected_cli
            .components()
            .filter_map(|part| part.as_os_str().to_str())
            .find(|part| {
                part.chars().next().is_some_and(|ch| ch.is_ascii_digit()) && part.contains('.')
            })
            .unwrap_or("detected")
            .to_string();
        if snapshot
            .php_versions
            .iter()
            .any(|php| php.cli_path == detected_cli.display().to_string())
        {
            return;
        }
        if let Some(default_php) = snapshot.php_versions.iter_mut().find(|php| php.default) {
            if !Path::new(&default_php.cli_path).exists() {
                default_php.version = detected_version.clone();
                default_php.label = detected_version
                    .split('.')
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(".");
                default_php.cli_path = detected_cli.display().to_string();
                default_php.status = "active".to_string();
                default_php.compatibility = if detected_version.starts_with("8.") {
                    "Full".to_string()
                } else {
                    "Legacy".to_string()
                };
            }
        }
    }

    pub fn save(&self, snapshot: &AppSnapshot) -> AppResult<()> {
        let text = serde_json::to_string_pretty(snapshot)
            .map_err(|err| format!("Cannot serialize application state: {err}"))?;
        fs::write(&self.state_file, text)
            .map_err(|err| format!("Cannot save application state: {err}"))
    }

    pub fn ensure_host_files(&self, snapshot: &AppSnapshot) {
        for host in &snapshot.hosts {
            let root = PathBuf::from(&host.root_folder);
            let document_root = if Path::new(&host.document_root).is_absolute() {
                PathBuf::from(&host.document_root)
            } else {
                root.join(&host.document_root)
            };
            let logs = root.join("logs");
            let _ = fs::create_dir_all(&document_root);
            let _ = fs::create_dir_all(&logs);
            let index = document_root.join("index.html");
            if !index.exists() {
                let body = format!(
                    "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body><h1>{}</h1><p>LocalStack Pro host is ready.</p></body></html>",
                    host.domain, host.domain
                );
                let _ = fs::write(index, body);
            }
            let error_log = logs.join("error.log");
            if !error_log.exists() {
                let _ = fs::write(error_log, "");
            }
        }
    }

    pub fn ensure_service_files(&self, snapshot: &AppSnapshot) {
        for service in &snapshot.services {
            for path in [&service.config_path, &service.log_path] {
                let target = PathBuf::from(path);
                if let Some(parent) = target.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if !target.exists() {
                    let content = if path.ends_with(".log") {
                        String::new()
                    } else {
                        format!(
                            "# LocalStack Pro managed config for {}\nports={}\n",
                            service.name,
                            service
                                .ports
                                .iter()
                                .map(u16::to_string)
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    };
                    let _ = fs::write(target, content);
                }
            }
        }
    }

    pub fn log(
        &self,
        snapshot: &mut AppSnapshot,
        level: LogLevel,
        service: &str,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        let entry = LogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now().to_rfc3339(),
            level,
            service: service.to_string(),
            host: None,
            process_id: None,
            source: Some(format!("{}.log", service.to_lowercase())),
            line: None,
            message: message.into(),
            detail,
        };
        snapshot.logs.push(entry);
        if snapshot.logs.len() > 500 {
            let remove = snapshot.logs.len() - 500;
            snapshot.logs.drain(0..remove);
        }
    }

    pub fn refresh_runtime(&self, mut snapshot: AppSnapshot) -> AppSnapshot {
        let pids = snapshot
            .services
            .iter()
            .filter_map(|service| service.pid.map(Pid::from_u32))
            .collect::<Vec<_>>();
        let system = if pids.is_empty() {
            None
        } else {
            let mut system = System::new();
            system.refresh_processes_specifics(
                ProcessesToUpdate::Some(&pids),
                true,
                ProcessRefreshKind::nothing().with_memory().with_cpu(),
            );
            system.refresh_memory();
            Some(system)
        };
        for service in &mut snapshot.services {
            if let Some(pid) = service.pid {
                if let Some(process) = system
                    .as_ref()
                    .and_then(|system| system.process(Pid::from_u32(pid)))
                {
                    service.status = if service.status == ServiceStatus::Starting
                        && !service.ports.is_empty()
                        && !service.ports.iter().all(|port| port_accepting(*port))
                    {
                        ServiceStatus::Starting
                    } else {
                        ServiceStatus::Running
                    };
                    service.cpu = process.cpu_usage();
                    service.memory_mb = process.memory() / 1024 / 1024;
                    if let Some(started) = service.started_at {
                        service.uptime_seconds = (Utc::now().timestamp() - started).max(0) as u64;
                    }
                    service.last_error = None;
                } else {
                    service.status = ServiceStatus::Stopped;
                    service.pid = None;
                    service.cpu = 0.0;
                    service.memory_mb = 0;
                    service.uptime_seconds = 0;
                    service.started_at = None;
                }
            } else if matches!(
                service.status,
                ServiceStatus::Running | ServiceStatus::Starting
            ) {
                if Path::new(&service.executable_path).exists()
                    && !service.ports.is_empty()
                    && service.ports.iter().all(|port| port_accepting(*port))
                {
                    service.status = ServiceStatus::Running;
                    service.cpu = 0.0;
                    service.memory_mb = 0;
                    if service.started_at.is_none() {
                        service.started_at = Some(Utc::now().timestamp());
                    }
                    service.last_error = None;
                } else {
                    service.status = ServiceStatus::Stopped;
                    service.pid = None;
                    service.cpu = 0.0;
                    service.memory_mb = 0;
                    service.uptime_seconds = 0;
                    service.started_at = None;
                }
            } else if service.status == ServiceStatus::Stopped
                && service
                    .last_error
                    .as_deref()
                    .is_some_and(|message| message.starts_with("Executable not found:"))
            {
                service.last_error = None;
            }
        }
        let apache_ports = snapshot
            .services
            .iter()
            .find(|service| service.id == "apache")
            .map(|service| service.ports.clone())
            .unwrap_or_else(|| vec![80, 443]);
        let nginx_ports = snapshot
            .services
            .iter()
            .find(|service| service.id == "nginx")
            .map(|service| service.ports.clone())
            .unwrap_or_else(|| vec![8080, 8443]);
        let apache_running = snapshot
            .services
            .iter()
            .any(|service| service.id == "apache" && service.status == ServiceStatus::Running);
        let nginx_running = snapshot
            .services
            .iter()
            .any(|service| service.id == "nginx" && service.status == ServiceStatus::Running);
        for host in &mut snapshot.hosts {
            let uses_nginx = host.web_server.eq_ignore_ascii_case("nginx");
            let ports = if uses_nginx {
                &nginx_ports
            } else {
                &apache_ports
            };
            if let Some(port) = ports.first() {
                host.http_port = *port;
            }
            if let Some(port) = ports.get(1) {
                host.https_port = *port;
            }
            host.status = if (uses_nginx && nginx_running) || (!uses_nginx && apache_running) {
                ServiceStatus::Running
            } else {
                ServiceStatus::Stopped
            };
        }
        let previous_system = snapshot.system.clone();
        let (system_cpu, used_memory, disk_used) = current_system_metrics(&previous_system);
        snapshot.system = SystemInfo {
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            os: if previous_system.os.trim().is_empty() {
                System::long_os_version().unwrap_or_else(|| "Windows".to_string())
            } else {
                previous_system.os
            },
            uptime_seconds: System::uptime(),
            cpu: system_cpu,
            memory_gb: used_memory,
            disk_gb: disk_used,
        };
        snapshot
    }

    fn default_snapshot(&self) -> AppSnapshot {
        let now = Utc::now().to_rfc3339();
        let base = self.dir.display().to_string();
        let services_dir = format!("{base}\\services");
        let mut env = HashMap::new();
        env.insert("APP_ENV".to_string(), "local".to_string());
        env.insert("APP_DEBUG".to_string(), "true".to_string());
        let mut ini = HashMap::new();
        for (key, value) in default_ini_pairs() {
            ini.insert(key.to_string(), value.to_string());
        }

        AppSnapshot {
            app_data_dir: base.clone(),
            services: extended_default_services(&services_dir),
            hosts: vec![
                host(HostSeed {
                    domain: "shop.test",
                    root: "C:\\Projects\\shop",
                    php_version: "8.1.23",
                    ssl: true,
                    environment: "Production",
                    tags: vec!["ecommerce", "main"],
                    env_variables: env.clone(),
                    now: &now,
                }),
                host(HostSeed {
                    domain: "acme.test",
                    root: "C:\\Projects\\acme",
                    php_version: "8.2.10",
                    ssl: true,
                    environment: "Production",
                    tags: vec!["corporate", "primary"],
                    env_variables: env.clone(),
                    now: &now,
                }),
                host(HostSeed {
                    domain: "blog.test",
                    root: "C:\\Projects\\blog",
                    php_version: "8.3.6",
                    ssl: true,
                    environment: "Staging",
                    tags: vec!["blog", "headless"],
                    env_variables: env.clone(),
                    now: &now,
                }),
                host(HostSeed {
                    domain: "api.test",
                    root: "C:\\Projects\\api",
                    php_version: "8.2.10",
                    ssl: false,
                    environment: "Development",
                    tags: vec!["api"],
                    env_variables: env.clone(),
                    now: &now,
                }),
                host(HostSeed {
                    domain: "crm.test",
                    root: "C:\\Projects\\crm",
                    php_version: "7.4.33",
                    ssl: true,
                    environment: "Production",
                    tags: vec!["internal"],
                    env_variables: env.clone(),
                    now: &now,
                }),
                host(HostSeed {
                    domain: "legacy.test",
                    root: "C:\\Projects\\legacy",
                    php_version: "7.3.31",
                    ssl: false,
                    environment: "Development",
                    tags: vec!["legacy"],
                    env_variables: env,
                    now: &now,
                }),
            ],
            php_versions: ["8.4.22", "8.3.30", "8.2.29", "8.1.33", "7.4.33", "7.3.33"]
                .iter()
                .enumerate()
                .map(|(idx, version)| php(version, idx == 0, ini.clone()))
                .collect(),
            databases: vec![
                database(DatabaseSeed {
                    name: "shop",
                    description: "Main e-commerce DB",
                    engine: "MySQL",
                    version: "8.0.36",
                    user: "shop_user",
                    port: 3306,
                    size_mb: 128.6,
                    now: &now,
                }),
                database(DatabaseSeed {
                    name: "blog",
                    description: "Blog application DB",
                    engine: "MySQL",
                    version: "8.0.36",
                    user: "blog_user",
                    port: 3306,
                    size_mb: 64.2,
                    now: &now,
                }),
                database(DatabaseSeed {
                    name: "cms",
                    description: "CMS application DB",
                    engine: "MariaDB",
                    version: "10.6.18",
                    user: "cms_user",
                    port: 3307,
                    size_mb: 93.1,
                    now: &now,
                }),
                database(DatabaseSeed {
                    name: "test",
                    description: "Testing & development",
                    engine: "MySQL",
                    version: "8.0.36",
                    user: "test_user",
                    port: 3306,
                    size_mb: 12.4,
                    now: &now,
                }),
                database(DatabaseSeed {
                    name: "analytics",
                    description: "Analytics reporting",
                    engine: "PostgreSQL",
                    version: "15.3",
                    user: "analytics_user",
                    port: 5432,
                    size_mb: 256.7,
                    now: &now,
                }),
            ],
            certificates: vec![
                certificate_for(&base, "shop.test"),
                certificate_for(&base, "api.test"),
                certificate_for(&base, "crm.test"),
                certificate_for(&base, "blog.test"),
                certificate_for(&base, "legacy.test"),
                certificate_for(&base, "internal.test"),
            ],
            cms_installations: vec![],
            logs: vec![],
            settings: AppSettings {
                language: "English (US)".to_string(),
                preferred_browser: "Default System Browser".to_string(),
                minimize_to_tray: true,
                close_to_tray: true,
                launch_on_startup: false,
                show_notifications: true,
                play_sound: false,
                check_updates_on_startup: true,
                telemetry: false,
                ui_density: "Comfortable".to_string(),
                theme: default_theme(),
                log_level: "Information".to_string(),
                max_log_file_size: "50 MB".to_string(),
                retain_logs_days: 30,
                show_timestamps: true,
                projects_folder: "C:\\Projects".to_string(),
                services_folder: services_dir,
                backups_folder: format!("{base}\\backups"),
                http_port_start: 80,
                http_port_end: 8999,
                proxy_enabled: false,
                backup_retention_days: 30,
            },
            system: SystemInfo {
                app_version: env!("CARGO_PKG_VERSION").to_string(),
                os: "Windows".to_string(),
                uptime_seconds: 0,
                cpu: 0.0,
                memory_gb: 0.0,
                disk_gb: 0.0,
            },
        }
    }
}

fn current_system_metrics(previous: &SystemInfo) -> (f32, f64, f64) {
    let (cpu, memory_gb) = if let Ok(mut guard) = SYSTEM_METRICS.lock() {
        let system = guard.get_or_insert_with(System::new_all);
        system.refresh_memory();
        system.refresh_cpu_usage();
        let used_memory = system.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let total_memory = system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        (
            system.global_cpu_usage().clamp(0.0, 100.0),
            if total_memory > 0.0 {
                used_memory.min(total_memory)
            } else {
                previous.memory_gb
            },
        )
    } else {
        (previous.cpu, previous.memory_gb)
    };

    let disk_gb = if let Ok(mut guard) = DISK_METRICS.lock() {
        if let Some((refreshed_at, disk_gb)) = guard.as_ref() {
            if refreshed_at.elapsed() < Duration::from_secs(30) {
                *disk_gb
            } else {
                refresh_disk_metric(previous, &mut guard)
            }
        } else {
            refresh_disk_metric(previous, &mut guard)
        }
    } else {
        previous.disk_gb
    };

    (cpu, memory_gb, disk_gb)
}

fn refresh_disk_metric(previous: &SystemInfo, cache: &mut Option<(Instant, f64)>) -> f64 {
    let disks = Disks::new_with_refreshed_list();
    let total_disk: u64 = disks.iter().map(|disk| disk.total_space()).sum();
    let available_disk: u64 = disks.iter().map(|disk| disk.available_space()).sum();
    let disk_gb = if total_disk > 0 && total_disk >= available_disk {
        (total_disk - available_disk) as f64 / 1024.0 / 1024.0 / 1024.0
    } else {
        previous.disk_gb
    };
    *cache = Some((Instant::now(), disk_gb));
    disk_gb
}

pub fn bootstrap_bundled_services() -> AppResult<()> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if !store.bundled_snapshot_ready(&snapshot) && store.apply_bundled_services(&mut snapshot, true)
    {
        store.save(&snapshot)?;
    }
    Ok(())
}

fn default_ini_pairs() -> [(&'static str, &'static str); 15] {
    [
        ("memory_limit", "512M"),
        ("upload_max_filesize", "64M"),
        ("post_max_size", "64M"),
        ("max_execution_time", "120"),
        ("max_input_time", "120"),
        ("display_errors", "On"),
        ("display_startup_errors", "On"),
        ("log_errors", "On"),
        ("error_reporting", "E_ALL"),
        ("date_timezone", "UTC"),
        ("xdebug.mode", "develop,debug"),
        ("xdebug.start_with_request", "yes"),
        ("opcache.enable", "On"),
        ("opcache.memory_consumption", "128"),
        ("opcache.max_accelerated_files", "10000"),
    ]
}

fn default_theme() -> String {
    "Wet Asphalt".to_string()
}

fn certificate_for(base: &str, domain: &str) -> CertificateInfo {
    CertificateInfo {
        id: domain.to_string(),
        domain: domain.to_string(),
        status: "Valid".to_string(),
        trusted: true,
        expires_at: (Utc::now() + chrono::Duration::days(365)).to_rfc3339(),
        issuer: "LocalStack CA".to_string(),
        san_domains: vec![domain.to_string(), format!("www.{domain}")],
        auto_renew: true,
        cert_path: format!("{base}\\certs\\{domain}.crt"),
        key_path: format!("{base}\\keys\\{domain}.key"),
        pem_path: format!("{base}\\certs\\{domain}.pem"),
        fingerprint: "A3:6F:2B:9C:8D:33:1E:4A:7F:1C:6D:2E:9B:7E:77:2F:6C:4E:5F:8B:2A:09:3D:8F:5A:1B:2C:7D:9E:3F".to_string(),
    }
}

pub fn service(
    id: &str,
    name: &str,
    version: &str,
    executable_path: String,
    ports: Vec<u16>,
) -> ServiceInfo {
    let root = Path::new(&executable_path)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    ServiceInfo {
        id: id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        executable_path,
        arguments: vec![],
        config_path: format!("{root}\\conf\\service.conf"),
        log_path: format!("{root}\\logs\\service.log"),
        ports,
        status: ServiceStatus::Stopped,
        pid: None,
        uptime_seconds: 0,
        cpu: 0.0,
        memory_mb: 0,
        autostart: true,
        last_error: None,
        started_at: None,
    }
}

fn optional_service(
    id: &str,
    name: &str,
    version: &str,
    executable_path: String,
    ports: Vec<u16>,
) -> ServiceInfo {
    let mut item = service(id, name, version, executable_path, ports);
    item.autostart = false;
    item
}

fn extended_default_services(services_dir: &str) -> Vec<ServiceInfo> {
    vec![
        service(
            "apache",
            "Apache",
            "2.4.67",
            format!("{services_dir}\\apache\\bin\\httpd.exe"),
            vec![80, 443],
        ),
        service(
            "nginx",
            "Nginx",
            "1.29.8",
            format!("{services_dir}\\nginx\\nginx.exe"),
            vec![8080, 8443],
        ),
        service(
            "mysql",
            "MySQL",
            "9.7.0",
            format!("{services_dir}\\mysql\\bin\\mysqld.exe"),
            vec![3306],
        ),
        service(
            "mariadb",
            "MariaDB",
            "11.8.6",
            format!("{services_dir}\\mariadb\\bin\\mariadbd.exe"),
            vec![3307],
        ),
        service(
            "postgresql",
            "PostgreSQL",
            "18.4",
            format!("{services_dir}\\postgresql\\bin\\postgres.exe"),
            vec![5432],
        ),
        service(
            "redis",
            "Redis",
            "5.0.14.1",
            format!("{services_dir}\\redis\\redis-server.exe"),
            vec![6379],
        ),
        service(
            "mailpit",
            "Mailpit",
            "1.30.0",
            format!("{services_dir}\\mailpit\\mailpit.exe"),
            vec![1025, 8025],
        ),
        service(
            "node-proxy",
            "Node.js Proxy",
            "26.2.0",
            format!("{services_dir}\\nodejs\\node.exe"),
            vec![3000],
        ),
        service(
            "dns-helper",
            "DNS Helper",
            "1.0.4",
            format!("{services_dir}\\dns-helper\\dns-helper.exe"),
            vec![5353],
        ),
        optional_service(
            "mongodb",
            "MongoDB",
            "7.0",
            format!("{services_dir}\\mongodb\\bin\\mongod.exe"),
            vec![27017],
        ),
        optional_service(
            "memcached",
            "Memcached",
            "1.6",
            format!("{services_dir}\\memcached\\memcached.exe"),
            vec![11211],
        ),
        optional_service(
            "minio",
            "MinIO",
            "latest",
            format!("{services_dir}\\minio\\minio.exe"),
            vec![9000, 9001],
        ),
        optional_service(
            "elasticsearch",
            "Elasticsearch",
            "8.x",
            format!("{services_dir}\\elasticsearch\\bin\\elasticsearch.bat"),
            vec![9200],
        ),
        optional_service(
            "rabbitmq",
            "RabbitMQ",
            "3.x",
            format!("{services_dir}\\rabbitmq\\sbin\\rabbitmq-server.bat"),
            vec![5672, 15672],
        ),
        optional_service(
            "caddy",
            "Caddy",
            "2.x",
            format!("{services_dir}\\caddy\\caddy.exe"),
            vec![2019, 8081, 8444],
        ),
    ]
}

struct HostSeed<'a> {
    domain: &'a str,
    root: &'a str,
    php_version: &'a str,
    ssl: bool,
    environment: &'a str,
    tags: Vec<&'a str>,
    env_variables: HashMap<String, String>,
    now: &'a str,
}

fn host(seed: HostSeed<'_>) -> HostInfo {
    HostInfo {
        id: seed.domain.to_string(),
        domain: seed.domain.to_string(),
        root_folder: seed.root.to_string(),
        document_root: "public".to_string(),
        php_version: seed.php_version.to_string(),
        web_server: "Apache".to_string(),
        ssl: seed.ssl,
        environment: seed.environment.to_string(),
        http_port: 80,
        https_port: 443,
        database: seed
            .domain
            .split('.')
            .next()
            .unwrap_or(seed.domain)
            .to_string(),
        mail_service: "Mailpit".to_string(),
        env_variables: seed.env_variables,
        rewrite_rules: String::new(),
        notes: "Primary development environment.".to_string(),
        status: ServiceStatus::Stopped,
        tags: seed.tags.into_iter().map(str::to_string).collect(),
        created_at: seed.now.to_string(),
        updated_at: seed.now.to_string(),
    }
}

fn php(version: &str, default: bool, ini: HashMap<String, String>) -> PhpVersion {
    let extensions = [
        "xdebug",
        "intl",
        "gd",
        "imagick",
        "opcache",
        "pdo_mysql",
        "redis",
        "soap",
        "zip",
    ]
    .iter()
    .map(|name| PhpExtension {
        name: name.to_string(),
        version: if *name == "xdebug" { "3.2.1" } else { version }.to_string(),
        enabled: true,
        category: "Core".to_string(),
    })
    .collect();
    PhpVersion {
        version: version.to_string(),
        label: version.chars().take(3).collect(),
        status: if default { "active" } else { "installed" }.to_string(),
        default,
        cli_path: format!("C:\\tools\\php\\{version}\\php.exe"),
        sapi_mode: if version.starts_with("8.") {
            "FPM"
        } else {
            "Apache"
        }
        .to_string(),
        extensions,
        ini,
        compatibility: if version.starts_with("8.") {
            "Full"
        } else {
            "Legacy"
        }
        .to_string(),
    }
}

struct DatabaseSeed<'a> {
    name: &'a str,
    description: &'a str,
    engine: &'a str,
    version: &'a str,
    user: &'a str,
    port: u16,
    size_mb: f64,
    now: &'a str,
}

fn database(seed: DatabaseSeed<'_>) -> DatabaseInfo {
    DatabaseInfo {
        id: seed.name.to_string(),
        name: seed.name.to_string(),
        description: seed.description.to_string(),
        engine: seed.engine.to_string(),
        version: seed.version.to_string(),
        schemas: 4,
        user: seed.user.to_string(),
        password: "localstack".to_string(),
        port: seed.port,
        status: ServiceStatus::Stopped,
        size_mb: seed.size_mb,
        created_at: seed.now.to_string(),
    }
}

fn clean_windows_path(path: &str) -> String {
    path.trim_start_matches(r"\\?\").to_string()
}

fn fast_service_executable(service_id: &str) -> Option<PathBuf> {
    let candidates: &[&str] = match service_id {
        "apache" => &[
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\ApacheLounge.httpd_Microsoft.Winget.Source_8wekyb3d8bbwe\\Apache24\\bin\\httpd.exe",
            "C:\\Apache24\\bin\\httpd.exe",
            "C:\\Program Files\\Apache24\\bin\\httpd.exe",
        ],
        "nginx" => &[
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\nginxinc.nginx_Microsoft.Winget.Source_8wekyb3d8bbwe\\nginx-1.29.8\\nginx.exe",
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\nginx.exe",
            "C:\\nginx\\nginx.exe",
            "C:\\Program Files\\nginx\\nginx.exe",
        ],
        "mysql" => &[
            "C:\\Program Files\\MySQL\\MySQL Server 8.4\\bin\\mysqld.exe",
            "C:\\Program Files\\MySQL\\MySQL Server 8.0\\bin\\mysqld.exe",
        ],
        "mariadb" => &[
            "C:\\Program Files\\MariaDB 12.2\\bin\\mariadbd.exe",
            "C:\\Program Files\\MariaDB 11.4\\bin\\mariadbd.exe",
            "C:\\Program Files\\MariaDB 10.11\\bin\\mariadbd.exe",
        ],
        "postgresql" => &[
            "C:\\Program Files\\PostgreSQL\\18\\bin\\postgres.exe",
            "C:\\Program Files\\PostgreSQL\\17\\bin\\postgres.exe",
            "C:\\Program Files\\PostgreSQL\\16\\bin\\postgres.exe",
            "C:\\Program Files\\PostgreSQL\\15\\bin\\postgres.exe",
        ],
        "redis" => &[
            "C:\\Program Files\\Redis\\redis-server.exe",
            "C:\\Redis\\redis-server.exe",
        ],
        "mailpit" => &[
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\axllent.mailpit_Microsoft.Winget.Source_8wekyb3d8bbwe\\mailpit.exe",
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\mailpit.exe",
            "C:\\Program Files\\Mailpit\\mailpit.exe",
        ],
        "node-proxy" => &["C:\\Program Files\\nodejs\\node.exe"],
        "mongodb" => &[
            "C:\\Program Files\\MongoDB\\Server\\7.0\\bin\\mongod.exe",
            "C:\\Program Files\\MongoDB\\Server\\6.0\\bin\\mongod.exe",
        ],
        "memcached" => &[
            "C:\\Program Files\\memcached\\memcached.exe",
            "C:\\memcached\\memcached.exe",
        ],
        "minio" => &[
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\minio.exe",
            "C:\\Program Files\\MinIO\\minio.exe",
            "C:\\minio\\minio.exe",
        ],
        "elasticsearch" => &[
            "C:\\Program Files\\Elastic\\Elasticsearch\\8.17.0\\bin\\elasticsearch.bat",
            "C:\\elasticsearch\\bin\\elasticsearch.bat",
        ],
        "rabbitmq" => &[
            "C:\\Program Files\\RabbitMQ Server\\rabbitmq_server-3.13.0\\sbin\\rabbitmq-server.bat",
            "C:\\Program Files\\RabbitMQ Server\\rabbitmq_server-3.12.0\\sbin\\rabbitmq-server.bat",
        ],
        "caddy" => &[
            "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\caddy.exe",
            "C:\\Program Files\\Caddy\\caddy.exe",
            "C:\\caddy\\caddy.exe",
        ],
        "dns-helper" => {
            return std::env::current_exe().ok();
        }
        _ => return None,
    };
    candidates
        .iter()
        .map(|path| PathBuf::from(expand_service_env(path)))
        .find(|path| path.exists())
}

pub fn detect_service_executable(service_id: &str) -> Option<PathBuf> {
    fast_service_executable(service_id)
}

fn fast_php_executable(name: &str) -> Option<PathBuf> {
    let candidates = [
        format!("%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.4\\{name}"),
        format!("%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.3\\{name}"),
        format!("%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.2\\{name}"),
        format!("%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.1\\{name}"),
        format!("C:\\Program Files\\PHP\\8.4\\{name}"),
        format!("C:\\Program Files\\PHP\\8.3\\{name}"),
        format!("C:\\Program Files\\PHP\\8.3.30\\nts\\x64\\{name}"),
        format!("C:\\tools\\php\\{name}"),
    ];
    candidates
        .iter()
        .map(|path| PathBuf::from(expand_service_env(path)))
        .find(|path| path.exists())
        .or_else(|| which(name))
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|entry| entry.join(name))
        .find(|path| path.exists())
}

fn default_service_arguments(service_id: &str, executable_path: &Path) -> Vec<String> {
    match service_id {
        "apache" => executable_path
            .parent()
            .and_then(Path::parent)
            .map(|root| vec!["-d".to_string(), root.display().to_string()])
            .unwrap_or_default(),
        "nginx" => executable_path
            .parent()
            .map(|root| vec!["-p".to_string(), root.display().to_string()])
            .unwrap_or_default(),
        "dns-helper" => vec!["--localstack-dns-helper".to_string()],
        _ => Vec::new(),
    }
}

pub fn service_default_arguments(service_id: &str, executable_path: &Path) -> Vec<String> {
    default_service_arguments(service_id, executable_path)
}

fn expand_service_env(value: &str) -> String {
    value
        .replace(
            "%LOCALAPPDATA%",
            &std::env::var("LOCALAPPDATA").unwrap_or_default(),
        )
        .replace("%APPDATA%", &std::env::var("APPDATA").unwrap_or_default())
        .replace(
            "%ProgramFiles%",
            &std::env::var("ProgramFiles").unwrap_or_default(),
        )
        .replace(
            "%ProgramFiles(x86)%",
            &std::env::var("ProgramFiles(x86)").unwrap_or_default(),
        )
}

fn bundled_services_root() -> Option<PathBuf> {
    let exe_parent = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = Vec::new();
    if let Some(parent) = exe_parent {
        candidates.push(parent.join("bundled-services"));
        candidates.push(parent.join("resources").join("bundled-services"));
        candidates.push(parent.join("_up_").join("bundled-services"));
    }
    candidates.push(manifest_dir.join("bundled-services"));
    candidates
        .into_iter()
        .find(|path| path.join("manifest.json").is_file())
}

fn normalize_relative_path(value: &str) -> PathBuf {
    value
        .split('/')
        .filter(|part| !part.trim().is_empty() && *part != "." && *part != "..")
        .fold(PathBuf::new(), |path, part| path.join(part))
}

fn expand_zip_archive(archive: &Path, destination: &Path) -> AppResult<()> {
    let mut tar = Command::new("tar.exe");
    tar.args([
        "-xf",
        &archive.display().to_string(),
        "-C",
        &destination.display().to_string(),
    ]);
    tar.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    tar.creation_flags(CREATE_NO_WINDOW);
    if let Ok(output) = tar.output() {
        if output.status.success() {
            return Ok(());
        }
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        powershell_quote(&archive.display().to_string()),
        powershell_quote(&destination.display().to_string())
    );
    let mut command = Command::new("powershell.exe");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot extract bundled services: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot extract bundled services. Exit code {:?}. {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn port_accepting(port: u16) -> bool {
    let Ok(addresses) = ("localhost", port).to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(20)).is_ok())
}
