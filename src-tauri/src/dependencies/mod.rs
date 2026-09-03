use crate::state::{AppResult, LogLevel, ServiceInfo, ServiceStatus, Store};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Copy)]
struct DependencySpec {
    package_id: &'static str,
    executable_names: &'static [&'static str],
    common_paths: &'static [&'static str],
}

pub fn install_service_dependency(service_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
        .ok_or_else(|| format!("Service {service_id} is not configured."))?;
    let spec = dependency_spec(&service)
        .ok_or_else(|| format!("No automatic installer is configured for {}.", service.name))?;
    if let Some(path) = find_existing_executable(spec, &service) {
        update_service_path(&mut snapshot, &service_id, path)?;
    } else {
        mark_installing(&store, &mut snapshot, &service_id, &service.name)?;
        let install_result: AppResult<Option<String>> = if service.id == "meilisearch" {
            install_meilisearch(&store).map(|(_, version)| Some(version))
        } else {
            install_with_winget(spec.package_id, &service.name).map(|_| None)
        };
        let downloaded_version = match install_result {
            Ok(version) => version,
            Err(err) => {
                let mut failed = store.load_static()?;
                if let Some(service) = failed
                    .services
                    .iter_mut()
                    .find(|service| service.id == service_id)
                {
                    service.status = ServiceStatus::Error;
                    service.last_error = Some(err.clone());
                }
                store.log(
                    &mut failed,
                    LogLevel::Error,
                    &service.name,
                    err.clone(),
                    None,
                );
                store.save(&failed)?;
                return Err(err);
            }
        };
        snapshot = store.load_static()?;
        let path = find_existing_executable(spec, &service).ok_or_else(|| {
            format!(
                "{} was installed, but LocalStack Pro could not find one of: {}. Set the executable path manually in Services.",
                service.name,
                spec.executable_names.join(", ")
            )
        })?;
        update_service_path(&mut snapshot, &service_id, path)?;
        if let Some(version) = downloaded_version {
            if let Some(item) = snapshot
                .services
                .iter_mut()
                .find(|item| item.id == service_id)
            {
                item.version = version;
            }
        }
    }
    let name = snapshot
        .services
        .iter()
        .find(|item| item.id == service_id)
        .map(|item| item.name.clone())
        .unwrap_or(service.name);
    store.log(
        &mut snapshot,
        LogLevel::Info,
        &name,
        format!("{name} dependency installed or detected"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn update_service_dependency(service_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .cloned()
        .ok_or_else(|| format!("Service {service_id} is not configured."))?;
    let spec = dependency_spec(&service)
        .ok_or_else(|| format!("No automatic updater is configured for {}.", service.name))?;
    if matches!(
        service.status,
        ServiceStatus::Running | ServiceStatus::Starting
    ) {
        return Err(format!(
            "Stop {} before updating it so LocalStack Pro can replace its files safely.",
            service.name
        ));
    }

    mark_installing(&store, &mut snapshot, &service_id, &service.name)?;
    let result: AppResult<()> = if service.id == "meilisearch" {
        let (path, version) = install_meilisearch(&store)?;
        update_service_path(&mut snapshot, &service_id, path)?;
        if let Some(item) = snapshot
            .services
            .iter_mut()
            .find(|item| item.id == service_id)
        {
            item.version = version;
        }
        Ok(())
    } else {
        update_with_winget(spec.package_id, &service.name)?;
        if let Some(path) = find_existing_executable(spec, &service) {
            update_service_path(&mut snapshot, &service_id, path)?;
        }
        Ok(())
    };

    if let Err(err) = result {
        let mut failed = store.load_static()?;
        if let Some(item) = failed
            .services
            .iter_mut()
            .find(|item| item.id == service_id)
        {
            item.status = ServiceStatus::Error;
            item.last_error = Some(err.clone());
        }
        store.log(
            &mut failed,
            LogLevel::Error,
            &service.name,
            err.clone(),
            None,
        );
        store.save(&failed)?;
        return Err(err);
    }

    let name = snapshot
        .services
        .iter()
        .find(|item| item.id == service_id)
        .map(|item| item.name.clone())
        .unwrap_or(service.name);
    store.log(
        &mut snapshot,
        LogLevel::Info,
        &name,
        format!("{name} was updated"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn install_all_missing_dependencies() -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let missing = snapshot
        .services
        .iter()
        .filter(|service| {
            dependency_spec(service).is_some() && !Path::new(&service.executable_path).exists()
        })
        .map(|service| service.id.clone())
        .collect::<Vec<_>>();
    let mut current = snapshot;
    for service_id in missing {
        current = install_service_dependency(service_id)?;
    }
    Ok(current)
}

pub fn detect_dependencies() -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let service_ids = snapshot
        .services
        .iter()
        .map(|service| service.id.clone())
        .collect::<Vec<_>>();
    for service_id in service_ids {
        let Some(service) = snapshot
            .services
            .iter()
            .find(|service| service.id == service_id)
            .cloned()
        else {
            continue;
        };
        if let Some(spec) = dependency_spec(&service) {
            if let Some(path) = find_existing_executable(spec, &service) {
                update_service_path(&mut snapshot, &service.id, path)?;
            }
        }
    }
    store.save(&snapshot)?;
    Ok(snapshot)
}

fn dependency_spec(service: &ServiceInfo) -> Option<DependencySpec> {
    match service.id.as_str() {
        "apache" => Some(DependencySpec {
            package_id: "ApacheLounge.httpd",
            executable_names: &["httpd.exe"],
            common_paths: &[
                "C:\\Apache24\\bin\\httpd.exe",
                "C:\\Program Files\\Apache24\\bin\\httpd.exe",
                "C:\\Program Files\\Apache Lounge\\Apache24\\bin\\httpd.exe",
                "%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\ApacheLounge.httpd_Microsoft.Winget.Source_8wekyb3d8bbwe\\Apache24\\bin\\httpd.exe",
            ],
        }),
        "nginx" => Some(DependencySpec {
            package_id: "nginxinc.nginx",
            executable_names: &["nginx.exe"],
            common_paths: &[
                "C:\\nginx\\nginx.exe",
                "C:\\Program Files\\nginx\\nginx.exe",
                "C:\\Program Files\\nginx-1.29.8\\nginx.exe",
                "%LOCALAPPDATA%\\Microsoft\\WinGet\\Packages\\nginxinc.nginx_Microsoft.Winget.Source_8wekyb3d8bbwe\\nginx-1.29.8\\nginx.exe",
                "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\nginx.exe",
            ],
        }),
        "mysql" => Some(DependencySpec {
            package_id: "Oracle.MySQL",
            executable_names: &["mysqld.exe"],
            common_paths: &[
                "C:\\Program Files\\MySQL\\MySQL Server 8.4\\bin\\mysqld.exe",
                "C:\\Program Files\\MySQL\\MySQL Server 8.0\\bin\\mysqld.exe",
            ],
        }),
        "mariadb" => Some(DependencySpec {
            package_id: "MariaDB.Server",
            executable_names: &["mariadbd.exe", "mysqld.exe"],
            common_paths: &[
                "C:\\Program Files\\MariaDB 12.2\\bin\\mariadbd.exe",
                "C:\\Program Files\\MariaDB 11.4\\bin\\mariadbd.exe",
                "C:\\Program Files\\MariaDB 10.11\\bin\\mariadbd.exe",
            ],
        }),
        "postgresql" => Some(DependencySpec {
            package_id: "PostgreSQL.PostgreSQL.16",
            executable_names: &["postgres.exe"],
            common_paths: &[
                "C:\\Program Files\\PostgreSQL\\16\\bin\\postgres.exe",
                "C:\\Program Files\\PostgreSQL\\17\\bin\\postgres.exe",
                "C:\\Program Files\\PostgreSQL\\15\\bin\\postgres.exe",
            ],
        }),
        "redis" => Some(DependencySpec {
            package_id: "Redis.Redis",
            executable_names: &["redis-server.exe"],
            common_paths: &[
                "C:\\Program Files\\Redis\\redis-server.exe",
                "C:\\Redis\\redis-server.exe",
            ],
        }),
        "mailpit" => Some(DependencySpec {
            package_id: "axllent.mailpit",
            executable_names: &["mailpit.exe"],
            common_paths: &[
                "C:\\Program Files\\Mailpit\\mailpit.exe",
                "C:\\Program Files\\axllent\\mailpit\\mailpit.exe",
                "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\mailpit.exe",
            ],
        }),
        "node-proxy" => Some(DependencySpec {
            package_id: "OpenJS.NodeJS.LTS",
            executable_names: &["node.exe"],
            common_paths: &["C:\\Program Files\\nodejs\\node.exe"],
        }),
        "mongodb" => Some(DependencySpec {
            package_id: "MongoDB.Server",
            executable_names: &["mongod.exe"],
            common_paths: &[
                "C:\\Program Files\\MongoDB\\Server\\7.0\\bin\\mongod.exe",
                "C:\\Program Files\\MongoDB\\Server\\6.0\\bin\\mongod.exe",
            ],
        }),
        "minio" => Some(DependencySpec {
            package_id: "MinIO.MinIO",
            executable_names: &["minio.exe"],
            common_paths: &[
                "C:\\Program Files\\MinIO\\minio.exe",
                "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\minio.exe",
            ],
        }),
        "elasticsearch" => Some(DependencySpec {
            package_id: "Elastic.Elasticsearch",
            executable_names: &["elasticsearch.bat"],
            common_paths: &[
                "C:\\Program Files\\Elastic\\Elasticsearch\\8.17.0\\bin\\elasticsearch.bat",
                "C:\\elasticsearch\\bin\\elasticsearch.bat",
            ],
        }),
        "memcached" => Some(DependencySpec {
            package_id: "jef.memcached",
            executable_names: &["memcached.exe"],
            common_paths: &[
                "C:\\Program Files\\memcached\\memcached.exe",
                "C:\\memcached\\memcached.exe",
            ],
        }),
        "caddy" => Some(DependencySpec {
            package_id: "CaddyServer.Caddy",
            executable_names: &["caddy.exe"],
            common_paths: &[
                "C:\\Program Files\\Caddy\\caddy.exe",
                "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\caddy.exe",
            ],
        }),
        "rabbitmq" => Some(DependencySpec {
            package_id: "VMware.RabbitMQ",
            executable_names: &["rabbitmq-server.bat"],
            common_paths: &[
                "C:\\Program Files\\RabbitMQ Server\\rabbitmq_server-3.13.0\\sbin\\rabbitmq-server.bat",
                "C:\\Program Files\\RabbitMQ Server\\rabbitmq_server-3.12.0\\sbin\\rabbitmq-server.bat",
            ],
        }),
        "meilisearch" => Some(DependencySpec {
            package_id: "Meilisearch.Meilisearch",
            executable_names: &["meilisearch.exe"],
            common_paths: &[
                "%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\meilisearch\\meilisearch.exe",
                "%APPDATA%\\LocalStack Pro\\data\\services\\meilisearch\\meilisearch.exe",
            ],
        }),
        "docker" => Some(DependencySpec {
            package_id: "Docker.DockerDesktop",
            executable_names: &["Docker Desktop.exe"],
            common_paths: &[
                "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe",
                "%ProgramFiles%\\Docker\\Docker\\Docker Desktop.exe",
            ],
        }),
        _ => None,
    }
}

fn install_with_winget(package_id: &str, service_name: &str) -> AppResult<()> {
    run_winget(package_id, service_name, "install")
}

fn update_with_winget(package_id: &str, service_name: &str) -> AppResult<()> {
    run_winget(package_id, service_name, "upgrade")
}

fn run_winget(package_id: &str, service_name: &str, action: &str) -> AppResult<()> {
    let winget = which("winget.exe").or_else(|| {
        let fallback = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| {
                path.join("Microsoft")
                    .join("WindowsApps")
                    .join("winget.exe")
            })
            .unwrap_or_default();
        fallback.exists().then_some(fallback)
    });
    let winget = winget.ok_or_else(|| {
        format!("Cannot install {service_name}: winget.exe was not found on this Windows system.")
    })?;
    let mut command = Command::new(winget);
    command.args([
        action,
        "--id",
        package_id,
        "--exact",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--disable-interactivity",
        "--silent",
    ]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|err| format!("Cannot run winget for {service_name}: {err}"))?;
    let started = Instant::now();
    loop {
        if let Ok(Some(_status)) = child.try_wait() {
            break;
        }
        if started.elapsed() > Duration::from_secs(20 * 60) {
            let _ = child.kill();
            return Err(format!(
                "winget timed out while {action}ing {service_name}. Check Windows Package Manager and try again."
            ));
        }
        thread::sleep(Duration::from_millis(250));
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("Cannot read winget result for {service_name}: {err}"))?;
    let status = output.status;
    if !status.success() {
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");
        return Err(format!(
            "winget failed while {action}ing {service_name} ({package_id}). Exit code: {:?}. {}",
            status.code(),
            detail
        ));
    }
    Ok(())
}

fn install_meilisearch(store: &Store) -> AppResult<(PathBuf, String)> {
    let directory = store.dir.join("services").join("meilisearch");
    let target = directory.join("meilisearch.exe");
    fs::create_dir_all(&directory)
        .map_err(|err| format!("Cannot create Meilisearch folder: {err}"))?;
    let temporary = directory.join("meilisearch.download.exe");
    let quoted_target = temporary.display().to_string().replace('\'', "''");
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; $release=Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/meilisearch/meilisearch/releases/latest'; $asset=$release.assets | Where-Object {{ $_.name -eq 'meilisearch-windows-amd64.exe' }} | Select-Object -First 1; if (!$asset) {{ throw 'The official Meilisearch Windows release asset was not found.' }}; Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile '{quoted_target}'; Write-Output $release.tag_name"
    );
    let version = run_hidden_powershell(&script, "Meilisearch download")?;
    if !temporary.is_file() {
        return Err("Meilisearch download finished without an executable file.".to_string());
    }
    if target.exists() {
        fs::remove_file(&target).map_err(|err| format!("Cannot replace Meilisearch: {err}"))?;
    }
    fs::rename(&temporary, &target).map_err(|err| format!("Cannot finalize Meilisearch: {err}"))?;
    Ok((target, version.trim_start_matches('v').trim().to_string()))
}

fn run_hidden_powershell(script: &str, action: &str) -> AppResult<String> {
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start {action}: {err}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Unknown PowerShell error.")
        .trim();
    Err(format!("{action} failed: {detail}"))
}

fn find_existing_executable(spec: DependencySpec, service: &ServiceInfo) -> Option<PathBuf> {
    let configured = PathBuf::from(&service.executable_path);
    if configured.exists() {
        return Some(configured);
    }
    for path in spec.common_paths {
        let candidate = PathBuf::from(expand_env(path));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    for name in spec.executable_names {
        if let Some(path) = which(name) {
            return Some(path);
        }
    }
    for root in searchable_roots(service) {
        if let Some(path) = find_under(&root, spec.executable_names, 3) {
            return Some(path);
        }
    }
    None
}

fn update_service_path(
    snapshot: &mut crate::state::AppSnapshot,
    service_id: &str,
    executable_path: PathBuf,
) -> AppResult<()> {
    let service = snapshot
        .services
        .iter_mut()
        .find(|service| service.id == service_id)
        .ok_or_else(|| format!("Service {service_id} is not configured."))?;
    service.executable_path = display_path(&executable_path);
    service.arguments = default_arguments(service_id, &executable_path);
    let root = executable_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    service.config_path = root.join("conf").join("service.conf").display().to_string();
    service.log_path = root.join("logs").join("service.log").display().to_string();
    service.last_error = None;
    service.status = ServiceStatus::Stopped;
    Ok(())
}

fn mark_installing(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    service_id: &str,
    name: &str,
) -> AppResult<()> {
    if let Some(service) = snapshot
        .services
        .iter_mut()
        .find(|service| service.id == service_id)
    {
        service.status = ServiceStatus::Starting;
        service.last_error = Some(format!("Installing {name} through winget..."));
    }
    store.log(
        snapshot,
        LogLevel::Info,
        name,
        format!("Installing {name} through winget"),
        None,
    );
    store.save(snapshot)
}

fn display_path(path: &Path) -> String {
    path.display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .to_string()
}

fn default_arguments(service_id: &str, executable_path: &Path) -> Vec<String> {
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
        _ => Vec::new(),
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn searchable_roots(service: &ServiceInfo) -> Vec<PathBuf> {
    let local_app = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let winget_packages = local_app
        .as_ref()
        .map(|path| path.join("Microsoft").join("WinGet").join("Packages"));
    let winget_links = local_app
        .as_ref()
        .map(|path| path.join("Microsoft").join("WinGet").join("Links"));
    [
        winget_links,
        winget_packages,
        Path::new(&service.executable_path)
            .parent()
            .map(Path::to_path_buf),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn find_under(root: &Path, names: &[&str], depth: usize) -> Option<PathBuf> {
    if depth == 0 || !root.exists() {
        return None;
    }
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|value| value.to_str()) {
                if names
                    .iter()
                    .any(|name| file_name.eq_ignore_ascii_case(name))
                {
                    return Some(path);
                }
            }
        } else if path.is_dir() {
            if should_skip_dir(&path) {
                continue;
            }
            if let Some(found) = find_under(&path, names, depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

fn should_skip_dir(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    matches!(
        name.as_str(),
        "$recycle.bin" | "windows" | "winsxs" | "system32" | "syswow64" | "node_modules"
    )
}

fn expand_env(value: &str) -> String {
    value
        .replace(
            "%ProgramFiles%",
            &std::env::var("ProgramFiles").unwrap_or_default(),
        )
        .replace(
            "%ProgramFiles(x86)%",
            &std::env::var("ProgramFiles(x86)").unwrap_or_default(),
        )
        .replace(
            "%LOCALAPPDATA%",
            &std::env::var("LOCALAPPDATA").unwrap_or_default(),
        )
}
