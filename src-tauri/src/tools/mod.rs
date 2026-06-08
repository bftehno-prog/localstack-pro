use crate::state::{AppResult, Store};
use chrono::Utc;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use regex::RegexBuilder;
use serde::Serialize;
use std::{
    fs::File,
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tar::{Archive as TarArchive, Builder as TarBuilder};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

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
    pub size: u64,
    pub modified: Option<String>,
    pub language: String,
    pub read_only: bool,
    pub encoding: String,
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
pub struct FileSearchResult {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub path: String,
    pub kind: String,
    pub size: u64,
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
    read_file_with_encoding(path, "auto".to_string())
}

pub fn read_file_with_encoding(path: String, encoding: String) -> AppResult<ConfigFile> {
    let target = allowed_workspace_path(path)?;
    if !target.is_file() {
        return Err(format!("File does not exist: {}", target.display()));
    }
    let metadata = target
        .metadata()
        .map_err(|err| format!("Cannot inspect file {}: {err}", target.display()))?;
    if metadata.len() > 5_000_000 {
        return Err("File is too large for the built-in editor.".to_string());
    }
    let bytes = fs::read(&target)
        .map_err(|err| format!("Cannot read file {}: {err}", target.display()))?;
    if bytes.contains(&0) {
        return Err("Binary files cannot be opened in the built-in editor.".to_string());
    }
    let (content, encoding) = decode_text(bytes, &encoding)?;
    let modified = metadata
        .modified()
        .ok()
        .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339());
    Ok(ConfigFile {
        path: target.display().to_string(),
        content,
        size: metadata.len(),
        modified,
        language: file_language(&target),
        read_only: metadata.permissions().readonly(),
        encoding,
    })
}

pub fn write_file(path: String, content: String) -> AppResult<String> {
    write_file_with_encoding(path, content, "utf-8".to_string())
}

pub fn write_file_with_encoding(path: String, content: String, encoding: String) -> AppResult<String> {
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
    fs::write(&target, encode_text(&content, &encoding)?)
        .map_err(|err| format!("Cannot write file {}: {err}", target.display()))?;
    Ok(target.display().to_string())
}

pub fn create_file(path: String) -> AppResult<String> {
    let target = allowed_workspace_path(path)?;
    if target.exists() {
        return Err(format!("File already exists: {}", target.display()));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create parent folder: {err}"))?;
    }
    fs::write(&target, "")
        .map_err(|err| format!("Cannot create file {}: {err}", target.display()))?;
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

pub fn duplicate_path(path: String) -> AppResult<String> {
    let source = allowed_workspace_path(path)?;
    if !source.exists() {
        return Err(format!("Path does not exist: {}", source.display()));
    }
    let target = next_duplicate_path(&source)?;
    let target = allowed_workspace_path(target.display().to_string())?;
    if source.is_dir() {
        copy_dir_all(&source, &target)?;
    } else {
        fs::copy(&source, &target)
            .map_err(|err| format!("Cannot duplicate file {}: {err}", source.display()))?;
    }
    Ok(target.display().to_string())
}

pub fn copy_path(source: String, target: String, overwrite: bool) -> AppResult<String> {
    let source = allowed_workspace_path(source)?;
    let target = normalize_operation_target(&source, target)?;
    if target.exists() && !overwrite {
        return Err(format!("Target already exists: {}", target.display()));
    }
    if source.is_dir() {
        if target.exists() && overwrite {
            fs::remove_dir_all(&target)
                .map_err(|err| format!("Cannot replace target folder {}: {err}", target.display()))?;
        }
        copy_dir_all(&source, &target)?;
    } else if source.is_file() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("Cannot create target folder: {err}"))?;
        }
        fs::copy(&source, &target)
            .map_err(|err| format!("Cannot copy {}: {err}", source.display()))?;
    } else {
        return Err(format!("Source does not exist: {}", source.display()));
    }
    Ok(target.display().to_string())
}

pub fn move_path(source: String, target: String, overwrite: bool) -> AppResult<String> {
    let source = allowed_workspace_path(source)?;
    let target = normalize_operation_target(&source, target)?;
    if target.exists() {
        if !overwrite {
            return Err(format!("Target already exists: {}", target.display()));
        }
        if target.is_dir() {
            fs::remove_dir_all(&target)
                .map_err(|err| format!("Cannot replace target folder {}: {err}", target.display()))?;
        } else {
            fs::remove_file(&target)
                .map_err(|err| format!("Cannot replace target file {}: {err}", target.display()))?;
        }
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create target folder: {err}"))?;
    }
    match fs::rename(&source, &target) {
        Ok(_) => Ok(target.display().to_string()),
        Err(_) => {
            copy_path(source.display().to_string(), target.display().to_string(), true)?;
            delete_path(source.display().to_string())?;
            Ok(target.display().to_string())
        }
    }
}

pub fn chmod_path(path: String, mode: String, read_only: bool) -> AppResult<String> {
    let target = allowed_workspace_path(path)?;
    let mut permissions = target
        .metadata()
        .map_err(|err| format!("Cannot inspect permissions: {err}"))?
        .permissions();
    let readonly = read_only || matches!(mode.trim(), "400" | "440" | "444" | "500" | "550" | "555");
    permissions.set_readonly(readonly);
    fs::set_permissions(&target, permissions)
        .map_err(|err| format!("Cannot change permissions for {}: {err}", target.display()))?;
    Ok(if readonly {
        format!("{} set to read-only", target.display())
    } else {
        format!("{} set to writable", target.display())
    })
}

pub fn upload_files(sources: Vec<String>, destination: String, overwrite: bool) -> AppResult<Vec<String>> {
    let destination = allowed_workspace_path(destination)?;
    fs::create_dir_all(&destination)
        .map_err(|err| format!("Cannot create upload folder {}: {err}", destination.display()))?;
    let mut uploaded = Vec::new();
    for source in sources {
        let source_path = PathBuf::from(source.trim());
        if !source_path.exists() {
            return Err(format!("Upload source does not exist: {}", source_path.display()));
        }
        let name = source_path
            .file_name()
            .ok_or_else(|| "Cannot detect uploaded file name.".to_string())?;
        let target = destination.join(name);
        if target.exists() && !overwrite {
            return Err(format!("Target already exists: {}", target.display()));
        }
        if source_path.is_dir() {
            if target.exists() && overwrite {
                fs::remove_dir_all(&target)
                    .map_err(|err| format!("Cannot replace folder {}: {err}", target.display()))?;
            }
            copy_dir_all(&source_path, &target)?;
        } else {
            fs::copy(&source_path, &target)
                .map_err(|err| format!("Cannot upload {}: {err}", source_path.display()))?;
        }
        uploaded.push(target.display().to_string());
    }
    Ok(uploaded)
}

pub fn extract_archive_to(path: String, destination: String) -> AppResult<String> {
    let archive = allowed_workspace_path(path)?;
    if !archive.is_file() {
        return Err(format!("Archive does not exist: {}", archive.display()));
    }
    let destination = allowed_workspace_path(destination)?;
    fs::create_dir_all(&destination)
        .map_err(|err| format!("Cannot create extract folder {}: {err}", destination.display()))?;
    let lower = archive
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if lower.ends_with(".zip") {
        extract_zip_safe(&archive, &destination)?;
    } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz") {
        extract_tar_safe(&archive, &destination, lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz"))?;
    } else {
        return Err("Supported archives: zip, tar, tar.gz, tgz, gz.".to_string());
    }
    Ok(destination.display().to_string())
}

pub fn create_archive(paths: Vec<String>, target: String) -> AppResult<String> {
    if paths.is_empty() {
        return Err("Select files or folders to archive.".to_string());
    }
    let target = allowed_workspace_path(target)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("Cannot create archive folder: {err}"))?;
    }
    let sources = paths
        .into_iter()
        .map(allowed_workspace_path)
        .collect::<AppResult<Vec<_>>>()?;
    let lower = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if lower.ends_with(".zip") {
        create_zip_archive(&sources, &target)?;
    } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        create_tar_archive(&sources, &target, lower.ends_with(".tar.gz") || lower.ends_with(".tgz"))?;
    } else {
        return Err("Archive target must end with .zip, .tar, .tar.gz or .tgz.".to_string());
    }
    Ok(target.display().to_string())
}

pub fn search_file_contents(root: String, query: String, regexp: bool, case_sensitive: bool) -> AppResult<Vec<FileSearchResult>> {
    search_file_contents_advanced(
        root,
        query,
        regexp,
        case_sensitive,
        "".to_string(),
        "node_modules,.git,.next,vendor,target".to_string(),
        500,
    )
}

pub fn search_file_contents_advanced(
    root: String,
    query: String,
    regexp: bool,
    case_sensitive: bool,
    include_extensions: String,
    exclude_folders: String,
    limit: usize,
) -> AppResult<Vec<FileSearchResult>> {
    let root = allowed_workspace_path(root)?;
    if !root.is_dir() {
        return Err("Search root must be a folder.".to_string());
    }
    let query = query.trim().to_string();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let regex = if regexp {
        Some(
            RegexBuilder::new(&query)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|err| format!("Invalid RegExp: {err}"))?,
        )
    } else {
        None
    };
    let mut results = Vec::new();
    let include_extensions = split_csv(&include_extensions)
        .into_iter()
        .map(|item| item.trim_start_matches('.').to_string())
        .collect::<Vec<_>>();
    let exclude_folders = split_csv(&exclude_folders);
    search_folder(&root, &query, regex.as_ref(), case_sensitive, &include_extensions, &exclude_folders, limit.clamp(1, 5000), &mut results)?;
    Ok(results)
}

pub fn list_archive_entries(path: String) -> AppResult<Vec<ArchiveEntry>> {
    let archive = allowed_workspace_path(path)?;
    if !archive.is_file() {
        return Err(format!("Archive does not exist: {}", archive.display()));
    }
    let lower = archive
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    if lower.ends_with(".zip") {
        let file = File::open(&archive)
            .map_err(|err| format!("Cannot open archive {}: {err}", archive.display()))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|err| format!("Cannot read zip archive: {err}"))?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|err| format!("Cannot read zip entry: {err}"))?;
            entries.push(ArchiveEntry {
                path: entry.name().to_string(),
                kind: if entry.is_dir() { "folder" } else { "file" }.to_string(),
                size: entry.size(),
            });
        }
        Ok(entries)
    } else if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz") {
        list_tar_entries(&archive, lower.ends_with(".tar.gz") || lower.ends_with(".tgz") || lower.ends_with(".gz"))
    } else {
        Err("Supported archive preview: zip, tar, tar.gz, tgz, gz.".to_string())
    }
}

pub fn apply_windows_acl(path: String, identity: String, rights: String, inherit: bool) -> AppResult<String> {
    let target = allowed_workspace_path(path)?;
    let identity = identity.trim();
    if identity.is_empty() {
        return Err("Enter a Windows user or group.".to_string());
    }
    let rights = match rights.trim().to_uppercase().as_str() {
        "R" | "READ" => "R",
        "RX" | "READEXECUTE" | "READ_EXECUTE" => "RX",
        "M" | "MODIFY" => "M",
        "F" | "FULL" | "FULLCONTROL" | "FULL_CONTROL" => "F",
        _ => return Err("Supported ACL rights: R, RX, M, F.".to_string()),
    };
    let grant = if inherit {
        format!("{identity}:(OI)(CI){rights}")
    } else {
        format!("{identity}:{rights}")
    };
    let mut command = Command::new("icacls.exe");
    command
        .arg(&target)
        .arg("/grant")
        .arg(grant)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags_hidden();
    let output = command
        .output()
        .map_err(|err| format!("Cannot start icacls: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot apply Windows ACL. {detail}"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        size: target.metadata().map(|item| item.len()).unwrap_or(0),
        modified: target
            .metadata()
            .ok()
            .and_then(|item| item.modified().ok())
            .map(|time| chrono::DateTime::<Utc>::from(time).to_rfc3339()),
        language: file_language(&target),
        read_only: target
            .metadata()
            .map(|item| item.permissions().readonly())
            .unwrap_or(false),
        encoding: "utf-8".to_string(),
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

fn copy_dir_all(source: &Path, target: &Path) -> AppResult<()> {
    fs::create_dir_all(target)
        .map_err(|err| format!("Cannot create {}: {err}", target.display()))?;
    for entry in
        fs::read_dir(source).map_err(|err| format!("Cannot read {}: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("Cannot read folder entry: {err}"))?;
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &destination)?;
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("Cannot create {}: {err}", parent.display()))?;
            }
            fs::copy(&path, &destination)
                .map_err(|err| format!("Cannot copy {}: {err}", path.display()))?;
        }
    }
    Ok(())
}

fn extract_zip_safe(source: &Path, target: &Path) -> AppResult<()> {
    let file = File::open(source)
        .map_err(|err| format!("Cannot open archive {}: {err}", source.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|err| format!("Cannot read zip archive {}: {err}", source.display()))?;
    let target_root = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Cannot read zip entry: {err}"))?;
        let Some(safe_name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let destination = target.join(safe_name);
        let normalized = destination
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .unwrap_or_else(|| target_root.clone());
        if !normalized.starts_with(&target_root) {
            return Err("Archive contains an unsafe path.".to_string());
        }
        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|err| format!("Cannot create folder {}: {err}", destination.display()))?;
        } else {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("Cannot create folder {}: {err}", parent.display()))?;
            }
            let mut output = File::create(&destination)
                .map_err(|err| format!("Cannot create file {}: {err}", destination.display()))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|err| format!("Cannot extract file {}: {err}", destination.display()))?;
        }
    }
    Ok(())
}

fn extract_tar_safe(source: &Path, target: &Path, gzip: bool) -> AppResult<()> {
    let file = File::open(source)
        .map_err(|err| format!("Cannot open archive {}: {err}", source.display()))?;
    if gzip {
        let decoder = GzDecoder::new(file);
        unpack_tar_entries(TarArchive::new(decoder), target)
    } else {
        unpack_tar_entries(TarArchive::new(file), target)
    }
}

fn list_tar_entries(source: &Path, gzip: bool) -> AppResult<Vec<ArchiveEntry>> {
    let file = File::open(source)
        .map_err(|err| format!("Cannot open archive {}: {err}", source.display()))?;
    if gzip {
        let decoder = GzDecoder::new(file);
        collect_tar_entries(TarArchive::new(decoder))
    } else {
        collect_tar_entries(TarArchive::new(file))
    }
}

fn collect_tar_entries<R: Read>(mut archive: TarArchive<R>) -> AppResult<Vec<ArchiveEntry>> {
    let mut entries = Vec::new();
    for entry in archive
        .entries()
        .map_err(|err| format!("Cannot read tar entries: {err}"))?
    {
        let entry = entry.map_err(|err| format!("Cannot read tar entry: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("Cannot read tar entry path: {err}"))?
            .to_string_lossy()
            .to_string();
        let header = entry.header();
        entries.push(ArchiveEntry {
            path,
            kind: if header.entry_type().is_dir() { "folder" } else { "file" }.to_string(),
            size: header.size().unwrap_or(0),
        });
    }
    Ok(entries)
}

fn unpack_tar_entries<R: Read>(mut archive: TarArchive<R>, target: &Path) -> AppResult<()> {
    let target_root = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
    for entry in archive
        .entries()
        .map_err(|err| format!("Cannot read tar entries: {err}"))?
    {
        let mut entry = entry.map_err(|err| format!("Cannot read tar entry: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("Cannot read tar entry path: {err}"))?
            .to_path_buf();
        if path.is_absolute() || path.components().any(|part| matches!(part, std::path::Component::ParentDir)) {
            return Err("Archive contains an unsafe path.".to_string());
        }
        let destination = target.join(path);
        let normalized = destination
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .unwrap_or_else(|| target_root.clone());
        if !normalized.starts_with(&target_root) {
            return Err("Archive contains an unsafe path.".to_string());
        }
        entry
            .unpack(&destination)
            .map_err(|err| format!("Cannot unpack {}: {err}", destination.display()))?;
    }
    Ok(())
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn create_zip_archive(sources: &[PathBuf], target: &Path) -> AppResult<()> {
    let file = File::create(target)
        .map_err(|err| format!("Cannot create archive {}: {err}", target.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for source in sources {
        let base = source
            .parent()
            .ok_or_else(|| "Cannot detect archive source folder.".to_string())?;
        add_path_to_zip(&mut zip, source, base, options)?;
    }
    zip.finish()
        .map_err(|err| format!("Cannot finish zip archive: {err}"))?;
    Ok(())
}

fn add_path_to_zip(zip: &mut ZipWriter<File>, path: &Path, base: &Path, options: SimpleFileOptions) -> AppResult<()> {
    let relative = path.strip_prefix(base).unwrap_or(path);
    let name = relative.to_string_lossy().replace('\\', "/");
    if path.is_dir() {
        if !name.is_empty() {
            zip.add_directory(format!("{name}/"), options)
                .map_err(|err| format!("Cannot add folder to zip: {err}"))?;
        }
        for entry in fs::read_dir(path).map_err(|err| format!("Cannot read {}: {err}", path.display()))? {
            let entry = entry.map_err(|err| format!("Cannot read folder entry: {err}"))?;
            add_path_to_zip(zip, &entry.path(), base, options)?;
        }
    } else {
        zip.start_file(name, options)
            .map_err(|err| format!("Cannot add file to zip: {err}"))?;
        let mut file = File::open(path)
            .map_err(|err| format!("Cannot open {}: {err}", path.display()))?;
        std::io::copy(&mut file, zip)
            .map_err(|err| format!("Cannot write zip file: {err}"))?;
    }
    Ok(())
}

fn create_tar_archive(sources: &[PathBuf], target: &Path, gzip: bool) -> AppResult<()> {
    let file = File::create(target)
        .map_err(|err| format!("Cannot create archive {}: {err}", target.display()))?;
    if gzip {
        let encoder = GzEncoder::new(file, Compression::default());
        let mut tar = TarBuilder::new(encoder);
        add_sources_to_tar(&mut tar, sources)?;
        tar.finish().map_err(|err| format!("Cannot finish tar archive: {err}"))?;
    } else {
        let mut tar = TarBuilder::new(file);
        add_sources_to_tar(&mut tar, sources)?;
        tar.finish().map_err(|err| format!("Cannot finish tar archive: {err}"))?;
    }
    Ok(())
}

fn add_sources_to_tar<W: Write>(tar: &mut TarBuilder<W>, sources: &[PathBuf]) -> AppResult<()> {
    for source in sources {
        let name = source
            .file_name()
            .ok_or_else(|| "Cannot detect archive item name.".to_string())?;
        if source.is_dir() {
            tar.append_dir_all(name, source)
                .map_err(|err| format!("Cannot add folder to tar: {err}"))?;
        } else {
            tar.append_path_with_name(source, name)
                .map_err(|err| format!("Cannot add file to tar: {err}"))?;
        }
    }
    Ok(())
}

fn search_folder(
    folder: &Path,
    query: &str,
    regex: Option<&regex::Regex>,
    case_sensitive: bool,
    include_extensions: &[String],
    exclude_folders: &[String],
    limit: usize,
    results: &mut Vec<FileSearchResult>,
) -> AppResult<()> {
    if results.len() >= limit {
        return Ok(());
    }
    for entry in fs::read_dir(folder).map_err(|err| format!("Cannot search {}: {err}", folder.display()))? {
        let entry = entry.map_err(|err| format!("Cannot read search entry: {err}"))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if exclude_folders.iter().any(|item| item.eq_ignore_ascii_case(&name)) {
                continue;
            }
            search_folder(&path, query, regex, case_sensitive, include_extensions, exclude_folders, limit, results)?;
            if results.len() >= limit {
                return Ok(());
            }
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if !include_extensions.is_empty() {
            let ext = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !include_extensions.iter().any(|item| item.eq_ignore_ascii_case(&ext)) {
                continue;
            }
        }
        let metadata = match path.metadata() {
            Ok(value) => value,
            Err(_) => continue,
        };
        if metadata.len() > 2_000_000 {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if bytes.contains(&0) {
            continue;
        }
        let (content, _) = match decode_text(bytes, "auto") {
            Ok(value) => value,
            Err(_) => continue,
        };
        for (index, line) in content.lines().enumerate() {
            let matched = if let Some(regex) = regex {
                regex.find(line).map(|hit| hit.start() + 1)
            } else if case_sensitive {
                line.find(query).map(|column| column + 1)
            } else {
                line.to_lowercase()
                    .find(&query.to_lowercase())
                    .map(|column| column + 1)
            };
            if let Some(column) = matched {
                results.push(FileSearchResult {
                    path: path.display().to_string(),
                    line: index + 1,
                    column,
                    preview: line.trim().chars().take(240).collect(),
                });
                if results.len() >= limit {
                    return Ok(());
                }
            }
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

fn normalize_operation_target(source: &Path, target: String) -> AppResult<PathBuf> {
    let mut target = PathBuf::from(target.trim());
    if target.as_os_str().is_empty() || !target.is_absolute() {
        return Err("Target path must be absolute.".to_string());
    }
    if target.exists() && target.is_dir() {
        let name = source
            .file_name()
            .ok_or_else(|| "Cannot detect source name.".to_string())?;
        target = target.join(name);
    }
    allowed_workspace_path(target.display().to_string())
}

fn decode_text(bytes: Vec<u8>, encoding: &str) -> AppResult<(String, String)> {
    let normalized = normalize_encoding(encoding);
    if normalized == "auto" {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            return Ok((
                String::from_utf8(bytes[3..].to_vec())
                    .map_err(|_| "This file is not valid UTF-8 text.".to_string())?,
                "utf-8-bom".to_string(),
            ));
        }
        if bytes.starts_with(&[0xFF, 0xFE]) {
            return decode_utf16(&bytes[2..], true).map(|content| (content, "utf-16le".to_string()));
        }
        if bytes.starts_with(&[0xFE, 0xFF]) {
            return decode_utf16(&bytes[2..], false).map(|content| (content, "utf-16be".to_string()));
        }
        return Ok((
            String::from_utf8(bytes)
                .map_err(|_| "This file is not valid UTF-8 text. Try another encoding.".to_string())?,
            "utf-8".to_string(),
        ));
    }
    match normalized.as_str() {
        "utf-8" => Ok((
            String::from_utf8(bytes)
                .map_err(|_| "This file is not valid UTF-8 text.".to_string())?,
            "utf-8".to_string(),
        )),
        "utf-8-bom" => {
            let slice = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                bytes[3..].to_vec()
            } else {
                bytes
            };
            Ok((
                String::from_utf8(slice)
                    .map_err(|_| "This file is not valid UTF-8 text.".to_string())?,
                "utf-8-bom".to_string(),
            ))
        }
        "utf-16le" => decode_utf16(&bytes, true).map(|content| (content, "utf-16le".to_string())),
        "utf-16be" => decode_utf16(&bytes, false).map(|content| (content, "utf-16be".to_string())),
        _ => Err("Supported encodings: auto, utf-8, utf-8-bom, utf-16le, utf-16be.".to_string()),
    }
}

fn encode_text(content: &str, encoding: &str) -> AppResult<Vec<u8>> {
    match normalize_encoding(encoding).as_str() {
        "auto" | "utf-8" => Ok(content.as_bytes().to_vec()),
        "utf-8-bom" => {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(content.as_bytes());
            Ok(bytes)
        }
        "utf-16le" => {
            let mut bytes = vec![0xFF, 0xFE];
            for code in content.encode_utf16() {
                bytes.extend_from_slice(&code.to_le_bytes());
            }
            Ok(bytes)
        }
        "utf-16be" => {
            let mut bytes = vec![0xFE, 0xFF];
            for code in content.encode_utf16() {
                bytes.extend_from_slice(&code.to_be_bytes());
            }
            Ok(bytes)
        }
        _ => Err("Supported encodings: utf-8, utf-8-bom, utf-16le, utf-16be.".to_string()),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> AppResult<String> {
    if bytes.len() % 2 != 0 {
        return Err("Invalid UTF-16 byte length.".to_string());
    }
    let values = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&values).map_err(|_| "This file is not valid UTF-16 text.".to_string())
}

fn normalize_encoding(encoding: &str) -> String {
    encoding.trim().to_lowercase().replace('_', "-")
}

fn next_duplicate_path(source: &Path) -> AppResult<PathBuf> {
    let parent = source
        .parent()
        .ok_or_else(|| "Cannot duplicate this path.".to_string())?;
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .or_else(|| source.file_name().and_then(|value| value.to_str()))
        .unwrap_or("copy");
    let extension = source.extension().and_then(|value| value.to_str());
    for index in 1..1000 {
        let suffix = if index == 1 {
            " copy".to_string()
        } else {
            format!(" copy {index}")
        };
        let file_name = match extension {
            Some(ext) if source.is_file() => format!("{stem}{suffix}.{ext}"),
            _ => format!("{stem}{suffix}"),
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("Cannot find a free duplicate name.".to_string())
}

fn file_language(path: &Path) -> String {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "php" | "phtml" => "PHP",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" => "TypeScript",
        "tsx" | "jsx" => "React",
        "json" => "JSON",
        "html" | "htm" => "HTML",
        "css" => "CSS",
        "scss" | "sass" => "SCSS",
        "md" | "markdown" => "Markdown",
        "xml" => "XML",
        "yml" | "yaml" => "YAML",
        "toml" => "TOML",
        "ini" | "conf" | "cnf" => "Config",
        "env" => "Environment",
        "sql" => "SQL",
        "log" => "Log",
        "rs" => "Rust",
        "py" => "Python",
        "sh" | "bat" | "cmd" | "ps1" => "Script",
        _ => "Text",
    }
    .to_string()
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
