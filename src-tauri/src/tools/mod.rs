use crate::state::{AppResult, Store};
use chrono::Utc;
use serde::Serialize;
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInspection {
    pub port: u16,
    pub status: String,
    pub service: Option<String>,
    pub pid: Option<u32>,
    pub process: Option<String>,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInspection {
    pub kind: String,
    pub root: String,
    pub document_root: String,
    pub env_template: String,
    pub commands: Vec<String>,
    pub checks: Vec<ProjectCheck>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCheck {
    pub title: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SitePreview {
    pub url: String,
    pub status: String,
    pub response_time_ms: u128,
    pub content_type: String,
    pub redirected_to: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SslDiagnostic {
    pub domain: String,
    pub ca_trusted: bool,
    pub cert_exists: bool,
    pub key_exists: bool,
    pub san_correct: bool,
    pub vhost_configured: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTool {
    pub id: String,
    pub name: String,
    pub command: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentSnapshotInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub hosts: usize,
    pub services: usize,
    pub databases: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceProcess {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory_mb: u64,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeScript {
    pub name: String,
    pub command: String,
}

pub fn scan_ports() -> AppResult<Vec<PortInspection>> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let mut ports = snapshot
        .services
        .iter()
        .flat_map(|service| {
            service
                .ports
                .iter()
                .map(move |port| (*port, Some(service.name.clone())))
        })
        .collect::<Vec<_>>();
    ports.extend(snapshot.hosts.iter().flat_map(|host| {
        [
            (host.http_port, Some(host.domain.clone())),
            (host.https_port, Some(host.domain.clone())),
        ]
    }));
    ports.sort_by_key(|(port, _)| *port);
    ports.dedup_by_key(|(port, _)| *port);

    let netstat = netstat_output();
    let result = ports
        .into_iter()
        .map(|(port, service)| {
            let pid = pid_for_port(&netstat, port);
            let process = pid.and_then(process_name);
            let listening = TcpStream::connect_timeout(
                &format!("127.0.0.1:{port}")
                    .parse()
                    .unwrap_or_else(|_| "127.0.0.1:9".parse().expect("static socket")),
                Duration::from_millis(180),
            )
            .is_ok();
            PortInspection {
                port,
                status: if listening { "Listening" } else { "Free" }.to_string(),
                service,
                pid,
                process,
                action: if listening { "In use" } else { "Available" }.to_string(),
            }
        })
        .collect();
    Ok(result)
}

pub fn inspect_project(path: String) -> AppResult<ProjectInspection> {
    let root = ensure_project_path(path)?;
    let package = root.join("package.json");
    let composer = root.join("composer.json");
    let artisan = root.join("artisan");
    let wp_config = root.join("wp-config.php");
    let public = root.join("public");
    let kind = if package.is_file() {
        let text = fs::read_to_string(&package)
            .unwrap_or_default()
            .to_lowercase();
        if text.contains("\"next\"") {
            "Next.js"
        } else if text.contains("\"vite\"") {
            "Vite"
        } else if text.contains("\"@nestjs") {
            "NestJS"
        } else if text.contains("\"express\"") {
            "Express"
        } else {
            "Node.js"
        }
    } else if artisan.is_file() {
        "Laravel"
    } else if wp_config.is_file() || root.join("wp-admin").is_dir() {
        "WordPress"
    } else if composer.is_file() {
        "PHP Composer"
    } else {
        "Custom PHP"
    }
    .to_string();
    let document_root = if matches!(
        kind.as_str(),
        "Next.js" | "Vite" | "NestJS" | "Express" | "Node.js"
    ) {
        ".".to_string()
    } else if public.is_dir() {
        "public".to_string()
    } else {
        ".".to_string()
    };
    let mut checks = Vec::new();
    push_project_check(
        &mut checks,
        "Project folder",
        "ok",
        format!("Found {}", root.display()),
    );
    push_project_check(
        &mut checks,
        "package.json",
        if package.is_file() { "ok" } else { "warning" },
        if package.is_file() {
            "Node metadata found"
        } else {
            "No package.json found"
        },
    );
    push_project_check(
        &mut checks,
        "composer.json",
        if composer.is_file() { "ok" } else { "warning" },
        if composer.is_file() {
            "Composer metadata found"
        } else {
            "No composer.json found"
        },
    );
    push_project_check(
        &mut checks,
        ".env",
        if root.join(".env").is_file() {
            "ok"
        } else {
            "warning"
        },
        if root.join(".env").is_file() {
            ".env exists"
        } else {
            ".env can be generated"
        },
    );
    let commands = project_commands(&kind);
    Ok(ProjectInspection {
        kind: kind.clone(),
        root: root.display().to_string(),
        document_root,
        env_template: env_template(
            &kind,
            "local_db",
            "local_user",
            "local_password",
            "local.test",
        ),
        commands,
        checks,
    })
}

pub fn generate_env_template(
    path: String,
    kind: String,
    database: String,
    user: String,
    password: String,
    domain: String,
) -> AppResult<String> {
    let root = ensure_project_path(path)?;
    let env = root.join(".env");
    if env.exists() {
        let backup = root.join(format!(".env.backup-{}", Utc::now().format("%Y%m%d%H%M%S")));
        fs::copy(&env, &backup).map_err(|err| format!("Cannot back up existing .env: {err}"))?;
    }
    let content = env_template(&kind, &database, &user, &password, &domain);
    fs::write(&env, content).map_err(|err| format!("Cannot write .env: {err}"))?;
    Ok(env.display().to_string())
}

pub fn run_project_command(path: String, command_key: String) -> AppResult<String> {
    let root = ensure_project_path(path)?;
    let (program, args, long_running) = match command_key.as_str() {
        "npm-install" => ("cmd.exe", vec!["/C", "npm install"], false),
        "npm-dev" => ("cmd.exe", vec!["/C", "start /B npm run dev"], true),
        "composer-install" => ("cmd.exe", vec!["/C", "composer install"], false),
        "artisan-migrate" => ("cmd.exe", vec!["/C", "php artisan migrate --force"], false),
        "wp-info" => ("cmd.exe", vec!["/C", "wp core version"], false),
        _ => return Err("Unsupported project command.".to_string()),
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(if long_running {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .stderr(if long_running {
            Stdio::null()
        } else {
            Stdio::piped()
        });
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    if long_running {
        command
            .spawn()
            .map_err(|err| format!("Cannot start project command: {err}"))?;
        return Ok(format!("Started {} in {}", command_key, root.display()));
    }
    let output = command
        .output()
        .map_err(|err| format!("Cannot run project command: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "Project command failed with exit code {:?}. {}",
            output.status.code(),
            detail
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if stdout.is_empty() {
        format!("Command {} completed.", command_key)
    } else {
        stdout
    })
}

pub fn preview_host(host_id: String) -> AppResult<SitePreview> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let host = snapshot
        .hosts
        .iter()
        .find(|item| item.id == host_id || item.domain == host_id)
        .ok_or_else(|| "Host not found.".to_string())?;
    let scheme = if host.ssl { "https" } else { "http" };
    let port = if host.ssl {
        host.https_port
    } else {
        host.http_port
    };
    let url = if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
        format!("{scheme}://{}", host.domain)
    } else {
        format!("{scheme}://{}:{port}", host.domain)
    };
    let started = Instant::now();
    let mut stream = TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}")
            .parse()
            .map_err(|err| format!("Cannot parse target socket: {err}"))?,
        Duration::from_secs(2),
    )
    .map_err(|err| format!("Cannot connect to {url}: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("Cannot set read timeout: {err}"))?;
    let request = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: LocalStackProPreview\r\n\r\n",
        host.domain
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|err| format!("Cannot request {url}: {err}"))?;
    let mut buffer = String::new();
    stream
        .read_to_string(&mut buffer)
        .map_err(|err| format!("Cannot read response from {url}: {err}"))?;
    let response_time_ms = started.elapsed().as_millis();
    let status = buffer
        .lines()
        .next()
        .unwrap_or("HTTP/1.1 000 No response")
        .to_string();
    let content_type =
        header_value(&buffer, "content-type").unwrap_or_else(|| "unknown".to_string());
    let redirected_to = header_value(&buffer, "location");
    Ok(SitePreview {
        url,
        status: status.clone(),
        response_time_ms,
        content_type,
        redirected_to,
        message: if status.contains(" 200 ") || status.contains(" 30") {
            "Site responded.".to_string()
        } else {
            "Site responded with a non-success status.".to_string()
        },
    })
}

pub fn export_portable_host(host_id: String, target: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let host = snapshot
        .hosts
        .iter()
        .find(|item| item.id == host_id || item.domain == host_id)
        .ok_or_else(|| "Host not found.".to_string())?;
    let target = if target.trim().ends_with(".zip") {
        PathBuf::from(target.trim())
    } else {
        PathBuf::from(&snapshot.settings.backups_folder).join(format!(
            "{}-portable-{}.zip",
            host.domain,
            Utc::now().format("%Y%m%d-%H%M%S")
        ))
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create export folder: {err}"))?;
    }
    let temp = store
        .dir
        .join("temp")
        .join(format!("portable-{}", Utc::now().format("%Y%m%d%H%M%S")));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).map_err(|err| format!("Cannot create portable temp: {err}"))?;
    fs::write(
        temp.join("host.json"),
        serde_json::to_string_pretty(host)
            .map_err(|err| format!("Cannot serialize host: {err}"))?,
    )
    .map_err(|err| format!("Cannot write host manifest: {err}"))?;
    let root = PathBuf::from(&host.root_folder);
    if root.exists() {
        copy_dir(&root, &temp.join("project"))?;
    }
    compress_folder(&temp, &target)?;
    let _ = fs::remove_dir_all(&temp);
    Ok(target.display().to_string())
}

pub fn backup_host(host_id: String, target: String) -> AppResult<String> {
    export_portable_host(host_id, target)
}

pub fn restore_host_backup(path: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let archive = PathBuf::from(path.trim());
    if !archive.is_file() {
        return Err(format!("Host backup not found: {}", archive.display()));
    }
    let temp = store.dir.join("temp").join(format!(
        "host-restore-{}",
        Utc::now().format("%Y%m%d%H%M%S")
    ));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).map_err(|err| format!("Cannot create restore temp: {err}"))?;
    extract_archive(&archive, &temp)?;
    let root = if temp.join("host.json").is_file() {
        temp.clone()
    } else {
        fs::read_dir(&temp)
            .map_err(|err| format!("Cannot read restore temp: {err}"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|item| item.join("host.json").is_file())
            .ok_or_else(|| "Host backup manifest was not found.".to_string())?
    };
    let mut host: crate::state::HostInfo = serde_json::from_str(
        &fs::read_to_string(root.join("host.json"))
            .map_err(|err| format!("Cannot read host backup manifest: {err}"))?,
    )
    .map_err(|err| format!("Cannot parse host backup manifest: {err}"))?;
    let project = root.join("project");
    if project.is_dir() {
        let project_root = PathBuf::from(&host.root_folder);
        if project_root.exists() {
            let backup = project_root
                .with_extension(format!("pre-restore-{}", Utc::now().format("%Y%m%d%H%M%S")));
            fs::rename(&project_root, &backup)
                .map_err(|err| format!("Cannot move existing project folder: {err}"))?;
        }
        copy_dir(&project, &project_root)?;
    }
    host.updated_at = Utc::now().to_rfc3339();
    let snapshot = crate::hosts::save_host(host)?;
    let _ = fs::remove_dir_all(&temp);
    Ok(snapshot)
}

pub fn inspect_installed_tools() -> AppResult<Vec<InstalledTool>> {
    let tools = [
        ("node", "Node.js", "node", "--version"),
        ("npm", "npm", "npm", "--version"),
        ("git", "Git", "git", "--version"),
        ("composer", "Composer", "composer", "--version"),
        ("php", "PHP", "php", "--version"),
        ("mysql", "MySQL Client", "mysql", "--version"),
        ("mysqldump", "MySQL Dump", "mysqldump", "--version"),
        ("psql", "PostgreSQL Client", "psql", "--version"),
        ("redis-cli", "Redis CLI", "redis-cli", "--version"),
        ("winget", "Windows Package Manager", "winget", "--version"),
    ];
    Ok(tools
        .iter()
        .map(|(id, name, command, version_arg)| {
            let path = find_command_path(command);
            let version = path
                .as_ref()
                .and_then(|_| command_version(command, version_arg).ok());
            let installed = path.is_some();
            InstalledTool {
                id: (*id).to_string(),
                name: (*name).to_string(),
                command: (*command).to_string(),
                path,
                version,
                status: if installed { "installed" } else { "missing" }.to_string(),
            }
        })
        .collect())
}

pub fn list_files(path: String) -> AppResult<Vec<FileEntry>> {
    let root = allowed_workspace_path(path)?;
    if !root.exists() {
        fs::create_dir_all(&root)
            .map_err(|err| format!("Cannot create folder {}: {err}", root.display()))?;
    }
    if !root.is_dir() {
        return Err(format!("Path is not a folder: {}", root.display()));
    }
    let mut entries = fs::read_dir(&root)
        .map_err(|err| format!("Cannot read folder {}: {err}", root.display()))?
        .filter_map(Result::ok)
        .map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok();
            let modified = metadata
                .as_ref()
                .and_then(|item| item.modified().ok())
                .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339());
            FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path.display().to_string(),
                kind: if path.is_dir() { "folder" } else { "file" }.to_string(),
                size: metadata.as_ref().map(|item| item.len()).unwrap_or(0),
                modified,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        b.kind
            .cmp(&a.kind)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub fn read_file(path: String) -> AppResult<ConfigFile> {
    let target = allowed_workspace_path(path)?;
    if !target.is_file() {
        return Err(format!("File does not exist: {}", target.display()));
    }
    if target.metadata().map(|item| item.len()).unwrap_or(0) > 2_000_000 {
        return Err("File is too large for the built-in editor.".to_string());
    }
    let content = fs::read_to_string(&target)
        .map_err(|err| format!("Cannot read file {}: {err}", target.display()))?;
    Ok(ConfigFile {
        path: target.display().to_string(),
        content,
    })
}

pub fn write_file(path: String, content: String) -> AppResult<String> {
    let target = allowed_workspace_path(path)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create parent folder: {err}"))?;
    }
    if target.exists() {
        let backup = target.with_extension(format!(
            "{}.backup-{}",
            target
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("txt"),
            Utc::now().format("%Y%m%d%H%M%S")
        ));
        let _ = fs::copy(&target, backup);
    }
    fs::write(&target, content)
        .map_err(|err| format!("Cannot write file {}: {err}", target.display()))?;
    Ok(target.display().to_string())
}

pub fn create_folder(path: String) -> AppResult<String> {
    let target = allowed_workspace_path(path)?;
    fs::create_dir_all(&target)
        .map_err(|err| format!("Cannot create folder {}: {err}", target.display()))?;
    Ok(target.display().to_string())
}

pub fn delete_path(path: String) -> AppResult<String> {
    let target = allowed_workspace_path(path)?;
    if target.is_dir() {
        fs::remove_dir_all(&target)
            .map_err(|err| format!("Cannot delete folder {}: {err}", target.display()))?;
    } else if target.is_file() {
        fs::remove_file(&target)
            .map_err(|err| format!("Cannot delete file {}: {err}", target.display()))?;
    } else {
        return Err(format!("Path does not exist: {}", target.display()));
    }
    Ok(target.display().to_string())
}

pub fn rename_path(path: String, new_name: String) -> AppResult<String> {
    let source = allowed_workspace_path(path)?;
    let clean = new_name.trim();
    if clean.is_empty() || clean.contains('\\') || clean.contains('/') {
        return Err("Enter a valid file or folder name.".to_string());
    }
    let target = source
        .parent()
        .ok_or_else(|| "Cannot rename this path.".to_string())?
        .join(clean);
    let target = allowed_workspace_path(target.display().to_string())?;
    fs::rename(&source, &target).map_err(|err| format!("Cannot rename path: {err}"))?;
    Ok(target.display().to_string())
}

pub fn list_environment_snapshots() -> AppResult<Vec<EnvironmentSnapshotInfo>> {
    let store = Store::new()?;
    let dir = store.dir.join("snapshots");
    fs::create_dir_all(&dir).map_err(|err| format!("Cannot create snapshots folder: {err}"))?;
    let mut items = fs::read_dir(&dir)
        .map_err(|err| format!("Cannot read snapshots folder: {err}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| snapshot_info(entry.path()).ok())
        .collect::<Vec<_>>();
    items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(items)
}

pub fn create_environment_snapshot(name: String) -> AppResult<EnvironmentSnapshotInfo> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let dir = store.dir.join("snapshots");
    fs::create_dir_all(&dir).map_err(|err| format!("Cannot create snapshots folder: {err}"))?;
    let safe_name = sanitize_name(if name.trim().is_empty() {
        "environment"
    } else {
        name.trim()
    });
    let id = format!("{}-{}", Utc::now().format("%Y%m%d%H%M%S"), safe_name);
    let path = dir.join(format!("{id}.json"));
    fs::write(
        &path,
        serde_json::to_string_pretty(&snapshot)
            .map_err(|err| format!("Cannot serialize environment snapshot: {err}"))?,
    )
    .map_err(|err| format!("Cannot write environment snapshot: {err}"))?;
    snapshot_info(path)
}

pub fn restore_environment_snapshot(id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let path = store
        .dir
        .join("snapshots")
        .join(format!("{}.json", sanitize_name(&id)));
    if !path.is_file() {
        return Err(format!("Snapshot not found: {}", path.display()));
    }
    let text = fs::read_to_string(&path).map_err(|err| format!("Cannot read snapshot: {err}"))?;
    let mut snapshot: crate::state::AppSnapshot =
        serde_json::from_str(&text).map_err(|err| format!("Cannot parse snapshot: {err}"))?;
    snapshot.app_data_dir = store.dir.display().to_string();
    let backup = store.dir.join("backups").join(format!(
        "pre-snapshot-restore-{}.zip",
        Utc::now().format("%Y%m%d%H%M%S")
    ));
    let _ = crate::settings::create_app_backup(backup.display().to_string());
    store.save(&snapshot)?;
    Ok(store.refresh_runtime(snapshot))
}

pub fn list_node_scripts(path: String) -> AppResult<Vec<NodeScript>> {
    let root = ensure_project_path(path)?;
    let package = root.join("package.json");
    if !package.is_file() {
        return Ok(Vec::new());
    }
    let text =
        fs::read_to_string(&package).map_err(|err| format!("Cannot read package.json: {err}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("Cannot parse package.json: {err}"))?;
    let scripts = json
        .get("scripts")
        .and_then(|value| value.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(name, value)| {
                    value.as_str().map(|command| NodeScript {
                        name: name.clone(),
                        command: command.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(scripts)
}

pub fn run_node_script(path: String, script: String) -> AppResult<String> {
    let allowed = list_node_scripts(path.clone())?;
    if !allowed.iter().any(|item| item.name == script) {
        return Err("Script was not found in package.json.".to_string());
    }
    let root = ensure_project_path(path)?;
    let mut command = Command::new("cmd.exe");
    command
        .args(["/C", &format!("npm run {script}")])
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags_hidden();
    command
        .spawn()
        .map_err(|err| format!("Cannot start npm script: {err}"))?;
    Ok(format!("Started npm run {script} in {}", root.display()))
}

pub fn resource_monitor() -> AppResult<Vec<ResourceProcess>> {
    let script = "$ErrorActionPreference='SilentlyContinue'; Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 40 Id,ProcessName,CPU,WorkingSet64,Path | ConvertTo-Json -Compress";
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
        .stderr(Stdio::null())
        .creation_flags_hidden();
    let output = command
        .output()
        .map_err(|err| format!("Cannot inspect processes: {err}"))?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|err| format!("Cannot parse process list: {err}"))?;
    let rows = if let Some(items) = value.as_array() {
        items.clone()
    } else {
        vec![value]
    };
    Ok(rows
        .into_iter()
        .filter_map(|item| {
            Some(ResourceProcess {
                pid: item.get("Id")?.as_u64()? as u32,
                name: item.get("ProcessName")?.as_str()?.to_string(),
                cpu: item
                    .get("CPU")
                    .and_then(|value| value.as_f64())
                    .unwrap_or(0.0) as f32,
                memory_mb: item
                    .get("WorkingSet64")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0)
                    / 1024
                    / 1024,
                command: item
                    .get("Path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect())
}

pub fn kill_process(pid: u32) -> AppResult<String> {
    if pid == std::process::id() {
        return Err("Cannot stop the LocalStack Pro process from Resource Monitor.".to_string());
    }
    let mut command = Command::new("taskkill.exe");
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags_hidden();
    let output = command
        .output()
        .map_err(|err| format!("Cannot stop process {pid}: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot stop process {pid}. {detail}"));
    }
    Ok(format!("Process {pid} stopped."))
}

pub fn check_latest_release() -> AppResult<ReleaseInfo> {
    let script = "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; (Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/bftehno-prog/localstack-pro/releases/latest').tag_name + '|' + (Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/bftehno-prog/localstack-pro/releases/latest').html_url";
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
        .map_err(|err| format!("Cannot check GitHub releases: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot check updates. {detail}"));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parts = text.split('|');
    let latest = parts
        .next()
        .unwrap_or(env!("CARGO_PKG_VERSION"))
        .trim()
        .trim_start_matches('v')
        .to_string();
    let url = parts
        .next()
        .unwrap_or("https://github.com/bftehno-prog/localstack-pro/releases")
        .trim()
        .to_string();
    let current = env!("CARGO_PKG_VERSION").to_string();
    Ok(ReleaseInfo {
        current_version: current.clone(),
        latest_version: latest.clone(),
        update_available: latest != current,
        url,
    })
}

pub fn download_latest_release_installer() -> AppResult<String> {
    let store = Store::new()?;
    let target = store
        .dir
        .join("updates")
        .join("LocalStack-Pro-latest-setup.exe");
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create updates folder: {err}"))?;
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; $release=Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/bftehno-prog/localstack-pro/releases/latest'; $asset=$release.assets | Where-Object {{ $_.name -like '*.exe' }} | Select-Object -First 1; if (!$asset) {{ throw 'Release has no installer asset.' }}; Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile '{}'",
        target.display().to_string().replace('\'', "''")
    );
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start update download: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot download update. {detail}"));
    }
    Ok(target.display().to_string())
}

pub fn install_downloaded_update(path: String) -> AppResult<String> {
    let installer = PathBuf::from(path.trim());
    if !installer.is_file() {
        return Err(format!("Installer not found: {}", installer.display()));
    }
    let mut command = Command::new(&installer);
    command
        .arg("/S")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map_err(|err| format!("Cannot start installer: {err}"))?;
    Ok(format!("Installer started: {}", installer.display()))
}

pub fn read_config_file(path: String) -> AppResult<ConfigFile> {
    let target = allowed_text_path(path)?;
    let content = fs::read_to_string(&target)
        .map_err(|err| format!("Cannot read config {}: {err}", target.display()))?;
    Ok(ConfigFile {
        path: target.display().to_string(),
        content,
    })
}

pub fn save_config_file(path: String, content: String) -> AppResult<String> {
    let target = allowed_text_path(path)?;
    if target.exists() {
        let backup = target.with_extension(format!(
            "{}.backup-{}",
            target
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("txt"),
            Utc::now().format("%Y%m%d%H%M%S")
        ));
        fs::copy(&target, &backup).map_err(|err| format!("Cannot create config backup: {err}"))?;
    }
    fs::write(&target, content)
        .map_err(|err| format!("Cannot write config {}: {err}", target.display()))?;
    Ok(target.display().to_string())
}

pub fn create_diagnostic_bundle(target: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load()?;
    let target = if target.trim().ends_with(".zip") {
        PathBuf::from(target.trim())
    } else {
        snapshot
            .settings
            .backups_folder
            .parse::<PathBuf>()
            .unwrap_or_else(|_| store.dir.join("backups"))
            .join(format!(
                "localstack-diagnostics-{}.zip",
                Utc::now().format("%Y%m%d-%H%M%S")
            ))
    };
    let temp = store
        .dir
        .join("temp")
        .join(format!("diagnostics-{}", Utc::now().format("%Y%m%d%H%M%S")));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).map_err(|err| format!("Cannot create diagnostics temp: {err}"))?;
    fs::write(
        temp.join("state.json"),
        serde_json::to_string_pretty(&snapshot)
            .map_err(|err| format!("Cannot serialize state: {err}"))?,
    )
    .map_err(|err| format!("Cannot write diagnostic state: {err}"))?;
    fs::write(
        temp.join("health.json"),
        serde_json::to_string_pretty(&crate::health::run_health_check()?)
            .map_err(|err| format!("Cannot serialize health: {err}"))?,
    )
    .map_err(|err| format!("Cannot write health report: {err}"))?;
    for name in ["logs", "configs", "hosts"] {
        let source = store.dir.join(name);
        if source.exists() {
            copy_dir(&source, &temp.join(name))?;
        }
    }
    compress_folder(&temp, &target)?;
    let _ = fs::remove_dir_all(&temp);
    Ok(target.display().to_string())
}

pub fn diagnose_ssl(domain: String) -> AppResult<SslDiagnostic> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let cert = snapshot
        .certificates
        .iter()
        .find(|item| item.domain.eq_ignore_ascii_case(domain.trim()))
        .ok_or_else(|| "Certificate not found.".to_string())?;
    let cert_exists = PathBuf::from(&cert.cert_path).is_file();
    let key_exists = PathBuf::from(&cert.key_path).is_file();
    let san_correct = cert
        .san_domains
        .iter()
        .any(|san| san.eq_ignore_ascii_case(&cert.domain));
    let vhost_configured = snapshot
        .hosts
        .iter()
        .any(|host| host.domain.eq_ignore_ascii_case(&cert.domain) && host.ssl);
    let ca_trusted = cert.trusted;
    let ok = cert_exists && key_exists && san_correct && vhost_configured && ca_trusted;
    Ok(SslDiagnostic {
        domain: cert.domain.clone(),
        ca_trusted,
        cert_exists,
        key_exists,
        san_correct,
        vhost_configured,
        summary: if ok {
            "SSL is ready for this host."
        } else {
            "SSL needs repair."
        }
        .to_string(),
    })
}

pub fn clone_project_repository(url: String, folder: String) -> AppResult<String> {
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("git@")) || !url.ends_with(".git") {
        return Err("Enter a valid Git repository URL ending with .git.".to_string());
    }
    let target = PathBuf::from(folder.trim());
    if target.exists()
        && fs::read_dir(&target)
            .map_err(|err| format!("Cannot read project folder: {err}"))?
            .next()
            .is_some()
    {
        return Err("Project folder is not empty. Choose an empty folder or enable overwrite in CMS installer.".to_string());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create parent folder: {err}"))?;
    }
    let mut command = Command::new("git.exe");
    command
        .args(["clone", "--depth", "1", url, &target.display().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start git clone: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot clone repository. {detail}"));
    }
    Ok(format!("Repository cloned to {}", target.display()))
}

fn push_project_check(
    checks: &mut Vec<ProjectCheck>,
    title: &str,
    severity: &str,
    message: impl Into<String>,
) {
    checks.push(ProjectCheck {
        title: title.to_string(),
        severity: severity.to_string(),
        message: message.into(),
    });
}

fn project_commands(kind: &str) -> Vec<String> {
    match kind {
        "Next.js" | "Vite" | "NestJS" | "Express" | "Node.js" => vec!["npm-install", "npm-dev"],
        "Laravel" => vec!["composer-install", "artisan-migrate"],
        "WordPress" => vec!["wp-info"],
        "PHP Composer" => vec!["composer-install"],
        _ => vec![],
    }
    .into_iter()
    .map(String::from)
    .collect()
}

fn env_template(kind: &str, database: &str, user: &str, password: &str, domain: &str) -> String {
    if kind == "WordPress" {
        format!("DB_NAME={database}\nDB_USER={user}\nDB_PASSWORD={password}\nDB_HOST=127.0.0.1\nWP_HOME=http://{domain}\nWP_SITEURL=http://{domain}\n")
    } else if kind == "Next.js"
        || kind == "Vite"
        || kind == "Node.js"
        || kind == "Express"
        || kind == "NestJS"
    {
        format!("APP_URL=http://{domain}\nDATABASE_URL=mysql://{user}:{password}@127.0.0.1:3306/{database}\nDB_HOST=127.0.0.1\nDB_DATABASE={database}\nDB_USERNAME={user}\nDB_PASSWORD={password}\n")
    } else {
        format!("APP_NAME=LocalStack\nAPP_ENV=local\nAPP_DEBUG=true\nAPP_URL=http://{domain}\nDB_CONNECTION=mysql\nDB_HOST=127.0.0.1\nDB_PORT=3306\nDB_DATABASE={database}\nDB_USERNAME={user}\nDB_PASSWORD={password}\n")
    }
}

fn header_value(response: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}:");
    response.lines().find_map(|line| {
        if line.to_lowercase().starts_with(&prefix) {
            Some(line[prefix.len()..].trim().to_string())
        } else {
            None
        }
    })
}

fn copy_dir(source: &PathBuf, target: &PathBuf) -> AppResult<()> {
    fs::create_dir_all(target)
        .map_err(|err| format!("Cannot create {}: {err}", target.display()))?;
    for entry in
        fs::read_dir(source).map_err(|err| format!("Cannot read {}: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("Cannot read project entry: {err}"))?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default();
            if matches!(name, "node_modules" | ".next" | "vendor" | ".git") {
                continue;
            }
            copy_dir(&path, &destination)?;
        } else {
            fs::copy(&path, &destination)
                .map_err(|err| format!("Cannot copy {}: {err}", path.display()))?;
        }
    }
    Ok(())
}

fn compress_folder(source: &PathBuf, target: &PathBuf) -> AppResult<()> {
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Compress-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        source.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''")
    );
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start portable export: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot create portable archive. {detail}"));
    }
    Ok(())
}

fn extract_archive(source: &PathBuf, target: &PathBuf) -> AppResult<()> {
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        source.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''")
    );
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start archive extractor: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot extract host backup. {detail}"));
    }
    Ok(())
}

fn find_command_path(command: &str) -> Option<String> {
    let output = Command::new("where.exe")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags_hidden()
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn command_version(command: &str, version_arg: &str) -> AppResult<String> {
    let output = Command::new(command)
        .arg(version_arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags_hidden()
        .output()
        .map_err(|err| format!("Cannot run {command}: {err}"))?;
    if !output.status.success() {
        return Err(format!("{command} did not return a version."));
    }
    let text = String::from_utf8_lossy(if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    })
    .lines()
    .next()
    .unwrap_or("")
    .trim()
    .to_string();
    Ok(text)
}

trait HiddenCommand {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}

impl HiddenCommand for Command {
    fn creation_flags_hidden(&mut self) -> &mut Self {
        #[cfg(windows)]
        {
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}

fn allowed_text_path(path: String) -> AppResult<PathBuf> {
    let store = Store::new()?;
    let target = PathBuf::from(path.trim());
    if !target.is_absolute() {
        return Err("Config path must be absolute.".to_string());
    }
    let allowed_root = store.dir.canonicalize().unwrap_or(store.dir.clone());
    let canonical = target.canonicalize().unwrap_or(target.clone());
    if !canonical.starts_with(&allowed_root) {
        return Err("Config editor can only edit LocalStack Pro data files.".to_string());
    }
    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if !matches!(
        extension.as_str(),
        "conf" | "ini" | "cnf" | "txt" | "log" | "json" | "php" | "js" | "env"
    ) {
        return Err("Unsupported config file type.".to_string());
    }
    Ok(canonical)
}

fn allowed_workspace_path(path: String) -> AppResult<PathBuf> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let target = PathBuf::from(path.trim());
    if !target.is_absolute() {
        return Err("Path must be absolute.".to_string());
    }
    let canonical = target.canonicalize().unwrap_or(target.clone());
    let mut roots = vec![
        store.dir.clone(),
        PathBuf::from(&snapshot.settings.projects_folder),
        PathBuf::from(&snapshot.settings.backups_folder),
        PathBuf::from(&snapshot.settings.services_folder),
    ];
    roots.extend(
        snapshot
            .hosts
            .iter()
            .map(|host| PathBuf::from(&host.root_folder)),
    );
    let allowed = roots.into_iter().any(|root| {
        let root = root.canonicalize().unwrap_or(root);
        canonical.starts_with(root)
    });
    if !allowed {
        return Err("File manager can only access LocalStack Pro project, service, backup and data folders.".to_string());
    }
    Ok(canonical)
}

fn snapshot_info(path: PathBuf) -> AppResult<EnvironmentSnapshotInfo> {
    let text = fs::read_to_string(&path).map_err(|err| format!("Cannot read snapshot: {err}"))?;
    let snapshot: crate::state::AppSnapshot =
        serde_json::from_str(&text).map_err(|err| format!("Cannot parse snapshot: {err}"))?;
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("snapshot")
        .to_string();
    let created_at = path
        .metadata()
        .ok()
        .and_then(|item| item.modified().ok())
        .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    Ok(EnvironmentSnapshotInfo {
        name: id
            .split_once('-')
            .map(|(_, name)| name.replace('-', " "))
            .unwrap_or_else(|| id.clone()),
        id,
        path: path.display().to_string(),
        created_at,
        hosts: snapshot.hosts.len(),
        services: snapshot.services.len(),
        databases: snapshot.databases.len(),
    })
}

fn sanitize_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if cleaned.is_empty() {
        "snapshot".to_string()
    } else {
        cleaned
    }
}

fn ensure_project_path(path: String) -> AppResult<PathBuf> {
    let root = PathBuf::from(path.trim());
    if !root.is_dir() {
        return Err(format!("Project folder does not exist: {}", root.display()));
    }
    Ok(root)
}

fn netstat_output() -> String {
    let mut command = Command::new("netstat.exe");
    command
        .args(["-ano", "-p", "tcp"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default()
}

fn pid_for_port(netstat: &str, port: u16) -> Option<u32> {
    let suffix = format!(":{port}");
    netstat.lines().find_map(|line| {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() >= 5
            && parts[1].ends_with(&suffix)
            && parts[3].eq_ignore_ascii_case("LISTENING")
        {
            parts[4].parse().ok()
        } else {
            None
        }
    })
}

fn process_name(pid: u32) -> Option<String> {
    let filter = format!("PID eq {pid}");
    let mut command = Command::new("tasklist.exe");
    command
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .next()
        .and_then(|line| line.split(',').next())
        .map(|value| value.trim_matches('"').to_string())
}
