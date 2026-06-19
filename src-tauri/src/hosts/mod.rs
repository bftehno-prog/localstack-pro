use crate::state::{AppResult, HostInfo, LogLevel, ServiceStatus, Store};
use chrono::Utc;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    net::{TcpStream, ToSocketAddrs},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const HOSTS_BEGIN: &str = "# LocalStack Pro begin";
const HOSTS_END: &str = "# LocalStack Pro end";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiagnosticCheck {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDiagnosticReport {
    pub host_id: String,
    pub domain: String,
    pub generated_at: String,
    pub summary: String,
    pub ok: u32,
    pub warnings: u32,
    pub errors: u32,
    pub checks: Vec<HostDiagnosticCheck>,
}

pub fn save_host(mut host: HostInfo) -> AppResult<crate::state::AppSnapshot> {
    validate_host(&host)?;
    normalize_document_root(&mut host);
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    host.updated_at = Utc::now().to_rfc3339();
    if snapshot
        .hosts
        .iter()
        .any(|item| item.domain == host.domain && item.id != host.id)
    {
        return Err(format!("Host domain {} already exists.", host.domain));
    }
    apply_database_environment(&mut host, &snapshot);
    if let Some(existing) = snapshot.hosts.iter_mut().find(|item| item.id == host.id) {
        *existing = host.clone();
    } else {
        snapshot.hosts.push(host.clone());
    }
    create_host_project_files(&host)?;
    write_host_environment_files(&host)?;
    configure_detected_cms(&host)?;
    write_host_config(&store, &host)?;
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Hosts",
        format!("Host {} saved", host.domain),
        None,
    );
    if !hosts_file_maps_domain(&host.domain) {
        match try_write_windows_hosts_file_direct(&store, &snapshot) {
            Ok(()) => store.log(
                &mut snapshot,
                LogLevel::Info,
                "Hosts",
                format!("Windows hosts file was synchronized for {}", host.domain),
                None,
            ),
            Err(err) => store.log(
                &mut snapshot,
                LogLevel::Warning,
                "Hosts",
                format!(
                    "Host {} was saved, but Windows hosts sync requires administrator approval",
                    host.domain
                ),
                Some(format!(
                    "{err}. Use Sync Hosts File when you are ready to approve it."
                )),
            ),
        }
    }
    if let Err(err) = sync_proxy_bypass_for_hosts(&snapshot) {
        store.log(
            &mut snapshot,
            LogLevel::Warning,
            "Hosts",
            format!("Proxy bypass update failed for {}: {err}", host.domain),
            None,
        );
    }
    store.save(&snapshot)?;
    let service_id = if host.web_server.eq_ignore_ascii_case("nginx") {
        "nginx"
    } else {
        "apache"
    };
    if snapshot.services.iter().any(|service| {
        service.id == service_id && service.status == crate::state::ServiceStatus::Running
    }) {
        return crate::services::restart_service(service_id.to_string());
    }
    Ok(snapshot)
}

pub fn diagnose_host(host_id: String) -> AppResult<HostDiagnosticReport> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let host = snapshot
        .hosts
        .iter()
        .find(|host| host.id == host_id || host.domain.eq_ignore_ascii_case(&host_id))
        .cloned()
        .ok_or_else(|| "Host not found.".to_string())?;
    let mut checks = Vec::new();
    let doc_root = document_root(&host);
    let is_node_host = host.env_variables.contains_key("LOCALSTACK_NODE_PORT")
        || host.tags.iter().any(|tag| {
            matches!(
                tag.as_str(),
                "node" | "nextjs" | "node-express" | "vite-react" | "meteor" | "meteor-blog-cms"
            )
        });

    push_check(
        &mut checks,
        "document-root",
        "Document root",
        if doc_root.is_dir() { "ok" } else { "error" },
        if doc_root.is_dir() {
            format!("{} exists.", doc_root.display())
        } else {
            format!("{} does not exist.", doc_root.display())
        },
        None,
        if doc_root.is_dir() {
            None
        } else {
            Some("Create the folder or update host paths.".to_string())
        },
    );

    let index_ok = if is_node_host {
        doc_root.join("package.json").is_file()
    } else {
        ["index.php", "index.html"]
            .iter()
            .any(|name| doc_root.join(name).is_file())
    };
    push_check(
        &mut checks,
        "index-file",
        "Index file",
        if index_ok { "ok" } else { "warning" },
        if index_ok {
            if is_node_host {
                "package.json was found for the Node host.".to_string()
            } else {
                "index.php or index.html was found.".to_string()
            }
        } else {
            if is_node_host {
                "No package.json was found in the Node host root.".to_string()
            } else {
                "No index.php or index.html was found in document root.".to_string()
            }
        },
        None,
        if index_ok {
            None
        } else {
            Some("Install a CMS or add an index file.".to_string())
        },
    );

    let service_id = if host.web_server.eq_ignore_ascii_case("nginx") {
        "nginx"
    } else {
        "apache"
    };
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id);
    let service_running = service.is_some_and(|service| service.status == ServiceStatus::Running);
    push_check(
        &mut checks,
        "web-service",
        "Web service",
        if service_running { "ok" } else { "error" },
        service
            .map(|service| format!("{} is {:?}.", service.name, service.status))
            .unwrap_or_else(|| format!("{} service is not configured.", host.web_server)),
        service.and_then(|service| service.last_error.clone()),
        if service_running {
            None
        } else {
            Some(format!("Start {}.", host.web_server))
        },
    );

    let hosts_mapped = hosts_file_maps_domain(&host.domain);
    push_check(
        &mut checks,
        "hosts-file",
        "Windows hosts file",
        if hosts_mapped { "ok" } else { "error" },
        if hosts_mapped {
            format!("{} maps to a local address.", host.domain)
        } else {
            format!("{} is not mapped in the Windows hosts file.", host.domain)
        },
        None,
        if hosts_mapped {
            None
        } else {
            Some("Click Sync Hosts File and approve administrator access.".to_string())
        },
    );

    let proxy_bypassed = proxy_bypass_covers_domain(&host.domain);
    push_check(
        &mut checks,
        "proxy-bypass",
        "Windows proxy bypass",
        if proxy_bypassed { "ok" } else { "warning" },
        if proxy_bypassed {
            format!("{} is excluded from the Windows proxy.", host.domain)
        } else {
            format!(
                "{} is not excluded from the Windows proxy. Browser requests may return 503 from the proxy instead of reaching LocalStack.",
                host.domain
            )
        },
        read_proxy_override().ok(),
        if proxy_bypassed {
            None
        } else {
            Some("Open the host once or click Sync Hosts File to update proxy bypass.".to_string())
        },
    );

    let runtime = runtime_config_path(&store, service_id);
    let runtime_text = fs::read_to_string(&runtime).unwrap_or_default();
    let has_domain = runtime_text
        .lines()
        .any(|line| line.to_lowercase().contains(&host.domain.to_lowercase()));
    let has_doc_root = runtime_text.contains(&slash(&doc_root));
    let node_proxy_target = host
        .env_variables
        .get("LOCALSTACK_NODE_PORT")
        .map(|port| format!("127.0.0.1:{port}"));
    let runtime_ok = if let Some(target) = &node_proxy_target {
        has_domain && runtime_text.contains(target)
    } else {
        has_domain && has_doc_root
    };
    push_check(
        &mut checks,
        "runtime-config",
        "Runtime vhost config",
        if runtime_ok {
            "ok"
        } else {
            "error"
        },
        if runtime_ok {
            if let Some(target) = &node_proxy_target {
                format!("Runtime config contains {} and proxy target {}.", host.domain, target)
            } else {
                format!(
                    "Runtime config contains {} and its document root.",
                    host.domain
                )
            }
        } else {
            if let Some(target) = &node_proxy_target {
                format!("Runtime config is missing {} or proxy target {}.", host.domain, target)
            } else {
                format!(
                    "Runtime config is missing {} or {}.",
                    host.domain,
                    doc_root.display()
                )
            }
        },
        Some(runtime.display().to_string()),
        if runtime_ok {
            None
        } else {
            Some(format!(
                "Restart {} to regenerate runtime config.",
                host.web_server
            ))
        },
    );

    push_check(
        &mut checks,
        "http-port",
        "HTTP endpoint",
        if tcp_ready("127.0.0.1", host.http_port) {
            "ok"
        } else {
            "error"
        },
        if tcp_ready("127.0.0.1", host.http_port) {
            format!("127.0.0.1:{} accepts connections.", host.http_port)
        } else {
            format!("127.0.0.1:{} is not accepting connections.", host.http_port)
        },
        None,
        if tcp_ready("127.0.0.1", host.http_port) {
            None
        } else {
            Some(format!(
                "Start {} and check port conflicts.",
                host.web_server
            ))
        },
    );

    if host.ssl {
        let cert = snapshot
            .certificates
            .iter()
            .find(|cert| cert.domain.eq_ignore_ascii_case(&host.domain));
        let cert_files_ok = cert
            .map(|cert| Path::new(&cert.cert_path).is_file() && Path::new(&cert.key_path).is_file())
            .unwrap_or_else(|| {
                store
                    .dir
                    .join("certs")
                    .join(format!("{}.crt", host.domain))
                    .is_file()
                    && store
                        .dir
                        .join("keys")
                        .join(format!("{}.key", host.domain))
                        .is_file()
            });
        push_check(
            &mut checks,
            "ssl-files",
            "SSL certificate files",
            if cert_files_ok { "ok" } else { "error" },
            if cert_files_ok {
                "Certificate and key files are available.".to_string()
            } else {
                format!("Certificate or key for {} is missing.", host.domain)
            },
            cert.map(|cert| format!("{} | {}", cert.cert_path, cert.key_path)),
            if cert_files_ok {
                None
            } else {
                Some("Generate or repair the SSL certificate.".to_string())
            },
        );
        push_check(
            &mut checks,
            "https-port",
            "HTTPS endpoint",
            if tcp_ready("127.0.0.1", host.https_port) {
                "ok"
            } else {
                "error"
            },
            if tcp_ready("127.0.0.1", host.https_port) {
                format!("127.0.0.1:{} accepts HTTPS connections.", host.https_port)
            } else {
                format!(
                    "127.0.0.1:{} is not accepting HTTPS connections.",
                    host.https_port
                )
            },
            None,
            if tcp_ready("127.0.0.1", host.https_port) {
                None
            } else {
                Some("Restart Apache/Nginx after generating SSL.".to_string())
            },
        );
    }

    let php_configured = snapshot
        .php_versions
        .iter()
        .any(|php| php.version == host.php_version);
    let php_runtime = snapshot
        .php_versions
        .iter()
        .find(|php| php.default)
        .or_else(|| snapshot.php_versions.first())
        .map(|php| Path::new(&php.cli_path).exists())
        .unwrap_or(false);
    push_check(
        &mut checks,
        "php",
        "PHP runtime",
        if php_configured && php_runtime {
            "ok"
        } else {
            "warning"
        },
        if php_configured && php_runtime {
            format!(
                "PHP {} is configured and a PHP CLI path exists.",
                host.php_version
            )
        } else if php_configured {
            "Host PHP version exists, but PHP CLI path is not available.".to_string()
        } else {
            format!(
                "PHP {} is not configured in LocalStack Pro.",
                host.php_version
            )
        },
        None,
        if php_configured && php_runtime {
            None
        } else {
            Some("Open PHP page and detect or add a PHP version.".to_string())
        },
    );

    if !host.database.trim().is_empty() {
        let database = snapshot.databases.iter().find(|database| {
            database.id.eq_ignore_ascii_case(&host.database)
                || database.name.eq_ignore_ascii_case(&host.database)
        });
        let db_service_id = database
            .map(|database| match database.engine.as_str() {
                "PostgreSQL" => "postgresql",
                "MariaDB" => "mariadb",
                _ => "mysql",
            })
            .unwrap_or("mysql");
        let db_service_running = snapshot
            .services
            .iter()
            .any(|service| service.id == db_service_id && service.status == ServiceStatus::Running);
        push_check(
            &mut checks,
            "database",
            "Database mapping",
            if database.is_some() && db_service_running {
                "ok"
            } else {
                "warning"
            },
            if let Some(database) = database {
                format!(
                    "{} database {} is configured; service running: {}.",
                    database.engine, database.name, db_service_running
                )
            } else {
                format!(
                    "Host database {} is not present in LocalStack Pro state.",
                    host.database
                )
            },
            None,
            if database.is_some() && db_service_running {
                None
            } else {
                Some("Open Database page and start/create the database service.".to_string())
            },
        );
    }

    let logs_dir = PathBuf::from(&host.root_folder).join("logs");
    push_check(
        &mut checks,
        "logs",
        "Host logs folder",
        if logs_dir.is_dir() { "ok" } else { "warning" },
        if logs_dir.is_dir() {
            format!("{} exists.", logs_dir.display())
        } else {
            format!("{} does not exist.", logs_dir.display())
        },
        None,
        if logs_dir.is_dir() {
            None
        } else {
            Some("Restart the host service or open logs once to create the folder.".to_string())
        },
    );

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
        format!("{errors} blocking issue(s) found for {}.", host.domain)
    } else if warnings > 0 {
        format!("{} is usable with {warnings} warning(s).", host.domain)
    } else {
        format!("{} is healthy.", host.domain)
    };

    Ok(HostDiagnosticReport {
        host_id: host.id,
        domain: host.domain,
        generated_at: Utc::now().to_rfc3339(),
        summary,
        ok,
        warnings,
        errors,
        checks,
    })
}

pub fn repair_host(host_id: String) -> AppResult<HostDiagnosticReport> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let host = snapshot
        .hosts
        .iter()
        .find(|host| host.id == host_id || host.domain.eq_ignore_ascii_case(&host_id))
        .cloned()
        .ok_or_else(|| "Host not found.".to_string())?;
    store.ensure_host_files(&snapshot);
    write_host_environment_files(&host)?;
    configure_detected_cms(&host)?;
    write_host_config(&store, &host)?;
    if host.ssl {
        let cert_exists = snapshot.certificates.iter().any(|cert| {
            cert.domain.eq_ignore_ascii_case(&host.domain)
                && Path::new(&cert.cert_path).exists()
                && Path::new(&cert.key_path).exists()
        });
        if !cert_exists {
            let _ = crate::ssl::generate_certificate(
                host.domain.clone(),
                vec![host.domain.clone(), format!("www.{}", host.domain)],
            )?;
        }
    }
    if !hosts_file_maps_domain(&host.domain) {
        write_windows_hosts_file(&store, &snapshot)?;
        store.log(
            &mut snapshot,
            LogLevel::Info,
            "Hosts",
            format!("Windows hosts file was synchronized for {}", host.domain),
            None,
        );
        store.save(&snapshot)?;
    }
    sync_proxy_bypass_for_hosts(&snapshot)?;
    let service_id = if host.web_server.eq_ignore_ascii_case("nginx") {
        "nginx"
    } else {
        "apache"
    };
    crate::services::restart_service(service_id.to_string()).map_err(|err| {
        format!("Host files were repaired, but {service_id} restart failed: {err}")
    })?;
    diagnose_host(host.id)
}

pub fn delete_host(host_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let removed = snapshot
        .hosts
        .iter()
        .find(|host| host.id == host_id || host.domain.eq_ignore_ascii_case(&host_id))
        .cloned()
        .ok_or_else(|| format!("Host {host_id} not found."))?;
    let service_id = if removed.web_server.eq_ignore_ascii_case("nginx") {
        "nginx"
    } else {
        "apache"
    };
    let service_was_running = snapshot
        .services
        .iter()
        .any(|service| service.id == service_id && service.status == ServiceStatus::Running);
    snapshot
        .hosts
        .retain(|host| host.id != removed.id && !host.domain.eq_ignore_ascii_case(&removed.domain));
    snapshot
        .certificates
        .retain(|cert| !cert.domain.eq_ignore_ascii_case(&removed.domain));
    remove_host_sidecar_files(&store, &removed.domain);
    if let Err(err) = write_windows_hosts_file(&store, &snapshot) {
        store.log(
            &mut snapshot,
            LogLevel::Warning,
            "Hosts",
            format!(
                "Host {} was deleted, but Windows hosts file sync failed: {err}",
                removed.domain
            ),
            None,
        );
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Hosts",
        format!("Host {} deleted", removed.domain),
        None,
    );
    store.save(&snapshot)?;
    if service_was_running {
        match crate::services::restart_service(service_id.to_string()) {
            Ok(snapshot) => return Ok(snapshot),
            Err(err) => {
                let mut snapshot = store.load_static()?;
                store.log(
                    &mut snapshot,
                    LogLevel::Warning,
                    "Hosts",
                    format!(
                        "Host {} was deleted, but {service_id} restart failed: {err}",
                        removed.domain
                    ),
                    None,
                );
                store.save(&snapshot)?;
                return Ok(snapshot);
            }
        }
    }
    Ok(snapshot)
}

fn remove_host_sidecar_files(store: &Store, domain: &str) {
    let file_stem = domain.replace('*', "wildcard").replace(':', "_");
    let paths = [
        store.dir.join("hosts").join(format!("{domain}.json")),
        store
            .dir
            .join("configs")
            .join("apache")
            .join("vhosts")
            .join(format!("{domain}.conf")),
        store
            .dir
            .join("configs")
            .join("nginx")
            .join("vhosts")
            .join(format!("{domain}.conf")),
        store.dir.join("certs").join(format!("{file_stem}.crt")),
        store.dir.join("certs").join(format!("{file_stem}.pem")),
        store.dir.join("certs").join(format!("{file_stem}.issuer")),
        store.dir.join("keys").join(format!("{file_stem}.key")),
    ];
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

pub fn duplicate_host(host_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let source = snapshot
        .hosts
        .iter()
        .find(|host| host.id == host_id)
        .cloned()
        .ok_or_else(|| "Host not found.".to_string())?;
    let mut copy = source;
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.domain = format!("copy-{}", copy.domain);
    copy.created_at = Utc::now().to_rfc3339();
    copy.updated_at = copy.created_at.clone();
    snapshot.hosts.push(copy);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn import_hosts(path: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let source = resolve_data_path(&store, &path);
    if !source.exists() {
        let mut snapshot = store.load_static()?;
        if let Some(parent) = source.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("Cannot create import folder {}: {err}", parent.display())
            })?;
        }
        let text = serde_json::to_string_pretty(&snapshot.hosts)
            .map_err(|err| format!("Cannot serialize current hosts: {err}"))?;
        fs::write(&source, text).map_err(|err| {
            format!(
                "Cannot create hosts import file {}: {err}",
                source.display()
            )
        })?;
        store.log(
            &mut snapshot,
            LogLevel::Warning,
            "Hosts",
            format!(
                "Hosts import file was missing, so LocalStack Pro created {}",
                source.display()
            ),
            None,
        );
        store.save(&snapshot)?;
        return Ok(snapshot);
    }
    let text = fs::read_to_string(&source)
        .map_err(|err| format!("Cannot read hosts JSON {}: {err}", source.display()))?;
    let hosts: Vec<HostInfo> =
        serde_json::from_str(&text).map_err(|err| format!("Cannot parse hosts JSON: {err}"))?;
    for host in &hosts {
        validate_host(host)?;
    }
    let mut snapshot = store.load_static()?;
    snapshot.hosts = hosts;
    store.ensure_host_files(&snapshot);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn export_hosts(path: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let target = if path.ends_with(".json") {
        resolve_data_path(&store, &path)
    } else {
        store.dir.join("hosts-export.json")
    };
    let text = serde_json::to_string_pretty(&snapshot.hosts)
        .map_err(|err| format!("Cannot serialize hosts: {err}"))?;
    fs::write(&target, text).map_err(|err| format!("Cannot export hosts: {err}"))?;
    Ok(target.display().to_string())
}

pub fn sync_hosts_file() -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    write_windows_hosts_file(&store, &snapshot)?;
    sync_proxy_bypass_for_hosts(&snapshot)?;
    let missing = snapshot
        .hosts
        .iter()
        .filter(|host| !hosts_file_maps_domain(&host.domain))
        .map(|host| host.domain.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "Windows hosts file was updated, but these domains are still not mapped: {}",
            missing.join(", ")
        ));
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Hosts",
        "Windows hosts file was synchronized",
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub(crate) fn sync_proxy_bypass_for_hosts(snapshot: &crate::state::AppSnapshot) -> AppResult<()> {
    let mut required = snapshot
        .hosts
        .iter()
        .map(|host| host.domain.trim().to_lowercase())
        .filter(|domain| !domain.is_empty())
        .collect::<Vec<_>>();
    if required.iter().any(|domain| domain.ends_with(".test")) {
        required.push("*.test".to_string());
    }
    required.push("localhost".to_string());
    required.push("127.*".to_string());
    required.push("<local>".to_string());
    required.sort();
    required.dedup();

    let current = read_proxy_override().unwrap_or_default();
    let mut entries = current
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut changed = false;
    for domain in required {
        if !entries
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(&domain))
        {
            entries.push(domain);
            changed = true;
        }
    }
    if !changed {
        sync_no_proxy_environment(snapshot)?;
        return Ok(());
    }
    write_proxy_override(&entries.join(";"))?;
    sync_no_proxy_environment(snapshot)?;
    notify_proxy_settings_changed();
    Ok(())
}

pub(crate) fn hosts_file_maps_domain(domain: &str) -> bool {
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

fn write_windows_hosts_file(store: &Store, snapshot: &crate::state::AppSnapshot) -> AppResult<()> {
    let hosts_path = windows_hosts_path()?;
    let content = build_hosts_file_content(&hosts_path, snapshot)?;
    match write_hosts_direct(&hosts_path, &content) {
        Ok(()) => {
            flush_dns_cache();
            return Ok(());
        }
        Err(_) => write_hosts_elevated(store, &hosts_path, &content)?,
    }
    flush_dns_cache();
    Ok(())
}

fn read_proxy_override() -> AppResult<String> {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyOverride",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("Cannot read Windows proxy bypass list: {err}"))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if line.contains("ProxyOverride") {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() >= 3 {
                return Ok(parts[2..].join(" "));
            }
        }
    }
    Ok(String::new())
}

fn proxy_bypass_covers_domain(domain: &str) -> bool {
    if !windows_proxy_enabled() {
        return true;
    }
    let normalized = domain.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }
    let Ok(override_text) = read_proxy_override() else {
        return false;
    };
    override_text
        .split(';')
        .map(|entry| entry.trim().to_lowercase())
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            entry == normalized
                || (entry == "*.test" && normalized.ends_with(".test"))
                || (entry == "<local>" && !normalized.contains('.'))
        })
}

fn windows_proxy_enabled() -> bool {
    let Ok(output) = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyEnable",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.contains("ProxyEnable") && line.split_whitespace().last() == Some("0x1"))
}

fn write_proxy_override(value: &str) -> AppResult<()> {
    let status = Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            "/v",
            "ProxyOverride",
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|err| format!("Cannot update Windows proxy bypass list: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot update Windows proxy bypass list. Exit code: {:?}",
            status.code()
        ))
    }
}

fn sync_no_proxy_environment(snapshot: &crate::state::AppSnapshot) -> AppResult<()> {
    let mut entries = snapshot
        .hosts
        .iter()
        .map(|host| host.domain.trim().to_lowercase())
        .filter(|domain| !domain.is_empty())
        .collect::<Vec<_>>();
    if entries.iter().any(|domain| domain.ends_with(".test")) {
        entries.push("*.test".to_string());
        entries.push(".test".to_string());
    }
    entries.extend([
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "127.*".to_string(),
    ]);
    entries.sort();
    entries.dedup();
    let value = entries.join(",");
    std::env::set_var("NO_PROXY", &value);
    std::env::set_var("no_proxy", &value);
    write_user_environment_value("NO_PROXY", &value)?;
    write_user_environment_value("no_proxy", &value)?;
    Ok(())
}

fn write_user_environment_value(name: &str, value: &str) -> AppResult<()> {
    let status = Command::new("reg")
        .args([
            "add",
            r"HKCU\Environment",
            "/v",
            name,
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|err| format!("Cannot update {name}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot update {name}. Exit code: {:?}",
            status.code()
        ))
    }
}

fn notify_proxy_settings_changed() {
    let script = r#"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class LocalStackWinInet {
    [DllImport("wininet.dll", SetLastError=true)]
    public static extern bool InternetSetOption(IntPtr hInternet, int dwOption, IntPtr lpBuffer, int dwBufferLength);
}
"@
[LocalStackWinInet]::InternetSetOption([IntPtr]::Zero, 39, [IntPtr]::Zero, 0) | Out-Null
[LocalStackWinInet]::InternetSetOption([IntPtr]::Zero, 37, [IntPtr]::Zero, 0) | Out-Null
"#;
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn try_write_windows_hosts_file_direct(
    _store: &Store,
    snapshot: &crate::state::AppSnapshot,
) -> AppResult<()> {
    let hosts_path = windows_hosts_path()?;
    let content = build_hosts_file_content(&hosts_path, snapshot)?;
    write_hosts_direct(&hosts_path, &content)?;
    flush_dns_cache();
    Ok(())
}

fn windows_hosts_path() -> AppResult<PathBuf> {
    let windir = std::env::var("WINDIR")
        .map_err(|_| "WINDIR is not available. Cannot locate Windows hosts file.".to_string())?;
    Ok(Path::new(&windir)
        .join("System32")
        .join("drivers")
        .join("etc")
        .join("hosts"))
}

fn build_hosts_file_content(
    hosts_path: &Path,
    snapshot: &crate::state::AppSnapshot,
) -> AppResult<String> {
    let existing = fs::read_to_string(hosts_path).unwrap_or_default();
    let entries = snapshot
        .hosts
        .iter()
        .map(|host| host.domain.trim().to_lowercase())
        .filter(|domain| !domain.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|domain| format!("127.0.0.1 {domain}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    let block = format!("{HOSTS_BEGIN}\r\n{entries}\r\n{HOSTS_END}");
    let normalized = existing.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut inside = false;
    let mut replaced = false;
    for line in normalized.lines() {
        if line.trim() == HOSTS_BEGIN {
            if !replaced {
                lines.push(block.clone());
                replaced = true;
            }
            inside = true;
            continue;
        }
        if inside {
            if line.trim() == HOSTS_END {
                inside = false;
            }
            continue;
        }
        lines.push(line.to_string());
    }
    if !replaced {
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
        lines.push(block);
    }
    let mut content = lines.join("\r\n");
    content.push_str("\r\n");
    Ok(content)
}

fn write_hosts_direct(hosts_path: &Path, content: &str) -> AppResult<()> {
    fs::write(hosts_path, content)
        .map_err(|err| format!("Cannot write Windows hosts file directly: {err}"))?;
    Ok(())
}

fn write_hosts_elevated(store: &Store, hosts_path: &Path, content: &str) -> AppResult<()> {
    let source = store.dir.join("sync-hosts-content.txt");
    let result = store.dir.join("sync-hosts-result.txt");
    fs::write(&source, content)
        .map_err(|err| format!("Cannot create hosts sync content file: {err}"))?;
    let _ = fs::remove_file(&result);
    let command = format!(
        "/C copy /Y \"{}\" \"{}\" >NUL && ipconfig /flushdns >NUL && echo OK>\"{}\"",
        source.display(),
        hosts_path.display(),
        result.display()
    );
    #[cfg(windows)]
    run_elevated_hidden("cmd.exe", &command)
        .map_err(|err| format!("Cannot request administrator rights for hosts file: {err}"))?;
    if fs::read_to_string(&result).unwrap_or_default().trim() != "OK" {
        return Err("Windows hosts-file sync failed or was cancelled.".to_string());
    }
    Ok(())
}

fn flush_dns_cache() {
    let mut command = Command::new("ipconfig");
    command
        .arg("/flushdns")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let _ = command.status();
}

#[cfg(windows)]
fn run_elevated_hidden(executable: &str, args: &str) -> AppResult<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND};
    use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = wide("runas");
    let file = wide(executable);
    let params = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: 0 as HWND,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        lpDirectory: std::ptr::null(),
        nShow: SW_HIDE,
        hInstApp: std::ptr::null_mut(),
        lpIDList: std::ptr::null_mut(),
        lpClass: std::ptr::null(),
        hkeyClass: std::ptr::null_mut(),
        dwHotKey: 0,
        Anonymous: unsafe { std::mem::zeroed() },
        hProcess: 0 as HANDLE,
    };
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        return Err(format!("Shell elevation failed: {}", unsafe {
            GetLastError()
        }));
    }
    if !info.hProcess.is_null() {
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn push_check(
    checks: &mut Vec<HostDiagnosticCheck>,
    id: &str,
    title: &str,
    severity: &str,
    message: String,
    detail: Option<String>,
    action: Option<String>,
) {
    checks.push(HostDiagnosticCheck {
        id: id.to_string(),
        title: title.to_string(),
        severity: severity.to_string(),
        message,
        detail,
        action,
    });
}

fn runtime_config_path(store: &Store, service_id: &str) -> PathBuf {
    if service_id == "nginx" {
        store
            .dir
            .join("configs")
            .join("nginx-runtime")
            .join("conf")
            .join("nginx.conf")
    } else {
        store
            .dir
            .join("configs")
            .join("apache-runtime")
            .join("httpd.conf")
    }
}

fn tcp_ready(host: &str, port: u16) -> bool {
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    let addresses = addresses.collect::<Vec<_>>();
    for _ in 0..3 {
        if addresses
            .iter()
            .any(|address| TcpStream::connect_timeout(address, Duration::from_millis(160)).is_ok())
        {
            return true;
        }
    }
    false
}

fn slash(path: &Path) -> String {
    path.display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

fn validate_host(host: &HostInfo) -> AppResult<()> {
    if !host.domain.contains('.') || host.domain.contains(' ') {
        return Err("Domain must be a valid local domain like shop.test.".to_string());
    }
    if host.root_folder.trim().is_empty() {
        return Err("Root folder is required.".to_string());
    }
    if host.document_root.trim().is_empty() {
        return Err("Document root is required.".to_string());
    }
    Ok(())
}

fn normalize_document_root(host: &mut HostInfo) {
    if Path::new(&host.document_root).is_absolute() {
        return;
    }
    let current = host.document_root.trim().replace('/', "\\");
    if current != "public" {
        return;
    }
    let root = PathBuf::from(&host.root_folder);
    let public = root.join("public");
    if has_index_file(&public) {
        return;
    }
    for candidate in ["www", "htdocs", "web"] {
        if has_index_file(&root.join(candidate)) {
            host.document_root = candidate.to_string();
            return;
        }
    }
    if has_index_file(&root) {
        host.document_root = ".".to_string();
    }
}

fn has_index_file(path: &Path) -> bool {
    ["index.php", "index.html", "index.htm"]
        .iter()
        .any(|name| path.join(name).is_file())
}

fn write_host_config(store: &Store, host: &HostInfo) -> AppResult<()> {
    let path = store
        .dir
        .join("hosts")
        .join(format!("{}.json", host.domain));
    let text = serde_json::to_string_pretty(host)
        .map_err(|err| format!("Cannot serialize host config: {err}"))?;
    fs::write(path, text).map_err(|err| format!("Cannot write host config: {err}"))?;
    write_vhost_snippets(store, host)
}

fn create_host_project_files(host: &HostInfo) -> AppResult<()> {
    let root = PathBuf::from(&host.root_folder);
    let doc_root = document_root(host);
    let logs = root.join("logs");
    fs::create_dir_all(&root)
        .map_err(|err| format!("Cannot create project folder {}: {err}", root.display()))?;
    fs::create_dir_all(&doc_root)
        .map_err(|err| format!("Cannot create document root {}: {err}", doc_root.display()))?;
    fs::create_dir_all(&logs)
        .map_err(|err| format!("Cannot create logs folder {}: {err}", logs.display()))?;
    let index_html = doc_root.join("index.html");
    let index_php = doc_root.join("index.php");
    if !index_html.exists() && !index_php.exists() {
        let body = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title></head><body><h1>{}</h1><p>LocalStack Pro host is ready.</p></body></html>",
            host.domain, host.domain
        );
        fs::write(&index_html, body).map_err(|err| {
            format!(
                "Cannot create host index file {}: {err}",
                index_html.display()
            )
        })?;
    }
    let error_log = logs.join("error.log");
    if !error_log.exists() {
        fs::write(&error_log, "").map_err(|err| {
            format!(
                "Cannot create host error log {}: {err}",
                error_log.display()
            )
        })?;
    }
    let access_log = logs.join("access.log");
    if !access_log.exists() {
        fs::write(&access_log, "").map_err(|err| {
            format!(
                "Cannot create host access log {}: {err}",
                access_log.display()
            )
        })?;
    }
    Ok(())
}

fn apply_database_environment(host: &mut HostInfo, snapshot: &crate::state::AppSnapshot) {
    let Some(database) = snapshot.databases.iter().find(|database| {
        database.id.eq_ignore_ascii_case(&host.database)
            || database.name.eq_ignore_ascii_case(&host.database)
    }) else {
        return;
    };
    host.database = database.name.clone();
    let host_name = database_host_only(&database.engine).to_string();
    let port = database.port.to_string();
    host.env_variables.insert(
        "DB_CONNECTION".to_string(),
        database_connection(&database.engine).to_string(),
    );
    host.env_variables
        .insert("DB_HOST".to_string(), host_name.clone());
    host.env_variables
        .insert("DB_PORT".to_string(), port.clone());
    host.env_variables
        .insert("DB_DATABASE".to_string(), database.name.clone());
    host.env_variables
        .insert("DB_NAME".to_string(), database.name.clone());
    host.env_variables
        .insert("DB_USERNAME".to_string(), database.user.clone());
    host.env_variables
        .insert("DB_USER".to_string(), database.user.clone());
    host.env_variables
        .insert("DB_PASSWORD".to_string(), database.password.clone());
    host.env_variables
        .insert("DB_PASS".to_string(), database.password.clone());
    host.env_variables
        .insert("MYSQL_HOST".to_string(), host_name.clone());
    host.env_variables
        .insert("MYSQL_PORT".to_string(), port.clone());
    host.env_variables
        .insert("MYSQL_DATABASE".to_string(), database.name.clone());
    host.env_variables
        .insert("MYSQL_USER".to_string(), database.user.clone());
    host.env_variables
        .insert("MYSQL_PASSWORD".to_string(), database.password.clone());
    host.env_variables.insert(
        "DATABASE_URL".to_string(),
        format!(
            "{}://{}:{}@{}:{}/{}",
            if database.engine == "PostgreSQL" {
                "postgresql"
            } else {
                "mysql"
            },
            database.user,
            database.password,
            host_name,
            port,
            database.name
        ),
    );
}

fn write_host_environment_files(host: &HostInfo) -> AppResult<()> {
    if host.env_variables.is_empty() {
        return Ok(());
    }
    write_env_file(
        &PathBuf::from(&host.root_folder).join(".env"),
        &host.env_variables,
    )
}

fn write_env_file(
    path: &Path,
    values: &std::collections::HashMap<String, String>,
) -> AppResult<()> {
    let mut merged = BTreeMap::new();
    let existing = fs::read_to_string(path).unwrap_or_default();
    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || !trimmed.contains('=') {
            continue;
        }
        let mut parts = trimmed.splitn(2, '=');
        let key = parts.next().unwrap_or_default().trim();
        let value = parts.next().unwrap_or_default().trim();
        if !key.is_empty() {
            merged.insert(key.to_string(), value.to_string());
        }
    }
    for (key, value) in values {
        merged.insert(key.clone(), env_escape(value));
    }
    let content = merged
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    fs::write(path, format!("{content}\r\n"))
        .map_err(|err| format!("Cannot write environment file {}: {err}", path.display()))
}

fn configure_detected_cms(host: &HostInfo) -> AppResult<()> {
    let Some(database) = database_from_host(host) else {
        return Ok(());
    };
    let public = document_root(host);
    if public.join("wp-admin").is_dir()
        || public.join("wp-config-sample.php").is_file()
        || public.join("wp-config.php").is_file()
    {
        write_wordpress_config(&public, &database)?;
    }
    if public.join("configuration.php").is_file() && !public.join("installation").is_dir() {
        write_joomla_config(&public, &database, host)?;
    }
    if public.join("sites").join("default").is_dir() || public.join("core").is_dir() {
        write_drupal_config(&public, &database)?;
    }
    write_generic_php_database_configs(&public, &database)?;
    write_generic_php_installer_defaults(&public, &database)?;
    write_localstack_database_helper(&public, &database)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct HostDatabaseConfig {
    name: String,
    user: String,
    password: String,
    engine: String,
    port: u16,
}

fn database_from_host(host: &HostInfo) -> Option<HostDatabaseConfig> {
    let name = host
        .env_variables
        .get("DB_DATABASE")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| (!host.database.trim().is_empty()).then(|| host.database.clone()))?;
    let user = host
        .env_variables
        .get("DB_USERNAME")
        .cloned()
        .filter(|value| !value.trim().is_empty())?;
    let password = host
        .env_variables
        .get("DB_PASSWORD")
        .cloned()
        .unwrap_or_default();
    let connection = host
        .env_variables
        .get("DB_CONNECTION")
        .map(|value| value.to_lowercase())
        .unwrap_or_else(|| "mysql".to_string());
    let engine = if connection.contains("pgsql") || connection.contains("postgres") {
        "PostgreSQL"
    } else {
        "MySQL"
    }
    .to_string();
    let port = host
        .env_variables
        .get("DB_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or_else(|| database_port(&engine));
    Some(HostDatabaseConfig {
        name,
        user,
        password,
        engine,
        port,
    })
}

fn write_wordpress_config(public: &Path, database: &HostDatabaseConfig) -> AppResult<()> {
    let sample = public.join("wp-config-sample.php");
    let target = public.join("wp-config.php");
    let mut config = if target.exists() {
        fs::read_to_string(&target).map_err(|err| format!("Cannot read WordPress config: {err}"))?
    } else if sample.exists() {
        fs::read_to_string(&sample)
            .map_err(|err| format!("Cannot read WordPress config sample: {err}"))?
    } else {
        return Ok(());
    };
    config = config
        .replace("database_name_here", &database.name)
        .replace("username_here", &database.user)
        .replace("password_here", &database.password)
        .replace("localhost", &database_host(database));
    config = set_php_define(&config, "DB_NAME", &database.name);
    config = set_php_define(&config, "DB_USER", &database.user);
    config = set_php_define(&config, "DB_PASSWORD", &database.password);
    config = set_php_define(&config, "DB_HOST", &database_host(database));
    fs::write(target, config).map_err(|err| format!("Cannot write WordPress config: {err}"))
}

fn write_joomla_config(
    public: &Path,
    database: &HostDatabaseConfig,
    host: &HostInfo,
) -> AppResult<()> {
    let log_path = public.join("administrator").join("logs");
    let tmp_path = public.join("tmp");
    fs::create_dir_all(&log_path)
        .map_err(|err| format!("Cannot create Joomla log folder: {err}"))?;
    fs::create_dir_all(&tmp_path)
        .map_err(|err| format!("Cannot create Joomla temp folder: {err}"))?;
    let content = format!(
        "<?php\nclass JConfig {{\n\tpublic $offline = false;\n\tpublic $sitename = '{}';\n\tpublic $dbtype = '{}';\n\tpublic $host = '{}';\n\tpublic $user = '{}';\n\tpublic $password = '{}';\n\tpublic $db = '{}';\n\tpublic $dbprefix = 'lsp_';\n\tpublic $live_site = '{}://{}';\n\tpublic $secret = '{}';\n\tpublic $log_path = '{}';\n\tpublic $tmp_path = '{}';\n\tpublic $session_handler = 'database';\n}}\n",
        php_escape(&host.domain),
        php_escape(if database.engine == "PostgreSQL" { "pgsql" } else { "mysqli" }),
        php_escape(&database_host(database)),
        php_escape(&database.user),
        php_escape(&database.password),
        php_escape(&database.name),
        if host.ssl { "https" } else { "http" },
        php_escape(&host.domain),
        php_escape(&uuid::Uuid::new_v4().simple().to_string()),
        php_escape(&log_path.display().to_string()),
        php_escape(&tmp_path.display().to_string())
    );
    fs::write(public.join("configuration.php"), content)
        .map_err(|err| format!("Cannot write Joomla configuration.php: {err}"))
}

fn write_drupal_config(public: &Path, database: &HostDatabaseConfig) -> AppResult<()> {
    let default_dir = public.join("sites").join("default");
    fs::create_dir_all(default_dir.join("files"))
        .map_err(|err| format!("Cannot create Drupal files folder: {err}"))?;
    let target = default_dir.join("settings.php");
    let mut config = if target.exists() {
        fs::read_to_string(&target)
            .map_err(|err| format!("Cannot read Drupal settings.php: {err}"))?
    } else {
        let sample = default_dir.join("default.settings.php");
        if sample.exists() {
            fs::read_to_string(&sample)
                .map_err(|err| format!("Cannot read Drupal default.settings.php: {err}"))?
        } else {
            "<?php\n".to_string()
        }
    };
    config.push_str(&format!(
        "\n$databases['default']['default'] = [\n  'database' => '{}',\n  'username' => '{}',\n  'password' => '{}',\n  'prefix' => '',\n  'host' => '{}',\n  'port' => '{}',\n  'namespace' => 'Drupal\\\\Core\\\\Database\\\\Driver\\\\{}',\n  'driver' => '{}',\n];\n$settings['hash_salt'] = '{}';\n$settings['file_public_path'] = 'sites/default/files';\n",
        php_escape(&database.name),
        php_escape(&database.user),
        php_escape(&database.password),
        database_host_only(&database.engine),
        database.port,
        drupal_driver(&database.engine),
        drupal_driver(&database.engine),
        php_escape(&uuid::Uuid::new_v4().to_string())
    ));
    fs::write(target, config).map_err(|err| format!("Cannot write Drupal settings.php: {err}"))
}

fn write_generic_php_database_configs(
    public: &Path,
    database: &HostDatabaseConfig,
) -> AppResult<()> {
    let candidates = [
        public.join("config.php"),
        public.join("settings.php"),
        public.join("database.php"),
        public.join("db.php"),
        public.join("config").join("config.php"),
        public.join("config").join("database.php"),
        public.join("app").join("config").join("config.php"),
        public.join("app").join("config").join("database.php"),
        public.join("includes").join("config.php"),
        public.join("includes").join("database.php"),
    ];
    for path in candidates {
        if path.is_file() {
            patch_generic_php_config(&path, database)?;
        }
    }
    Ok(())
}

fn write_generic_php_installer_defaults(
    public: &Path,
    database: &HostDatabaseConfig,
) -> AppResult<()> {
    let candidates = [
        public.join("install.php"),
        public.join("setup.php"),
        public.join("installer.php"),
        public.join("install").join("index.php"),
        public.join("setup").join("index.php"),
    ];
    for path in candidates {
        if path.is_file() {
            patch_generic_php_installer(&path, database)?;
        }
    }
    Ok(())
}

fn patch_generic_php_installer(path: &Path, database: &HostDatabaseConfig) -> AppResult<()> {
    let original = fs::read_to_string(path)
        .map_err(|err| format!("Cannot read PHP installer {}: {err}", path.display()))?;
    let mut text = original.clone();
    for key in ["db_host", "database_host", "mysql_host", "host"] {
        text = replace_php_call_default(&text, "field", key, &database_host_for_form(database));
        text = replace_php_call_default(
            &text,
            "install_post",
            key,
            &database_host_for_form(database),
        );
        text = replace_php_input_value(&text, key, &database_host_for_form(database));
    }
    for key in [
        "db_name",
        "database_name",
        "mysql_database",
        "database",
        "dbname",
    ] {
        text = replace_php_call_default(&text, "field", key, &database.name);
        text = replace_php_call_default(&text, "install_post", key, &database.name);
        text = replace_php_input_value(&text, key, &database.name);
    }
    for key in ["db_user", "database_user", "mysql_user", "username", "user"] {
        text = replace_php_call_default(&text, "field", key, &database.user);
        text = replace_php_call_default(&text, "install_post", key, &database.user);
        text = replace_php_input_value(&text, key, &database.user);
    }
    for key in [
        "db_pass",
        "db_password",
        "database_password",
        "mysql_password",
        "password",
        "pass",
    ] {
        text = replace_php_call_default(&text, "field", key, &database.password);
        text = replace_php_call_default(&text, "install_post", key, &database.password);
        text = replace_php_input_value(&text, key, &database.password);
    }
    text = inject_localstack_php_post_defaults(&text);
    text = inject_localstack_pdo_fallbacks(&text);
    text = append_localstack_installer_autofill(&text, database);
    if text != original {
        fs::write(path, text)
            .map_err(|err| format!("Cannot update PHP installer {}: {err}", path.display()))?;
    }
    Ok(())
}

fn inject_localstack_php_post_defaults(text: &str) -> String {
    let marker = "LocalStack Pro PHP database POST defaults";
    if text.contains(marker) {
        return text.to_string();
    }
    let snippet = format!(
        "\n/* {marker} */\nif (is_file(__DIR__.'/localstack-database.php')) {{\n  $lspDb = @require __DIR__.'/localstack-database.php';\n  if (is_array($lspDb)) {{\n    $_POST['db_host'] = (string)($lspDb['host'] ?? '127.0.0.1');\n    if (str_contains($_POST['db_host'], ':')) $_POST['db_host'] = explode(':', $_POST['db_host'], 2)[0];\n    $_POST['db_name'] = (string)($lspDb['database'] ?? '');\n    $_POST['db_user'] = (string)($lspDb['username'] ?? '');\n    $_POST['db_pass'] = (string)($lspDb['password'] ?? '');\n  }}\n}}\n"
    );
    if let Some(position) = text.find("<?php") {
        let insert_at = position + "<?php".len();
        format!("{}{}{}", &text[..insert_at], snippet, &text[insert_at..])
    } else {
        format!("<?php{snippet}?>\n{text}")
    }
}

fn inject_localstack_pdo_fallbacks(text: &str) -> String {
    let marker = "LocalStack Pro PDO fallback";
    if text.contains(marker) {
        return text.to_string();
    }
    let needle = "} catch (PDOException $e) {";
    let Some(position) = text.find(needle) else {
        return text.to_string();
    };
    let insert_at = position + needle.len();
    let snippet = format!(
        "\n    /* {marker} */\n    if (is_file(__DIR__.'/localstack-database.php')) {{\n      $lspDb = @require __DIR__.'/localstack-database.php';\n      if (is_array($lspDb)) {{\n        $lspHost = (string)($lspDb['host'] ?? '127.0.0.1');\n        if (str_contains($lspHost, ':')) $lspHost = explode(':', $lspHost, 2)[0];\n        $lspName = (string)($lspDb['database'] ?? '');\n        $lspUser = (string)($lspDb['username'] ?? '');\n        $lspPass = (string)($lspDb['password'] ?? '');\n        if ($lspName !== '' && $lspUser !== '') {{\n          $dsn = 'mysql:host='.$lspHost.';dbname='.$lspName.';charset=utf8mb4';\n          return new PDO($dsn, $lspUser, $lspPass, [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC, PDO::MYSQL_ATTR_INIT_COMMAND => 'SET NAMES utf8mb4']);\n        }}\n      }}\n    }}\n"
    );
    format!("{}{}{}", &text[..insert_at], snippet, &text[insert_at..])
}

fn append_localstack_installer_autofill(text: &str, database: &HostDatabaseConfig) -> String {
    let marker = "LocalStack Pro database autofill";
    let script = format!(
        "<script>\n/* {marker} */\n(function(){{\n  var db={{host:'{}',name:'{}',user:'{}',password:'{}'}};\n  var names={{host:['db_host','database_host','mysql_host','host'],name:['db_name','database_name','mysql_database','database','dbname'],user:['db_user','database_user','mysql_user','username','user'],password:['db_pass','db_password','database_password','mysql_password','password','pass']}};\n  function fill(list,value){{list.forEach(function(name){{document.querySelectorAll('[name=\"'+name+'\"]').forEach(function(input){{input.value=value; input.setAttribute('value',value); input.dispatchEvent(new Event('input',{{bubbles:true}})); input.dispatchEvent(new Event('change',{{bubbles:true}}));}});}});}}\n  function apply(){{fill(names.host,db.host); fill(names.name,db.name); fill(names.user,db.user); fill(names.password,db.password);}}\n  if(document.readyState==='loading') document.addEventListener('DOMContentLoaded',apply); else apply();\n  document.addEventListener('submit',apply,true);\n  document.addEventListener('click',function(event){{if(event.target&&event.target.closest('button,input[type=\"submit\"],[data-next],[data-install]')) apply();}},true);\n}})();\n</script>",
        html_escape(&database_host_for_form(database)),
        html_escape(&database.name),
        html_escape(&database.user),
        html_escape(&database.password)
    );
    if text.contains(marker) {
        return text.to_string();
    }
    if let Some(position) = text.rfind("</body>") {
        let mut output = String::with_capacity(text.len() + script.len() + 2);
        output.push_str(&text[..position]);
        output.push_str(&script);
        output.push('\n');
        output.push_str(&text[position..]);
        output
    } else {
        format!("{text}\n{script}\n")
    }
}

fn replace_php_call_default(text: &str, function: &str, key: &str, value: &str) -> String {
    let mut next = replace_php_call_default_with_quotes(text, function, key, value, '\'', '\'');
    next = replace_php_call_default_with_quotes(&next, function, key, value, '"', '"');
    next
}

fn replace_php_call_default_with_quotes(
    text: &str,
    function: &str,
    key: &str,
    value: &str,
    quote: char,
    value_quote: char,
) -> String {
    let needle = format!("{function}({quote}{key}{quote},{value_quote}");
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find(&needle) {
        output.push_str(&rest[..position]);
        output.push_str(&needle);
        output.push_str(&php_escape(value));
        let after = &rest[position + needle.len()..];
        if let Some(end) = find_unescaped_quote(after, value_quote) {
            rest = &after[end..];
        } else {
            rest = after;
        }
    }
    output.push_str(rest);
    output
}

fn replace_php_input_value(text: &str, key: &str, value: &str) -> String {
    text.lines()
        .map(|line| {
            if !line.contains("name=") || !line.contains(key) || !line.contains("value=") {
                return line.to_string();
            }
            replace_html_value_attribute(line, value)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn replace_html_value_attribute(line: &str, value: &str) -> String {
    let Some(value_pos) = line.find("value=") else {
        return line.to_string();
    };
    let quote_start = value_pos + "value=".len();
    let Some(quote) = line[quote_start..].chars().next() else {
        return line.to_string();
    };
    if quote != '"' && quote != '\'' {
        return line.to_string();
    }
    let content_start = quote_start + quote.len_utf8();
    let Some(content_end_rel) = find_unescaped_quote(&line[content_start..], quote) else {
        return line.to_string();
    };
    let content_end = content_start + content_end_rel;
    format!(
        "{}{}{}",
        &line[..content_start],
        html_escape(value),
        &line[content_end..]
    )
}

fn find_unescaped_quote(text: &str, quote: char) -> Option<usize> {
    let mut escaped = false;
    for (index, ch) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(index);
        }
    }
    None
}

fn patch_generic_php_config(path: &Path, database: &HostDatabaseConfig) -> AppResult<()> {
    let original = fs::read_to_string(path)
        .map_err(|err| format!("Cannot read PHP database config {}: {err}", path.display()))?;
    let mut text = original.clone();
    let replacements = [
        ("DB_HOST", database_host(database)),
        ("DB_PORT", database.port.to_string()),
        ("DB_DATABASE", database.name.clone()),
        ("DB_NAME", database.name.clone()),
        ("DB_USERNAME", database.user.clone()),
        ("DB_USER", database.user.clone()),
        ("DB_PASSWORD", database.password.clone()),
        ("DB_PASS", database.password.clone()),
        ("MYSQL_HOST", database_host(database)),
        ("MYSQL_PORT", database.port.to_string()),
        ("MYSQL_DATABASE", database.name.clone()),
        ("MYSQL_USER", database.user.clone()),
        ("MYSQL_PASSWORD", database.password.clone()),
    ];
    for (key, value) in replacements {
        text = replace_php_define(&text, key, &value);
        text = replace_php_variable(&text, key, &value);
        text = replace_php_array_value(&text, key, &value);
    }
    for (key, value) in [
        ("host", database_host(database)),
        ("hostname", database_host(database)),
        ("port", database.port.to_string()),
        ("database", database.name.clone()),
        ("dbname", database.name.clone()),
        ("name", database.name.clone()),
        ("username", database.user.clone()),
        ("user", database.user.clone()),
        ("password", database.password.clone()),
        ("pass", database.password.clone()),
    ] {
        text = replace_php_array_value(&text, key, &value);
        text = replace_php_variable(&text, key, &value);
    }
    if text != original {
        fs::write(path, text).map_err(|err| {
            format!(
                "Cannot update PHP database config {}: {err}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn replace_php_define(text: &str, key: &str, value: &str) -> String {
    let mut output = Vec::new();
    let escaped = php_escape(value);
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&format!("define('{key}'"))
            || trimmed.starts_with(&format!("define( '{key}'"))
            || trimmed.starts_with(&format!("defined('{key}'"))
        {
            output.push(format!("define( '{key}', '{escaped}' );"));
        } else {
            output.push(line.to_string());
        }
    }
    output.join("\n")
}

fn replace_php_variable(text: &str, key: &str, value: &str) -> String {
    let names = [
        key.to_lowercase(),
        key.to_uppercase(),
        key.to_lowercase().replace("db_", "db"),
    ];
    let escaped = php_escape(value);
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            for name in &names {
                let prefix = format!("${name}");
                if trimmed.starts_with(&prefix) && trimmed.contains('=') {
                    let indent = &line[..line.len() - trimmed.len()];
                    return format!("{indent}{prefix} = '{escaped}';");
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn replace_php_array_value(text: &str, key: &str, value: &str) -> String {
    let escaped = php_escape(value);
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let quoted_single = format!("'{key}'");
            let quoted_double = format!("\"{key}\"");
            if (trimmed.starts_with(&quoted_single) || trimmed.starts_with(&quoted_double))
                && trimmed.contains("=>")
            {
                let indent = &line[..line.len() - trimmed.len()];
                let comma = if trimmed.trim_end().ends_with(',') {
                    ","
                } else {
                    ""
                };
                return format!("{indent}'{key}' => '{escaped}'{comma}");
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn write_localstack_database_helper(public: &Path, database: &HostDatabaseConfig) -> AppResult<()> {
    let content = format!(
        "<?php\nreturn [\n    'host' => '{}',\n    'port' => '{}',\n    'database' => '{}',\n    'username' => '{}',\n    'password' => '{}',\n    'dsn' => '{}:host={};port={};dbname={};charset=utf8mb4',\n];\n",
        php_escape(&database_host(database)),
        database.port,
        php_escape(&database.name),
        php_escape(&database.user),
        php_escape(&database.password),
        if database.engine == "PostgreSQL" { "pgsql" } else { "mysql" },
        database_host_only(&database.engine),
        database.port,
        php_escape(&database.name)
    );
    fs::write(public.join("localstack-database.php"), content)
        .map_err(|err| format!("Cannot write localstack-database.php: {err}"))
}

fn set_php_define(config: &str, key: &str, value: &str) -> String {
    let spaced = format!("define( '{key}'");
    let compact = format!("define('{key}'");
    config
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&spaced) || trimmed.starts_with(&compact) {
                format!("define( '{key}', '{}' );", php_escape(value))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn database_connection(engine: &str) -> &'static str {
    if engine == "PostgreSQL" {
        "pgsql"
    } else {
        "mysql"
    }
}

fn database_port(engine: &str) -> u16 {
    match engine {
        "PostgreSQL" => 5432,
        "MariaDB" => 3307,
        _ => 3306,
    }
}

fn database_host(database: &HostDatabaseConfig) -> String {
    format!("{}:{}", database_host_only(&database.engine), database.port)
}

fn database_host_for_form(database: &HostDatabaseConfig) -> String {
    if database.port == database_port(&database.engine) {
        database_host_only(&database.engine).to_string()
    } else {
        database_host(database)
    }
}

fn database_host_only(_engine: &str) -> &'static str {
    "127.0.0.1"
}

fn drupal_driver(engine: &str) -> &'static str {
    if engine == "PostgreSQL" {
        "pgsql"
    } else {
        "mysql"
    }
}

fn env_escape(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '#' | '"' | '\''))
    {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

fn php_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn write_vhost_snippets(store: &Store, host: &HostInfo) -> AppResult<()> {
    let document_root = document_root(host);
    let apache_dir = store.dir.join("configs").join("apache").join("vhosts");
    let nginx_dir = store.dir.join("configs").join("nginx").join("vhosts");
    fs::create_dir_all(&apache_dir)
        .map_err(|err| format!("Cannot create Apache vhosts folder: {err}"))?;
    fs::create_dir_all(&nginx_dir)
        .map_err(|err| format!("Cannot create Nginx vhosts folder: {err}"))?;
    let apache = format!(
        "<VirtualHost *:{}>\n    ServerName {}\n    DocumentRoot \"{}\"\n    <Directory \"{}\">\n        AllowOverride All\n        Require all granted\n    </Directory>\n    ErrorLog \"{}\\\\logs\\\\error.log\"\n    CustomLog \"{}\\\\logs\\\\access.log\" combined\n</VirtualHost>\n",
        host.http_port,
        host.domain,
        document_root.display(),
        document_root.display(),
        host.root_folder,
        host.root_folder
    );
    let nginx = format!(
        "server {{\n    listen {};\n    server_name {};\n    root {};\n    index index.php index.html;\n\n    location / {{\n        try_files $uri $uri/ /index.php?$query_string;\n    }}\n\n    location ~ \\.php$ {{\n        include fastcgi_params;\n        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;\n        fastcgi_pass 127.0.0.1:9000;\n    }}\n}}\n",
        host.http_port,
        host.domain,
        document_root.display().to_string().replace('\\', "/")
    );
    fs::write(apache_dir.join(format!("{}.conf", host.domain)), apache)
        .map_err(|err| format!("Cannot write Apache vhost: {err}"))?;
    fs::write(nginx_dir.join(format!("{}.conf", host.domain)), nginx)
        .map_err(|err| format!("Cannot write Nginx vhost: {err}"))?;
    Ok(())
}

fn document_root(host: &HostInfo) -> PathBuf {
    let root = PathBuf::from(&host.root_folder);
    let document_root = PathBuf::from(&host.document_root);
    if document_root.is_absolute() {
        document_root
    } else {
        root.join(document_root)
    }
}

fn resolve_data_path(store: &Store, path: &str) -> PathBuf {
    let target = PathBuf::from(path);
    if target.is_absolute() {
        target
    } else {
        store.dir.join(target)
    }
}
