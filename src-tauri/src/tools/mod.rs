use crate::state::{AppResult, Store};
use serde::Serialize;
use std::{
    fs,
    net::TcpStream,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
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
