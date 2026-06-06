use crate::state::{AppResult, ServiceStatus, Store};
use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::{
    fs,
    net::{TcpStream, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub generated_at: String,
    pub summary: String,
    pub ok: u32,
    pub warnings: u32,
    pub errors: u32,
    pub checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub action: Option<String>,
}

pub fn run_health_check() -> AppResult<HealthReport> {
    let store = Store::new()?;
    let snapshot = store.load()?;
    let mut checks = Vec::new();

    check_data_dirs(&store, &mut checks);
    check_duplicates(&snapshot, &mut checks);
    check_services(&snapshot, &mut checks);
    check_hosts(&store, &snapshot, &mut checks);
    check_runtime_configs(&store, &snapshot, &mut checks);
    check_php(&snapshot, &mut checks);
    check_database_clients(&snapshot, &mut checks);
    check_cms_installations(&snapshot, &mut checks);
    check_tools_route(&store, &mut checks);
    check_settings(&snapshot, &mut checks);
    check_security(&snapshot, &mut checks);

    let ok = checks.iter().filter(|check| check.severity == "ok").count() as u32;
    let warnings = checks
        .iter()
        .filter(|check| check.severity == "warning")
        .count() as u32;
    let errors = checks
        .iter()
        .filter(|check| check.severity == "error")
        .count() as u32;
    let summary = if errors > 0 {
        format!("{errors} critical issue(s), {warnings} warning(s)")
    } else if warnings > 0 {
        format!("{warnings} warning(s), no critical issues")
    } else {
        "Environment is healthy".to_string()
    };

    Ok(HealthReport {
        generated_at: Utc::now().to_rfc3339(),
        summary,
        ok,
        warnings,
        errors,
        checks,
    })
}

pub fn repair_environment() -> AppResult<HealthReport> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let pre_repair = store.dir.join("backups").join(format!(
        "pre-repair-{}.zip",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let _ = crate::settings::create_app_backup(pre_repair.display().to_string());
    store.ensure_host_files(&snapshot);
    store.ensure_service_files(&snapshot);

    let _ = crate::dependencies::detect_dependencies();
    if snapshot
        .hosts
        .iter()
        .any(|host| !crate::hosts::hosts_file_maps_domain(&host.domain))
    {
        let _ = crate::hosts::sync_hosts_file();
    }
    if database_admin_tool_missing(&store, "adminer") {
        let _ = crate::settings::install_database_admin_tool("adminer".to_string());
    }
    if database_admin_tool_missing(&store, "phpmyadmin") {
        let _ = crate::settings::install_database_admin_tool("phpmyadmin".to_string());
    }

    let snapshot = store.load_static()?;
    for host in snapshot.hosts.iter().filter(|host| host.ssl) {
        let has_certificate = snapshot.certificates.iter().any(|cert| {
            cert.domain.eq_ignore_ascii_case(&host.domain) && Path::new(&cert.cert_path).exists()
        });
        if !has_certificate {
            let _ = crate::ssl::generate_certificate(
                host.domain.clone(),
                vec![host.domain.clone(), format!("www.{}", host.domain)],
            );
        }
    }

    for service_id in ["apache", "mysql", "mailpit"] {
        let _ = crate::services::start_service(service_id.to_string());
    }
    run_health_check()
}

fn database_admin_tool_missing(store: &Store, kind: &str) -> bool {
    let tools = store.dir.join("tools").join("public");
    let target = if kind == "adminer" {
        tools.join("adminer.php")
    } else {
        tools.join("phpmyadmin").join("index.php")
    };
    if !target.is_file() {
        return true;
    }
    fs::read_to_string(target)
        .map(|text| text.contains("The database tool route is ready."))
        .unwrap_or(true)
}

fn check_duplicates(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    let duplicate_hosts = duplicate_values(snapshot.hosts.iter().map(|host| host.domain.as_str()));
    if duplicate_hosts.is_empty() {
        ok(
            checks,
            "duplicates-hosts",
            "Duplicate hosts",
            "No duplicate host domains found".to_string(),
            None,
        );
    } else {
        error(
            checks,
            "duplicates-hosts",
            "Duplicate hosts",
            format!("Duplicate host domains: {}", duplicate_hosts.join(", ")),
            "Delete or rename duplicate hosts.",
        );
    }

    let duplicate_services =
        duplicate_values(snapshot.services.iter().map(|service| service.id.as_str()));
    if duplicate_services.is_empty() {
        ok(
            checks,
            "duplicates-services",
            "Duplicate services",
            "No duplicate service ids found".to_string(),
            None,
        );
    } else {
        error(
            checks,
            "duplicates-services",
            "Duplicate services",
            format!("Duplicate service ids: {}", duplicate_services.join(", ")),
            "Keep one service entry per service id.",
        );
    }

    let duplicate_databases = duplicate_values(
        snapshot
            .databases
            .iter()
            .map(|database| database.name.as_str()),
    );
    if duplicate_databases.is_empty() {
        ok(
            checks,
            "duplicates-databases",
            "Duplicate databases",
            "No duplicate database names found".to_string(),
            None,
        );
    } else {
        warning(
            checks,
            "duplicates-databases",
            "Duplicate databases",
            format!(
                "Duplicate database names: {}",
                duplicate_databases.join(", ")
            ),
            "Delete or rename duplicate database entries.",
        );
    }
}

fn duplicate_values<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for value in values {
        let key = value.trim().to_lowercase();
        if !key.is_empty() {
            *counts.entry(key).or_default() += 1;
        }
    }
    let mut duplicates = counts
        .into_iter()
        .filter_map(|(value, count)| (count > 1).then_some(value))
        .collect::<Vec<_>>();
    duplicates.sort();
    duplicates
}

fn check_data_dirs(store: &Store, checks: &mut Vec<HealthCheck>) {
    for name in ["configs", "hosts", "logs", "backups", "certs", "keys"] {
        let path = store.dir.join(name);
        if path.is_dir() {
            ok(
                checks,
                format!("dir-{name}"),
                format!("{} folder", title(name)),
                format!("{} exists", path.display()),
                None,
            );
        } else {
            error(
                checks,
                format!("dir-{name}"),
                format!("{} folder", title(name)),
                format!("{} is missing", path.display()),
                "Open Settings > Paths or restart LocalStack Pro to recreate data folders.",
            );
        }
    }
}

fn check_services(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    for service in &snapshot.services {
        let path = Path::new(&service.executable_path);
        if path.exists() {
            ok(
                checks,
                format!("service-{}-binary", service.id),
                format!("{} executable", service.name),
                format!("Found {}", path.display()),
                None,
            );
        } else if service.autostart || service.status == ServiceStatus::Running {
            error(
                checks,
                format!("service-{}-binary", service.id),
                format!("{} executable", service.name),
                format!("Missing {}", path.display()),
                "Use Services > Detect or Install for this service.",
            );
        } else {
            warning(
                checks,
                format!("service-{}-binary", service.id),
                format!("{} executable", service.name),
                format!("Missing {}", path.display()),
                "Use Services > Detect or Install for this service.",
            );
        }

        for port in &service.ports {
            let listening = if service.id == "dns-helper" {
                udp_port_in_use(*port)
            } else {
                tcp_ready("127.0.0.1", *port)
            };
            if service.status == ServiceStatus::Running {
                if listening {
                    ok(
                        checks,
                        format!("service-{}-port-{port}", service.id),
                        format!("{} port {port}", service.name),
                        "Port is accepting TCP connections".to_string(),
                        None,
                    );
                } else if service.id == "dns-helper" {
                    warning(
                        checks,
                        format!("service-{}-port-{port}", service.id),
                        format!("{} port {port}", service.name),
                        "DNS Helper is marked running, but the UDP port did not answer this check"
                            .to_string(),
                        "Restart DNS Helper if local wildcard domains do not resolve.",
                    );
                } else {
                    error(
                        checks,
                        format!("service-{}-port-{port}", service.id),
                        format!("{} port {port}", service.name),
                        "Service is marked running, but the port is not reachable".to_string(),
                        "Restart the service from the Services page.",
                    );
                }
            } else if listening && service.id == "dns-helper" {
                ok(
                    checks,
                    format!("service-{}-port-{port}", service.id),
                    format!("{} port {port}", service.name),
                    "Port is used by Windows/network discovery; DNS Helper will pick a fallback port when started".to_string(),
                    None,
                );
            } else if listening && service.id == "nginx" && *port == 8080 {
                ok(
                    checks,
                    format!("service-{}-port-{port}", service.id),
                    format!("{} port {port}", service.name),
                    "Nginx is accepting connections on the primary HTTP port".to_string(),
                    None,
                );
            } else if listening {
                let owner = port_owner_detail(*port)
                    .map(|detail| format!(" Owner: {detail}."))
                    .unwrap_or_default();
                warning(
                    checks,
                    format!("service-{}-port-{port}", service.id),
                    format!("{} port {port}", service.name),
                    format!("Port is already in use while service is not marked running.{owner}"),
                    "Click Detect, then restart the service or free the port.",
                );
            } else {
                ok(
                    checks,
                    format!("service-{}-port-{port}", service.id),
                    format!("{} port {port}", service.name),
                    "Port is free".to_string(),
                    None,
                );
            }
        }

        if let Some(error_text) = &service.last_error {
            warning(
                checks,
                format!("service-{}-last-error", service.id),
                format!("{} last error", service.name),
                error_text.clone(),
                "Open the service details and run Detect or Install.",
            );
        }
    }
}

fn check_cms_installations(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    for cms in &snapshot.cms_installations {
        let public = PathBuf::from(&cms.root_folder).join(&cms.document_root);
        if public.join("index.php").is_file() {
            ok(
                checks,
                format!("cms-{}-index", cms.domain),
                format!("{} CMS index", cms.domain),
                format!("Found {}", public.join("index.php").display()),
                None,
            );
        } else {
            error(
                checks,
                format!("cms-{}-index", cms.domain),
                format!("{} CMS index", cms.domain),
                format!("Missing {}", public.join("index.php").display()),
                "Reinstall CMS or choose the correct document root.",
            );
        }

        let metadata = PathBuf::from(&cms.root_folder).join("localstack-cms.json");
        if metadata.is_file() {
            let text = fs::read_to_string(&metadata).unwrap_or_default();
            if text.contains(&cms.domain) && text.contains(&cms.template_id) {
                ok(
                    checks,
                    format!("cms-{}-metadata", cms.domain),
                    format!("{} CMS metadata", cms.domain),
                    "Installation metadata matches this host".to_string(),
                    None,
                );
            } else {
                warning(
                    checks,
                    format!("cms-{}-metadata", cms.domain),
                    format!("{} CMS metadata", cms.domain),
                    "Installation metadata does not match the current host".to_string(),
                    "Run CMS install again with overwrite disabled to attach the existing files.",
                );
            }
        } else {
            warning(
                checks,
                format!("cms-{}-metadata", cms.domain),
                format!("{} CMS metadata", cms.domain),
                format!("Missing {}", metadata.display()),
                "Run CMS install again to recreate metadata.",
            );
        }

        if let Some(database) = &cms.database {
            if snapshot.databases.iter().any(|item| {
                item.name.eq_ignore_ascii_case(database) || item.id.eq_ignore_ascii_case(database)
            }) {
                ok(
                    checks,
                    format!("cms-{}-database", cms.domain),
                    format!("{} CMS database", cms.domain),
                    format!("Database {database} is registered"),
                    None,
                );
            } else {
                error(
                    checks,
                    format!("cms-{}-database", cms.domain),
                    format!("{} CMS database", cms.domain),
                    format!("Database {database} is missing from LocalStack Pro"),
                    "Create the database or reinstall the CMS.",
                );
            }
        }
    }
}

fn check_security(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    let external_listeners = external_listener_details();
    let externally_bound = snapshot
        .services
        .iter()
        .filter(|service| service.status == ServiceStatus::Running)
        .flat_map(|service| {
            service
                .ports
                .iter()
                .filter_map(|port| {
                    external_listeners
                        .get(port)
                        .cloned()
                        .map(|detail| (service.name.clone(), *port, detail))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    if externally_bound.is_empty() {
        ok(
            checks,
            "security-loopback",
            "Service bind addresses",
            "No running service is detected on an external interface".to_string(),
            None,
        );
    } else {
        for (service, port, detail) in externally_bound {
            warning(
                checks,
                format!("security-bind-{service}-{port}"),
                format!("{service} external bind"),
                format!("Port {port} is listening outside localhost: {detail}"),
                "Bind local services to 127.0.0.1 unless external access is required.",
            );
        }
    }

    if snapshot
        .php_versions
        .iter()
        .filter(|php| php.default)
        .any(|php| {
            php.ini
                .get("display_errors")
                .is_some_and(|value| value.eq_ignore_ascii_case("On"))
        })
    {
        warning(
            checks,
            "security-php-display-errors",
            "PHP display_errors",
            "Default PHP has display_errors enabled".to_string(),
            "Disable display_errors for production-like local testing.",
        );
    } else {
        ok(
            checks,
            "security-php-display-errors",
            "PHP display_errors",
            "Default PHP does not expose errors in responses".to_string(),
            None,
        );
    }

    for database in snapshot.databases.iter().filter(|database| {
        database.password.is_empty()
            || database.password.eq_ignore_ascii_case("localstack")
            || database.password.eq_ignore_ascii_case("password")
    }) {
        warning(
            checks,
            format!("security-db-password-{}", database.id),
            format!("{} database password", database.name),
            "Database uses an empty or default password".to_string(),
            "Generate a stronger local password from the Database page.",
        );
    }

    let untrusted = snapshot
        .certificates
        .iter()
        .filter(|certificate| !certificate.trusted)
        .count();
    if untrusted == 0 {
        ok(
            checks,
            "security-cert-trust",
            "Certificate trust",
            "All registered certificates are trusted".to_string(),
            None,
        );
    } else {
        warning(
            checks,
            "security-cert-trust",
            "Certificate trust",
            format!("{untrusted} certificate(s) are not trusted"),
            "Open SSL and click Repair Trust for affected certificates.",
        );
    }
}

fn check_hosts(store: &Store, snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    for host in &snapshot.hosts {
        let document_root = host_document_root(host);
        if document_root.is_dir() {
            ok(
                checks,
                format!("host-{}-root", host.domain),
                format!("{} document root", host.domain),
                format!("Found {}", document_root.display()),
                None,
            );
        } else {
            error(
                checks,
                format!("host-{}-root", host.domain),
                format!("{} document root", host.domain),
                format!("Missing {}", document_root.display()),
                "Create the folder or edit the host document root.",
            );
        }

        if hosts_file_maps_domain(&host.domain) {
            ok(
                checks,
                format!("host-{}-hosts", host.domain),
                format!("{} hosts mapping", host.domain),
                "Windows hosts file maps this domain to localhost".to_string(),
                None,
            );
        } else {
            error(
                checks,
                format!("host-{}-hosts", host.domain),
                format!("{} hosts mapping", host.domain),
                "Domain is not mapped in the Windows hosts file".to_string(),
                "Click Sync Hosts File and approve the administrator prompt.",
            );
        }

        let service_id = if host.web_server.eq_ignore_ascii_case("nginx") {
            "nginx"
        } else {
            "apache"
        };
        let runtime_has_host = runtime_config_contains(store, service_id, &host.domain);
        if runtime_has_host {
            ok(
                checks,
                format!("host-{}-vhost", host.domain),
                format!("{} vhost", host.domain),
                format!("{} runtime config contains this host", host.web_server),
                None,
            );
        } else {
            warning(
                checks,
                format!("host-{}-vhost", host.domain),
                format!("{} vhost", host.domain),
                format!(
                    "{} runtime config does not contain this host",
                    host.web_server
                ),
                "Restart the selected web server to regenerate runtime config.",
            );
        }

        if host.status == ServiceStatus::Running {
            let port = host.http_port;
            if tcp_ready(&host.domain, port) {
                ok(
                    checks,
                    format!("host-{}-endpoint", host.domain),
                    format!("{} endpoint", host.domain),
                    format!("http://{}:{} is reachable", host.domain, port),
                    None,
                );
            } else {
                error(
                    checks,
                    format!("host-{}-endpoint", host.domain),
                    format!("{} endpoint", host.domain),
                    format!("http://{}:{} is not reachable", host.domain, port),
                    "Check service status, hosts-file mapping, and vhost config.",
                );
            }
        }
    }
}

fn check_runtime_configs(
    store: &Store,
    snapshot: &crate::state::AppSnapshot,
    checks: &mut Vec<HealthCheck>,
) {
    let apache = store
        .dir
        .join("configs")
        .join("apache-runtime")
        .join("httpd.conf");
    check_runtime_file(checks, "apache-runtime", "Apache runtime config", &apache);
    if apache.exists() {
        let text = fs::read_to_string(&apache).unwrap_or_default();
        if text.contains("Alias /localstack-tools/") {
            ok(
                checks,
                "apache-tools-route",
                "Apache tools route",
                "Tools route is configured".to_string(),
                None,
            );
        } else {
            warning(
                checks,
                "apache-tools-route",
                "Apache tools route",
                "Tools route is missing from Apache config".to_string(),
                "Restart Apache to regenerate runtime config.",
            );
        }
        if text.contains("Timeout 90") {
            ok(
                checks,
                "apache-timeout",
                "Apache timeout",
                "Apache runtime timeout is current".to_string(),
                None,
            );
        } else {
            warning(
                checks,
                "apache-timeout",
                "Apache timeout",
                "Apache runtime timeout is stale and can interrupt PHP-CGI requests".to_string(),
                "Restart Apache to regenerate runtime config.",
            );
        }
        if text.contains("SetEnv PHPRC") && text.contains("SetEnv TMP") {
            ok(
                checks,
                "apache-php-runtime",
                "Apache PHP runtime",
                "PHP runtime environment is configured".to_string(),
                None,
            );
        } else {
            warning(
                checks,
                "apache-php-runtime",
                "Apache PHP runtime",
                "PHP runtime environment is missing or stale".to_string(),
                "Restart Apache to regenerate runtime config.",
            );
        }
        if text.contains("DocumentRoot \"C:/Projects/shop/public\"")
            && !text.contains("ServerName localhost")
        {
            warning(
                checks,
                "apache-default-vhost",
                "Apache default vhost",
                "Default vhost may fall back to a project folder".to_string(),
                "Restart Apache with the current LocalStack Pro build.",
            );
        }
    }

    let nginx = store
        .dir
        .join("configs")
        .join("nginx-runtime")
        .join("conf")
        .join("nginx.conf");
    let nginx_required = snapshot
        .services
        .iter()
        .any(|service| service.id == "nginx" && service.status == ServiceStatus::Running)
        || snapshot
            .hosts
            .iter()
            .any(|host| host.web_server.eq_ignore_ascii_case("nginx"));
    if nginx_required {
        check_runtime_file(checks, "nginx-runtime", "Nginx runtime config", &nginx);
    }
}

fn check_php(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    if let Some(default_php) = snapshot.php_versions.iter().find(|php| php.default) {
        if Path::new(&default_php.cli_path).exists() {
            ok(
                checks,
                "php-default-cli",
                "Default PHP CLI",
                format!("Found {}", default_php.cli_path),
                None,
            );
        } else {
            warning(
                checks,
                "php-default-cli",
                "Default PHP CLI",
                format!("Configured CLI path is missing: {}", default_php.cli_path),
                "Install PHP or edit the PHP CLI path.",
            );
        }
    } else {
        error(
            checks,
            "php-default-cli",
            "Default PHP CLI",
            "No default PHP version is selected".to_string(),
            "Open PHP and select a default version.",
        );
    }
}

fn check_database_clients(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    for database in &snapshot.databases {
        let service_id = match database.engine.as_str() {
            "PostgreSQL" => "postgresql",
            "MariaDB" => "mariadb",
            _ => "mysql",
        };
        let client = if database.engine == "PostgreSQL" {
            "psql.exe"
        } else {
            "mysql.exe"
        };
        let Some(service) = snapshot.services.iter().find(|item| item.id == service_id) else {
            error(
                checks,
                format!("database-{}-service", database.id),
                format!("{} service", database.engine),
                format!("Service {service_id} is not configured"),
                "Open Services and install or detect the database service.",
            );
            continue;
        };
        let client_path = database_tool_path(&service.executable_path, client);
        if client_path.exists() {
            ok(
                checks,
                format!("database-{}-client", database.id),
                format!("{} client", database.engine),
                format!("Found {}", client_path.display()),
                None,
            );
        } else {
            warning(
                checks,
                format!("database-{}-client", database.id),
                format!("{} client", database.engine),
                format!("Missing {}", client_path.display()),
                "Install the database client or run Services > Detect.",
            );
        }
    }
}

fn check_tools_route(store: &Store, checks: &mut Vec<HealthCheck>) {
    let tools = store.dir.join("tools").join("public");
    if tools.is_dir() {
        ok(
            checks,
            "tools-folder",
            "Tools folder",
            format!("Found {}", tools.display()),
            None,
        );
        let adminer = tools.join("adminer.php");
        check_tool_file(
            checks,
            "tool-adminer",
            "Adminer",
            &adminer,
            "Open Database > Open Adminer to install the tool.",
        );
        let phpmyadmin = tools.join("phpmyadmin").join("index.php");
        check_tool_file(
            checks,
            "tool-phpmyadmin",
            "phpMyAdmin",
            &phpmyadmin,
            "Open Database > Open phpMyAdmin to install the tool.",
        );
    } else {
        warning(
            checks,
            "tools-folder",
            "Tools folder",
            format!("Missing {}", tools.display()),
            "Open phpMyAdmin/Adminer once to create the tools folder.",
        );
    }
}

fn check_tool_file(
    checks: &mut Vec<HealthCheck>,
    id: impl Into<String>,
    title: impl Into<String>,
    path: &Path,
    action: impl Into<String>,
) {
    if path.is_file() {
        let text = fs::read_to_string(path).unwrap_or_default();
        if text.contains("The database tool route is ready.") {
            warning(
                checks,
                id,
                title,
                format!("{} is still a placeholder", path.display()),
                action,
            );
        } else {
            ok(checks, id, title, format!("Found {}", path.display()), None);
        }
    } else {
        warning(
            checks,
            id,
            title,
            format!("Missing {}", path.display()),
            action,
        );
    }
}

fn check_settings(snapshot: &crate::state::AppSnapshot, checks: &mut Vec<HealthCheck>) {
    if snapshot.settings.http_port_start <= snapshot.settings.http_port_end {
        ok(
            checks,
            "settings-port-range",
            "Port range",
            format!(
                "{}-{}",
                snapshot.settings.http_port_start, snapshot.settings.http_port_end
            ),
            None,
        );
    } else {
        error(
            checks,
            "settings-port-range",
            "Port range",
            "HTTP Port Start is greater than HTTP Port End".to_string(),
            "Open Settings > Network and correct the port range.",
        );
    }
}

fn check_runtime_file(
    checks: &mut Vec<HealthCheck>,
    id: impl Into<String>,
    title: impl Into<String>,
    path: &Path,
) {
    if path.exists() {
        ok(checks, id, title, format!("Found {}", path.display()), None);
    } else {
        warning(
            checks,
            id,
            title,
            format!("Missing {}", path.display()),
            "Start or restart the service to generate runtime config.",
        );
    }
}

fn host_document_root(host: &crate::state::HostInfo) -> PathBuf {
    let root = PathBuf::from(&host.root_folder);
    let document = PathBuf::from(&host.document_root);
    if document.is_absolute() {
        document
    } else {
        root.join(document)
    }
}

fn hosts_file_maps_domain(domain: &str) -> bool {
    let Ok(windir) = std::env::var("WINDIR") else {
        return false;
    };
    let path = Path::new(&windir)
        .join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts");
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    text.lines().any(|line| {
        let clean = line.split('#').next().unwrap_or_default();
        let mut parts = clean.split_whitespace();
        let Some(address) = parts.next() else {
            return false;
        };
        let local = matches!(address, "127.0.0.1" | "::1" | "0.0.0.0");
        local && parts.any(|name| name.eq_ignore_ascii_case(domain))
    })
}

fn runtime_config_contains(store: &Store, service_id: &str, domain: &str) -> bool {
    let config = match service_id {
        "nginx" => store
            .dir
            .join("configs")
            .join("nginx-runtime")
            .join("conf")
            .join("nginx.conf"),
        _ => store
            .dir
            .join("configs")
            .join("apache-runtime")
            .join("httpd.conf"),
    };
    let Ok(text) = fs::read_to_string(config) else {
        return false;
    };
    text.lines().any(|line| {
        let line = line.trim();
        line.eq_ignore_ascii_case(&format!("ServerName {domain}"))
            || line.eq_ignore_ascii_case(&format!("server_name {domain};"))
            || line.eq_ignore_ascii_case(&format!("server_name  {domain};"))
    })
}

fn database_tool_path(executable_path: &str, tool: &str) -> PathBuf {
    let executable = Path::new(executable_path);
    let sibling = executable.with_file_name(tool);
    if sibling.exists() {
        return sibling;
    }
    executable
        .parent()
        .and_then(Path::parent)
        .map(|parent| parent.join("bin").join(tool))
        .unwrap_or(sibling)
}

fn tcp_ready(host: &str, port: u16) -> bool {
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(180)).is_ok())
}

fn udp_port_in_use(port: u16) -> bool {
    UdpSocket::bind(("127.0.0.1", port)).is_err()
}

fn port_owner_detail(port: u16) -> Option<String> {
    let output = hidden_command_output("netstat", &["-ano"]).unwrap_or_default();
    output.lines().find_map(|line| {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 5 || !parts[1].ends_with(&format!(":{port}")) {
            return None;
        }
        Some(format!("pid={}", parts[4]))
    })
}

fn external_listener_details() -> HashMap<u16, String> {
    let output = hidden_command_output("netstat", &["-ano"]).unwrap_or_default();
    output
        .lines()
        .filter_map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 5 || !parts[3].eq_ignore_ascii_case("LISTENING") {
                return None;
            }
            let local = parts[1];
            if local.starts_with("127.0.0.1:") || local.starts_with("[::1]:") {
                return None;
            }
            let port = local.rsplit(':').next()?.parse::<u16>().ok()?;
            Some((port, format!("{local} pid={}", parts[4])))
        })
        .collect()
}

fn hidden_command_output(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn title(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn ok(
    checks: &mut Vec<HealthCheck>,
    id: impl Into<String>,
    title: impl Into<String>,
    message: impl Into<String>,
    detail: Option<String>,
) {
    checks.push(HealthCheck {
        id: id.into(),
        title: title.into(),
        severity: "ok".to_string(),
        message: message.into(),
        detail,
        action: None,
    });
}

fn warning(
    checks: &mut Vec<HealthCheck>,
    id: impl Into<String>,
    title: impl Into<String>,
    message: impl Into<String>,
    action: impl Into<String>,
) {
    checks.push(HealthCheck {
        id: id.into(),
        title: title.into(),
        severity: "warning".to_string(),
        message: message.into(),
        detail: None,
        action: Some(action.into()),
    });
}

fn error(
    checks: &mut Vec<HealthCheck>,
    id: impl Into<String>,
    title: impl Into<String>,
    message: impl Into<String>,
    action: impl Into<String>,
) {
    checks.push(HealthCheck {
        id: id.into(),
        title: title.into(),
        severity: "error".to_string(),
        message: message.into(),
        detail: None,
        action: Some(action.into()),
    });
}
