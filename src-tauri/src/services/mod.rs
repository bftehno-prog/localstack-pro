use crate::state::{AppResult, LogLevel, ServiceInfo, ServiceStatus, Store};
use chrono::Utc;
use std::{
    fs,
    net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, MutexGuard},
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
#[cfg(windows)]
const SERVER_PROCESS_FLAGS: u32 = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;

static SERVICE_OPERATION_LOCK: Mutex<()> = Mutex::new(());

fn service_operation_guard() -> AppResult<MutexGuard<'static, ()>> {
    SERVICE_OPERATION_LOCK.lock().map_err(|_| {
        "Another service operation is still finishing. Try again in a moment.".to_string()
    })
}

pub fn start_service(service_id: String) -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let index = snapshot
        .services
        .iter()
        .position(|service| service.id == service_id)
        .ok_or_else(|| "Service not found.".to_string())?;
    start_service_at(&store, &mut snapshot, index)?;
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn stop_service(service_id: String) -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let index = snapshot
        .services
        .iter()
        .position(|service| service.id == service_id)
        .ok_or_else(|| "Service not found.".to_string())?;
    stop_service_at(&store, &mut snapshot, index)?;
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn restart_service(service_id: String) -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let index = snapshot
        .services
        .iter()
        .position(|service| service.id == service_id)
        .ok_or_else(|| "Service not found.".to_string())?;
    let _ = stop_service_at(&store, &mut snapshot, index);
    start_service_at(&store, &mut snapshot, index)?;
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn start_service_profile(service_ids: Vec<String>) -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    for service_id in service_ids {
        let Some(index) = snapshot
            .services
            .iter()
            .position(|service| service.id == service_id)
        else {
            continue;
        };
        if snapshot.services[index].status == ServiceStatus::Running
            && service_is_live(&snapshot.services[index])
        {
            continue;
        }
        if let Err(err) = start_service_at_quick(&store, &mut snapshot, index) {
            let name = snapshot.services[index].name.clone();
            store.log(&mut snapshot, LogLevel::Error, &name, err.clone(), None);
            snapshot.services[index].last_error = Some(err);
            snapshot.services[index].status = ServiceStatus::Error;
        }
    }
    sync_host_statuses(&mut snapshot);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn start_all() -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    for index in 0..snapshot.services.len() {
        if !should_start_in_bulk(&snapshot, index) {
            continue;
        }
        if let Err(err) = start_service_at_quick(&store, &mut snapshot, index) {
            let name = snapshot.services[index].name.clone();
            store.log(&mut snapshot, LogLevel::Error, &name, err.clone(), None);
            snapshot.services[index].last_error = Some(err);
            snapshot.services[index].status = ServiceStatus::Error;
        }
    }
    sync_host_statuses(&mut snapshot);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn stop_all() -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    for index in 0..snapshot.services.len() {
        let _ = stop_service_at(&store, &mut snapshot, index);
    }
    sync_host_statuses(&mut snapshot);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn restart_all() -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    for index in 0..snapshot.services.len() {
        if snapshot.services[index].status != ServiceStatus::Running
            && !should_start_in_bulk(&snapshot, index)
        {
            continue;
        }
        let _ = stop_service_at(&store, &mut snapshot, index);
        if let Err(err) = start_service_at_quick(&store, &mut snapshot, index) {
            snapshot.services[index].last_error = Some(err);
            snapshot.services[index].status = ServiceStatus::Error;
        }
    }
    sync_host_statuses(&mut snapshot);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn save_service(service: ServiceInfo) -> AppResult<crate::state::AppSnapshot> {
    let _guard = service_operation_guard()?;
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if let Some(existing) = snapshot
        .services
        .iter_mut()
        .find(|item| item.id == service.id)
    {
        *existing = service;
    } else {
        snapshot.services.push(service);
    }
    store.ensure_service_files(&snapshot);
    store.save(&snapshot)?;
    Ok(snapshot)
}

fn start_service_at(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    start_service_at_mode(store, snapshot, index, true)
}

fn start_service_at_quick(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    start_service_at_mode(store, snapshot, index, false)
}

fn start_service_at_mode(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
    wait_until_ready: bool,
) -> AppResult<()> {
    let mut executable = snapshot.services[index].executable_path.clone();
    let name = snapshot.services[index].name.clone();
    if snapshot.services[index].status == ServiceStatus::Running {
        if service_is_live(&snapshot.services[index]) {
            return Ok(());
        }
        snapshot.services[index].status = ServiceStatus::Stopped;
        snapshot.services[index].pid = None;
        snapshot.services[index].started_at = None;
        snapshot.services[index].uptime_seconds = 0;
    }
    if wait_until_ready {
        if let Some(service_name) = registered_windows_service(&snapshot.services[index].id) {
            match windows_service_status(service_name) {
                Some(ServiceStatus::Running) => {
                    mark_running_from_ports(snapshot, index);
                    store.log(
                        snapshot,
                        LogLevel::Info,
                        &name,
                        format!("{name} is already running as Windows service {service_name}"),
                        None,
                    );
                    return Ok(());
                }
                Some(ServiceStatus::Stopped) if start_windows_service(service_name) => {
                    wait_for_service_ports(&snapshot.services[index].ports)?;
                    mark_running_from_ports(snapshot, index);
                    store.log(
                        snapshot,
                        LogLevel::Info,
                        &name,
                        format!("{name} started through Windows service {service_name}"),
                        None,
                    );
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    if snapshot.services[index].id == "dns-helper" {
        let current_exe = std::env::current_exe()
            .map_err(|err| format!("Cannot resolve DNS Helper executable: {err}"))?;
        snapshot.services[index].executable_path = current_exe.display().to_string();
        snapshot.services[index].arguments = vec!["--localstack-dns-helper".to_string()];
        let preferred = snapshot.services[index]
            .ports
            .first()
            .copied()
            .unwrap_or(5353);
        let port = if udp_port_available(preferred) {
            preferred
        } else {
            (53535..53600)
                .find(|port| udp_port_available(*port))
                .ok_or_else(|| "DNS Helper cannot find a free UDP port.".to_string())?
        };
        snapshot.services[index].ports = vec![port];
        executable = snapshot.services[index].executable_path.clone();
    }
    if !Path::new(&executable).exists() {
        if let Some(detected) =
            crate::state::detect_service_executable(&snapshot.services[index].id)
        {
            snapshot.services[index].executable_path = detected.display().to_string();
            snapshot.services[index].arguments =
                crate::state::service_default_arguments(&snapshot.services[index].id, &detected);
            snapshot.services[index].last_error = None;
            executable = snapshot.services[index].executable_path.clone();
        }
    }
    if !Path::new(&executable).exists() {
        let message = format!(
            "{name} cannot start because the executable was not found: {executable}. Install the service binary or set the correct executable path in Services."
        );
        snapshot.services[index].last_error = Some(message.clone());
        snapshot.services[index].status = ServiceStatus::Error;
        return Err(message);
    }
    prepare_runtime_config(store, snapshot, index)?;
    if !snapshot.services[index].ports.is_empty()
        && snapshot.services[index]
            .ports
            .iter()
            .all(|port| port_accepting(*port))
    {
        mark_running_from_ports(snapshot, index);
        store.log(
            snapshot,
            LogLevel::Info,
            &name,
            format!("{name} is already listening on configured port(s)"),
            None,
        );
        return Ok(());
    }
    let mut selected_ports = Vec::new();
    let mut owned_ports = Vec::new();
    for port in snapshot.services[index].ports.clone() {
        if port_available(port) {
            selected_ports.push(port);
        } else if port_owned_by_service(port, &snapshot.services[index]) {
            owned_ports.push(port);
        } else {
            let owner = port_owner_pid(port)
                .map(|pid| {
                    port_owner_process_name(pid)
                        .map(|process| format!("{process} pid={pid}"))
                        .unwrap_or_else(|| format!("pid={pid}"))
                })
                .unwrap_or_else(|| "unknown process".to_string());
            let message = format!(
                "{name} cannot start because port {port} is already used by {owner}. Stop the conflicting process or change the service port."
            );
            snapshot.services[index].last_error = Some(message.clone());
            snapshot.services[index].status = ServiceStatus::Error;
            return Err(message);
        }
    }
    if !owned_ports.is_empty() {
        mark_running_from_ports(snapshot, index);
        store.log(
            snapshot,
            LogLevel::Info,
            &name,
            format!("{name} is already listening on port(s) {:?}", owned_ports),
            None,
        );
        return Ok(());
    }
    snapshot.services[index].ports = selected_ports.clone();
    let mut command = Command::new(&executable);
    command.args(snapshot.services[index].arguments.clone());
    if let Some(parent) = Path::new(&executable).parent() {
        command.current_dir(parent);
    }
    if snapshot.services[index].id == "dns-helper" {
        if let Some(port) = snapshot.services[index].ports.first() {
            command.env("LOCALSTACK_DNS_PORT", port.to_string());
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(SERVER_PROCESS_FLAGS);
    let mut child = command
        .spawn()
        .map_err(|err| format!("Cannot start {name}: {err}"))?;
    if snapshot.services[index].id == "dns-helper" {
        thread::sleep(Duration::from_millis(180));
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "{name} exited before becoming ready. Exit code: {:?}",
                status.code()
            ));
        }
    } else if !selected_ports.is_empty() {
        if wait_until_ready {
            wait_for_spawned_service_ports(
                &mut child,
                &snapshot.services[index].id,
                &selected_ports,
            )
            .map_err(|err| format!("{name} did not become ready: {err}"))?;
        } else {
            wait_for_spawned_service_ports_quick(
                &mut child,
                &snapshot.services[index].id,
                &selected_ports,
            )
            .map_err(|err| format!("{name} did not start: {err}"))?;
        }
    }
    snapshot.services[index].pid = snapshot.services[index]
        .ports
        .iter()
        .find_map(|port| port_owner_pid(*port))
        .or_else(|| Some(child.id()));
    snapshot.services[index].status =
        if selected_ports.is_empty() || selected_ports.iter().all(|port| port_accepting(*port)) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Starting
        };
    snapshot.services[index].started_at = Some(Utc::now().timestamp());
    snapshot.services[index].last_error = None;
    store.log(
        snapshot,
        LogLevel::Info,
        &name,
        format!("{name} started with PID {} (native executable)", child.id()),
        None,
    );
    Ok(())
}

fn stop_service_at(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    let name = snapshot.services[index].name.clone();
    if let Some(service_name) = registered_windows_service(&snapshot.services[index].id) {
        if matches!(
            windows_service_status(service_name),
            Some(ServiceStatus::Running)
        ) && stop_windows_service(service_name)
        {
            snapshot.services[index].pid = None;
            snapshot.services[index].status = ServiceStatus::Stopped;
            snapshot.services[index].started_at = None;
            snapshot.services[index].uptime_seconds = 0;
            snapshot.services[index].cpu = 0.0;
            snapshot.services[index].memory_mb = 0;
            store.log(
                snapshot,
                LogLevel::Info,
                &name,
                format!("{name} stopped through Windows service {service_name}"),
                None,
            );
            return Ok(());
        }
    }
    if let Some(pid) = snapshot.services[index].pid {
        let mut command = Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let status = command.status();
        if let Err(err) = status {
            return Err(format!("Cannot stop {name}: {err}"));
        }
        store.log(
            snapshot,
            LogLevel::Info,
            &name,
            format!("{name} stopped"),
            None,
        );
        if let Some(image_name) = service_process_image(&snapshot.services[index].id) {
            let _ = kill_process_image(image_name);
        }
    } else if let Some(image_name) = service_process_image(&snapshot.services[index].id) {
        let _ = kill_process_image(image_name);
    }
    snapshot.services[index].pid = None;
    snapshot.services[index].status = ServiceStatus::Stopped;
    snapshot.services[index].started_at = None;
    snapshot.services[index].uptime_seconds = 0;
    snapshot.services[index].cpu = 0.0;
    snapshot.services[index].memory_mb = 0;
    Ok(())
}

fn kill_process_image(image_name: &str) -> std::io::Result<std::process::ExitStatus> {
    let mut command = Command::new("taskkill");
    command.args(["/IM", image_name, "/T", "/F"]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command.status()
}

fn port_available(port: u16) -> bool {
    !port_accepting(port) && TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn udp_port_available(port: u16) -> bool {
    UdpSocket::bind(("0.0.0.0", port)).is_ok()
}

fn wait_for_bound_port(port: u16) -> AppResult<()> {
    for _ in 0..18 {
        if port_accepting(port) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!("127.0.0.1:{port} did not start listening."))
}

fn wait_for_spawned_service_ports(
    child: &mut std::process::Child,
    service_id: &str,
    ports: &[u16],
) -> AppResult<()> {
    let (attempts, pause) = match service_id {
        "mysql" | "mariadb" | "postgresql" => (96, Duration::from_millis(250)),
        _ => (24, Duration::from_millis(75)),
    };
    for _ in 0..attempts {
        if ports.iter().all(|port| port_accepting(*port)) {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "process exited before opening port(s) {}. Exit code: {:?}",
                ports
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                status.code()
            ));
        }
        thread::sleep(pause);
    }
    Err(format!(
        "127.0.0.1:{} did not start listening.",
        ports
            .first()
            .map(u16::to_string)
            .unwrap_or_else(|| "-".to_string())
    ))
}

fn wait_for_spawned_service_ports_quick(
    child: &mut std::process::Child,
    service_id: &str,
    ports: &[u16],
) -> AppResult<()> {
    let (attempts, pause) = match service_id {
        "mysql" | "mariadb" | "postgresql" => (12, Duration::from_millis(160)),
        _ => (8, Duration::from_millis(80)),
    };
    for _ in 0..attempts {
        if ports.iter().all(|port| port_accepting(*port)) {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!(
                "process exited before opening port(s) {}. Exit code: {:?}",
                ports
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                status.code()
            ));
        }
        thread::sleep(pause);
    }
    Ok(())
}

fn service_is_live(service: &ServiceInfo) -> bool {
    if !service.ports.is_empty() && service.ports.iter().all(|port| port_accepting(*port)) {
        return true;
    }
    service.pid.is_some_and(process_exists)
}

fn process_exists(pid: u32) -> bool {
    let mut command = Command::new("tasklist");
    command.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\"")))
        .unwrap_or(false)
}

fn should_start_in_bulk(snapshot: &crate::state::AppSnapshot, index: usize) -> bool {
    let service = &snapshot.services[index];
    if !service.autostart {
        return false;
    }
    if matches!(
        service.status,
        ServiceStatus::Running | ServiceStatus::Starting
    ) {
        return true;
    }
    let has_apache_hosts = snapshot
        .hosts
        .iter()
        .any(|host| !host.web_server.eq_ignore_ascii_case("nginx"));
    let has_nginx_hosts = snapshot
        .hosts
        .iter()
        .any(|host| host.web_server.eq_ignore_ascii_case("nginx"));
    let has_node_hosts = snapshot.hosts.iter().any(is_node_host);
    let wanted = match service.id.as_str() {
        "apache" => has_apache_hosts,
        "nginx" => has_nginx_hosts,
        "mysql" | "redis" | "mailpit" => true,
        "node-proxy" => has_node_hosts,
        _ => false,
    };
    wanted
        && (Path::new(&service.executable_path).exists()
            || registered_windows_service(&service.id).is_some())
}

fn wait_for_service_ports(ports: &[u16]) -> AppResult<()> {
    for port in ports {
        wait_for_bound_port(*port)?;
    }
    Ok(())
}

fn mark_running_from_ports(snapshot: &mut crate::state::AppSnapshot, index: usize) {
    snapshot.services[index].pid = None;
    snapshot.services[index].status = ServiceStatus::Running;
    snapshot.services[index].started_at = Some(Utc::now().timestamp());
    snapshot.services[index].last_error = None;
}

fn service_process_image(service_id: &str) -> Option<&'static str> {
    match service_id {
        "apache" => Some("httpd.exe"),
        "nginx" => Some("nginx.exe"),
        "mysql" => Some("mysqld.exe"),
        "mailpit" => Some("mailpit.exe"),
        _ => None,
    }
}

fn sync_host_statuses(snapshot: &mut crate::state::AppSnapshot) {
    let apache = snapshot
        .services
        .iter()
        .any(|service| service.id == "apache" && service.status == ServiceStatus::Running);
    let nginx = snapshot
        .services
        .iter()
        .any(|service| service.id == "nginx" && service.status == ServiceStatus::Running);
    for host in &mut snapshot.hosts {
        let uses_nginx = host.web_server.eq_ignore_ascii_case("nginx");
        host.status = if (uses_nginx && nginx) || (!uses_nginx && apache) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        };
    }
}

fn registered_windows_service(service_id: &str) -> Option<&'static str> {
    match service_id {
        "mariadb" => Some("MariaDB"),
        "postgresql" => Some("postgresql-x64-16"),
        "redis" => Some("Redis"),
        _ => None,
    }
}

fn windows_service_status(service_name: &str) -> Option<ServiceStatus> {
    let mut command = Command::new("sc.exe");
    command.args(["query", service_name]);
    command.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).to_uppercase();
    if text.contains("RUNNING") {
        Some(ServiceStatus::Running)
    } else if text.contains("STOPPED") {
        Some(ServiceStatus::Stopped)
    } else {
        None
    }
}

fn start_windows_service(service_name: &str) -> bool {
    let mut command = Command::new("sc.exe");
    command.args(["start", service_name]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn stop_windows_service(service_name: &str) -> bool {
    let mut command = Command::new("sc.exe");
    command.args(["stop", service_name]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn port_owner_pid(port: u16) -> Option<u32> {
    let mut command = Command::new("netstat.exe");
    command.args(["-ano", "-p", "tcp"]);
    command.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let needle = format!(":{port}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains("LISTENING") && line.contains(&needle))
        .find_map(|line| line.split_whitespace().last()?.parse::<u32>().ok())
}

fn port_owned_by_service(port: u16, service: &ServiceInfo) -> bool {
    let Some(pid) = port_owner_pid(port) else {
        return false;
    };
    let Some(process_name) = port_owner_process_name(pid) else {
        return false;
    };
    let process_name = process_name.to_lowercase();
    expected_process_names(service)
        .iter()
        .any(|name| process_name.eq_ignore_ascii_case(name))
}

fn expected_process_names(service: &ServiceInfo) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(file_name) = Path::new(&service.executable_path)
        .file_name()
        .and_then(|name| name.to_str())
    {
        names.push(file_name.to_lowercase());
    }
    let aliases: &[&str] = match service.id.as_str() {
        "apache" => &["httpd.exe"],
        "nginx" => &["nginx.exe"],
        "mysql" => &["mysqld.exe"],
        "mariadb" => &["mariadbd.exe", "mysqld.exe"],
        "postgresql" => &["postgres.exe"],
        "redis" => &["redis-server.exe"],
        "mailpit" => &["mailpit.exe"],
        "node-proxy" => &["node.exe"],
        "mongodb" => &["mongod.exe"],
        "memcached" => &["memcached.exe"],
        "minio" => &["minio.exe"],
        "elasticsearch" => &["elasticsearch.exe", "java.exe", "elasticsearch.bat"],
        "rabbitmq" => &["rabbitmq-server.bat", "erl.exe", "beam.smp"],
        "caddy" => &["caddy.exe"],
        "dns-helper" => &["localstack-pro.exe"],
        _ => &[],
    };
    names.extend(aliases.iter().map(|name| name.to_string()));
    names.sort();
    names.dedup();
    names
}

fn port_owner_process_name(pid: u32) -> Option<String> {
    let mut command = Command::new("tasklist.exe");
    command.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
    command.stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    let first = line.split(',').next()?.trim().trim_matches('"').to_string();
    if first.eq_ignore_ascii_case("INFO:") || first.is_empty() {
        None
    } else {
        Some(first)
    }
}

fn port_accepting(port: u16) -> bool {
    let Ok(addresses) = ("localhost", port).to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(40)).is_ok())
}

fn prepare_runtime_config(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    match snapshot.services[index].id.as_str() {
        "apache" => prepare_apache_runtime(store, snapshot, index),
        "nginx" => prepare_nginx_runtime(store, snapshot, index),
        "mysql" => prepare_mysql_runtime(store, snapshot, index),
        "node-proxy" => {
            let script = store.dir.join("configs").join("node-proxy.js");
            fs::write(&script, node_proxy_script())
                .map_err(|err| format!("Cannot write Node.js proxy script: {err}"))?;
            snapshot.services[index].arguments = vec![script.display().to_string()];
            snapshot.services[index].ports = vec![3000];
            snapshot.services[index].config_path = script.display().to_string();
            snapshot.services[index].log_path = store
                .dir
                .join("logs")
                .join("node-proxy.log")
                .display()
                .to_string();
            Ok(())
        }
        "mongodb" => {
            let data = store.dir.join("services").join("mongodb").join("data");
            let logs = store.dir.join("logs");
            fs::create_dir_all(&data)
                .map_err(|err| format!("Cannot create MongoDB data folder: {err}"))?;
            fs::create_dir_all(&logs)
                .map_err(|err| format!("Cannot create MongoDB logs folder: {err}"))?;
            let log = logs.join("mongodb.log");
            snapshot.services[index].arguments = vec![
                "--dbpath".to_string(),
                data.display().to_string(),
                "--bind_ip".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "27017".to_string(),
                "--logpath".to_string(),
                log.display().to_string(),
            ];
            snapshot.services[index].log_path = log.display().to_string();
            Ok(())
        }
        "minio" => {
            let data = store.dir.join("services").join("minio").join("data");
            let logs = store.dir.join("logs");
            fs::create_dir_all(&data)
                .map_err(|err| format!("Cannot create MinIO data folder: {err}"))?;
            fs::create_dir_all(&logs)
                .map_err(|err| format!("Cannot create MinIO logs folder: {err}"))?;
            snapshot.services[index].arguments = vec![
                "server".to_string(),
                data.display().to_string(),
                "--address".to_string(),
                "127.0.0.1:9000".to_string(),
                "--console-address".to_string(),
                "127.0.0.1:9001".to_string(),
            ];
            snapshot.services[index].log_path = logs.join("minio.log").display().to_string();
            Ok(())
        }
        "caddy" => {
            let config = store.dir.join("configs").join("Caddyfile");
            if let Some(parent) = config.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("Cannot create Caddy config folder: {err}"))?;
            }
            if !config.exists() {
                fs::write(&config, "{\n    admin localhost:2019\n}\n:8081 {\n    respond \"LocalStack Pro Caddy is running\"\n}\n")
                    .map_err(|err| format!("Cannot write Caddyfile: {err}"))?;
            }
            snapshot.services[index].arguments = vec![
                "run".to_string(),
                "--config".to_string(),
                config.display().to_string(),
            ];
            snapshot.services[index].config_path = config.display().to_string();
            snapshot.services[index].log_path = store
                .dir
                .join("logs")
                .join("caddy.log")
                .display()
                .to_string();
            Ok(())
        }
        _ => Ok(()),
    }
}

fn prepare_mysql_runtime(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    let executable = PathBuf::from(&snapshot.services[index].executable_path);
    let basedir = executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "Cannot resolve MySQL base directory.".to_string())?;
    let runtime = store.dir.join("services").join("mysql");
    let data = runtime.join("data");
    let tmp = runtime.join("tmp");
    let logs = store.dir.join("logs");
    fs::create_dir_all(&data).map_err(|err| format!("Cannot create MySQL data folder: {err}"))?;
    fs::create_dir_all(&tmp).map_err(|err| format!("Cannot create MySQL temp folder: {err}"))?;
    fs::create_dir_all(&logs).map_err(|err| format!("Cannot create MySQL log folder: {err}"))?;
    let config = runtime.join("my.ini");
    let error_log = logs.join("mysql-error.log");
    let content = format!(
        r#"[mysqld]
basedir="{basedir}"
datadir="{data}"
tmpdir="{tmp}"
port=3306
bind-address=127.0.0.1
mysqlx=0
log-error="{error_log}"
pid-file="{pid_file}"
character-set-server=utf8mb4
collation-server=utf8mb4_unicode_ci

[client]
host=127.0.0.1
port=3306
user=root
"#,
        basedir = slash(basedir),
        data = slash(&data),
        tmp = slash(&tmp),
        error_log = slash(&error_log),
        pid_file = slash(&runtime.join("mysql.pid"))
    );
    fs::write(&config, content).map_err(|err| format!("Cannot write MySQL config: {err}"))?;
    if !data.join("auto.cnf").is_file() {
        initialize_mysql_datadir(&executable, &config)?;
    }
    snapshot.services[index].arguments = vec![format!("--defaults-file={}", config.display())];
    snapshot.services[index].config_path = config.display().to_string();
    snapshot.services[index].log_path = error_log.display().to_string();
    snapshot.services[index].ports = vec![3306];
    Ok(())
}

fn initialize_mysql_datadir(executable: &Path, config: &Path) -> AppResult<()> {
    let mut command = Command::new(executable);
    command
        .arg(format!("--defaults-file={}", config.display()))
        .arg("--initialize-insecure")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot initialize MySQL data directory: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(format!(
        "Cannot initialize MySQL data directory. Exit code {:?}. {}",
        output.status.code(),
        detail
    ))
}

fn prepare_apache_runtime(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    let executable = PathBuf::from(&snapshot.services[index].executable_path);
    let server_root = executable
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "Cannot resolve Apache server root.".to_string())?;
    let runtime = store.dir.join("configs").join("apache-runtime");
    fs::create_dir_all(&runtime).map_err(|err| format!("Cannot create Apache runtime: {err}"))?;
    fs::create_dir_all(store.dir.join("logs"))
        .map_err(|err| format!("Cannot create logs folder: {err}"))?;
    ensure_host_runtime_dirs(snapshot)?;
    let document_root = default_document_root(store)?;
    let tools_root = ensure_tools_root(store)?;
    let ssl_enabled = snapshot
        .hosts
        .iter()
        .any(|host| host.web_server.eq_ignore_ascii_case("apache") && host.ssl);
    let node_proxy_enabled = snapshot.hosts.iter().any(is_node_host);
    let ssl_block = if ssl_enabled {
        r#"Listen 443
LoadModule ssl_module modules/mod_ssl.so
LoadModule socache_shmcb_module modules/mod_socache_shmcb.so
LoadModule setenvif_module modules/mod_setenvif.so
SSLSessionCache "shmcb:logs/ssl_scache(512000)"
"#
    } else {
        ""
    };
    let php_cgi = find_php_cgi_in_store(store).or_else(find_php_cgi);
    let php_block = php_cgi
        .as_ref()
        .and_then(|path| path.parent().map(|dir| (path, dir)))
        .map(|(path, dir)| {
            let php_runtime = write_runtime_php_ini(store, path);
            let php_ini_dir = php_runtime
                .as_ref()
                .map(|path| slash(path))
                .unwrap_or_else(|_| slash(dir));
            let php_tmp = slash(&php_runtime_temp_dir(store));
            format!(
                r#"LoadModule actions_module modules/mod_actions.so
LoadModule alias_module modules/mod_alias.so
LoadModule cgi_module modules/mod_cgi.so
LoadModule env_module modules/mod_env.so
ScriptAlias /localstack-php-cgi/ "{php_dir}/"
Action application/x-httpd-php "/localstack-php-cgi/{php_exe}"
AddHandler application/x-httpd-php .php
SetEnv PHPRC "{php_ini_dir}"
SetEnv TMP "{php_tmp}"
SetEnv TEMP "{php_tmp}"
SetEnv TMPDIR "{php_tmp}"
<Directory "{php_dir}">
    Options None
    AllowOverride None
    Require all granted
</Directory>
"#,
                php_dir = slash(dir),
                php_ini_dir = php_ini_dir,
                php_tmp = php_tmp,
                php_exe = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("php-cgi.exe")
            )
        })
        .unwrap_or_default();
    let default_vhost = apache_default_vhost(store, &document_root);
    let vhosts = apache_vhosts(snapshot);
    let config = runtime.join("httpd.conf");
    let content = format!(
        r#"ServerRoot "{server_root}"
Listen 80
ServerName localhost
Timeout 90
KeepAlive On
MaxKeepAliveRequests 64
KeepAliveTimeout 2
{ssl_block}
LoadModule dir_module modules/mod_dir.so
LoadModule mime_module modules/mod_mime.so
LoadModule authz_core_module modules/mod_authz_core.so
LoadModule authz_host_module modules/mod_authz_host.so
LoadModule log_config_module modules/mod_log_config.so
LoadModule rewrite_module modules/mod_rewrite.so
{proxy_block}
TypesConfig conf/mime.types
{php_block}
DocumentRoot "{document_root}"
<Directory "{document_root}">
    Options Indexes FollowSymLinks
    AllowOverride All
    Require all granted
</Directory>
Alias /localstack-tools/ "{tools_root}/"
<Directory "{tools_root}">
    Options Indexes FollowSymLinks ExecCGI
    AllowOverride All
    Require all granted
</Directory>
DirectoryIndex index.html index.php
ErrorLog "{log_dir}/apache-error.log"
CustomLog "{log_dir}/apache-access.log" common
{default_vhost}
{vhosts}
"#,
        server_root = slash(server_root),
        document_root = slash(&document_root),
        tools_root = slash(&tools_root),
        log_dir = slash(&store.dir.join("logs")),
        ssl_block = ssl_block,
        proxy_block = if node_proxy_enabled {
            "LoadModule proxy_module modules/mod_proxy.so\nLoadModule proxy_http_module modules/mod_proxy_http.so"
        } else {
            ""
        },
        php_block = php_block,
        default_vhost = default_vhost,
        vhosts = vhosts,
    );
    fs::write(&config, content).map_err(|err| format!("Cannot write Apache config: {err}"))?;
    snapshot.services[index].arguments = vec![
        "-d".to_string(),
        server_root.display().to_string(),
        "-f".to_string(),
        config.display().to_string(),
    ];
    snapshot.services[index].config_path = config.display().to_string();
    snapshot.services[index].log_path = store
        .dir
        .join("logs")
        .join("apache-error.log")
        .display()
        .to_string();
    snapshot.services[index].ports = if ssl_enabled { vec![80, 443] } else { vec![80] };
    Ok(())
}

fn prepare_nginx_runtime(
    store: &Store,
    snapshot: &mut crate::state::AppSnapshot,
    index: usize,
) -> AppResult<()> {
    let executable = PathBuf::from(&snapshot.services[index].executable_path);
    let prefix = executable
        .parent()
        .ok_or_else(|| "Cannot resolve Nginx runtime root.".to_string())?;
    let runtime_conf = store.dir.join("configs").join("nginx-runtime");
    fs::create_dir_all(runtime_conf.join("conf"))
        .map_err(|err| format!("Cannot create Nginx config folder: {err}"))?;
    fs::create_dir_all(runtime_conf.join("logs"))
        .map_err(|err| format!("Cannot create Nginx log folder: {err}"))?;
    for dir in [
        "temp/client_body_temp",
        "temp/proxy_temp",
        "temp/fastcgi_temp",
        "temp/uwsgi_temp",
        "temp/scgi_temp",
    ] {
        fs::create_dir_all(runtime_conf.join(dir))
            .map_err(|err| format!("Cannot create Nginx temp folder: {err}"))?;
    }
    ensure_host_runtime_dirs(snapshot)?;
    let document_root = default_document_root(store)?;
    let servers = nginx_servers(snapshot);
    let config = runtime_conf.join("conf").join("nginx.conf");
    let content = format!(
        r#"worker_processes  1;
error_log  logs/error.log;
pid        logs/nginx.pid;
events {{ worker_connections  256; }}
http {{
    include       "{prefix}/conf/mime.types";
    default_type  application/octet-stream;
    access_log    logs/access.log;
    sendfile      on;
    server {{
        listen       8080;
        server_name  localhost;
        root         "{document_root}";
        index        index.html index.php;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
    }}
{servers}
}}
"#,
        prefix = slash(prefix),
        document_root = slash(&document_root),
        servers = servers,
    );
    fs::write(&config, content).map_err(|err| format!("Cannot write Nginx config: {err}"))?;
    snapshot.services[index].arguments = vec![
        "-p".to_string(),
        runtime_conf.display().to_string(),
        "-c".to_string(),
        "conf/nginx.conf".to_string(),
    ];
    snapshot.services[index].config_path = config.display().to_string();
    snapshot.services[index].log_path = runtime_conf
        .join("logs")
        .join("error.log")
        .display()
        .to_string();
    snapshot.services[index].ports = vec![8080];
    Ok(())
}

fn default_document_root(store: &Store) -> AppResult<PathBuf> {
    let root = store.dir.join("www").join("default");
    fs::create_dir_all(&root).map_err(|err| format!("Cannot create default web root: {err}"))?;
    let index = root.join("index.html");
    if !index.exists() {
        fs::write(
            &index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>LocalStack Pro</title></head><body><h1>LocalStack Pro</h1><p>Select a configured host in LocalStack Pro.</p></body></html>",
        )
        .map_err(|err| format!("Cannot write default web page: {err}"))?;
    }
    Ok(root)
}

fn ensure_tools_root(store: &Store) -> AppResult<PathBuf> {
    let root = store.dir.join("tools").join("public");
    fs::create_dir_all(&root).map_err(|err| format!("Cannot create web tools root: {err}"))?;
    fs::create_dir_all(root.join("phpmyadmin"))
        .map_err(|err| format!("Cannot create phpMyAdmin tools folder: {err}"))?;
    let index = root.join("index.html");
    if !index.exists() {
        fs::write(
            &index,
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>LocalStack Pro Tools</title></head><body><h1>LocalStack Pro Tools</h1></body></html>",
        )
        .map_err(|err| format!("Cannot write tools index: {err}"))?;
    }
    let adminer = root.join("adminer.php");
    let phpmyadmin = root.join("phpmyadmin").join("index.php");
    if !adminer.exists() {
        fs::write(&adminer, localstack_tools_php("Adminer"))
            .map_err(|err| format!("Cannot write Adminer placeholder: {err}"))?;
    }
    if !phpmyadmin.exists() {
        fs::write(&phpmyadmin, localstack_tools_php("phpMyAdmin"))
            .map_err(|err| format!("Cannot write phpMyAdmin placeholder: {err}"))?;
    }
    Ok(root)
}

fn localstack_tools_php(name: &str) -> String {
    format!(
        r#"<?php
header('Content-Type: text/html; charset=utf-8');
?><!doctype html><html><head><meta charset="utf-8"><title>LocalStack Pro {name}</title></head><body><h1>LocalStack Pro {name}</h1><p>The database tool route is ready. Use the LocalStack Pro button to install or open the full tool.</p></body></html>
"#
    )
}

fn apache_default_vhost(store: &Store, document_root: &Path) -> String {
    format!(
        r#"
<VirtualHost *:80>
    ServerName localhost
    ServerAlias 127.0.0.1
    DocumentRoot "{document_root}"
    DirectoryIndex index.html index.php
    <Directory "{document_root}">
        Options Indexes FollowSymLinks
        AllowOverride All
        Require all granted
    </Directory>
    ErrorLog "{log_dir}/apache-default-error.log"
    CustomLog "{log_dir}/apache-default-access.log" common
</VirtualHost>
"#,
        document_root = slash(document_root),
        log_dir = slash(&store.dir.join("logs"))
    )
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

fn ensure_host_runtime_dirs(snapshot: &crate::state::AppSnapshot) -> AppResult<()> {
    for host in &snapshot.hosts {
        fs::create_dir_all(host_document_root(host))
            .map_err(|err| format!("Cannot create document root for {}: {err}", host.domain))?;
        fs::create_dir_all(PathBuf::from(&host.root_folder).join("logs"))
            .map_err(|err| format!("Cannot create logs folder for {}: {err}", host.domain))?;
    }
    Ok(())
}

fn apache_vhosts(snapshot: &crate::state::AppSnapshot) -> String {
    snapshot
        .hosts
        .iter()
        .filter(|host| host.web_server.eq_ignore_ascii_case("apache"))
        .map(|host| {
            if is_node_host(host) {
                return apache_node_vhost(host);
            }
            let document_root = host_document_root(host);
            let ssl_vhost = if host.ssl {
                match ensure_apache_host_certificate(host) {
                    Ok((cert_path, key_path)) => format!(
                        r#"
<VirtualHost *:443>
    ServerName {domain}
    DocumentRoot "{document_root}"
    DirectoryIndex index.php index.html
    SSLEngine on
    SSLCertificateFile "{cert_path}"
    SSLCertificateKeyFile "{key_path}"
    <Directory "{document_root}">
        Options Indexes FollowSymLinks ExecCGI
        AllowOverride All
        Require all granted
    </Directory>
    ErrorLog "{logs}/{domain}-ssl-error.log"
    CustomLog "{logs}/{domain}-ssl-access.log" common
</VirtualHost>
"#,
                        domain = host.domain,
                        document_root = slash(&document_root),
                        cert_path = slash(&cert_path),
                        key_path = slash(&key_path),
                        logs = slash(&PathBuf::from(&host.root_folder).join("logs"))
                    ),
                    Err(_) => String::new(),
                }
            } else {
                String::new()
            };
            format!(
                r#"
<VirtualHost *:80>
    ServerName {domain}
    DocumentRoot "{document_root}"
    DirectoryIndex index.php index.html
    <Directory "{document_root}">
        Options Indexes FollowSymLinks ExecCGI
        AllowOverride All
        Require all granted
    </Directory>
    ErrorLog "{logs}/{domain}-error.log"
    CustomLog "{logs}/{domain}-access.log" common
</VirtualHost>
{ssl_vhost}
"#,
                domain = host.domain,
                document_root = slash(&document_root),
                logs = slash(&PathBuf::from(&host.root_folder).join("logs")),
                ssl_vhost = ssl_vhost
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_node_host(host: &crate::state::HostInfo) -> bool {
    host.tags.iter().any(|tag| {
        matches!(
            tag.as_str(),
            "node" | "nextjs" | "node-express" | "vite-react"
        )
    }) || host.env_variables.contains_key("LOCALSTACK_NODE_PORT")
}

fn apache_node_vhost(host: &crate::state::HostInfo) -> String {
    let logs = slash(&PathBuf::from(&host.root_folder).join("logs"));
    let port = node_host_port(host);
    format!(
        r#"
<VirtualHost *:80>
    ServerName {domain}
    ProxyPreserveHost On
    ProxyPass / http://127.0.0.1:{port}/
    ProxyPassReverse / http://127.0.0.1:{port}/
    ErrorLog "{logs}/{domain}-error.log"
    CustomLog "{logs}/{domain}-access.log" common
</VirtualHost>
"#,
        domain = host.domain,
        port = port,
        logs = logs
    )
}

fn node_host_port(host: &crate::state::HostInfo) -> u16 {
    host.env_variables
        .get("LOCALSTACK_NODE_PORT")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000)
}

fn ensure_apache_host_certificate(host: &crate::state::HostInfo) -> AppResult<(PathBuf, PathBuf)> {
    let store = Store::new()?;
    crate::ssl::ensure_host_certificate_files(&store, &host.domain, vec![host.domain.clone()])
}

fn nginx_servers(snapshot: &crate::state::AppSnapshot) -> String {
    snapshot
        .hosts
        .iter()
        .filter(|host| host.web_server.eq_ignore_ascii_case("nginx"))
        .map(|host| {
            let document_root = host_document_root(host);
            format!(
                r#"
    server {{
        listen       8080;
        server_name  {domain};
        root         "{document_root}";
        index        index.php index.html;
        location / {{
            try_files $uri $uri/ /index.php?$query_string;
        }}
    }}
"#,
                domain = host.domain,
                document_root = slash(&document_root)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn find_php_cgi() -> Option<PathBuf> {
    let candidates = [
        "C:\\Program Files\\PHP\\current\\php-cgi.exe",
        "C:\\Program Files\\PHP\\8.4\\php-cgi.exe",
        "C:\\Program Files\\PHP\\8.3\\php-cgi.exe",
        "C:\\Program Files\\PHP\\8.3.30\\nts\\x64\\php-cgi.exe",
        "C:\\tools\\php\\php-cgi.exe",
    ];
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .or_else(|| find_on_path("php-cgi.exe"))
}

fn find_php_cgi_in_store(store: &Store) -> Option<PathBuf> {
    ["8.5", "8.4", "8.3", "8.2", "8.1"]
        .iter()
        .map(|version| {
            store
                .dir
                .join("services")
                .join("php")
                .join(version)
                .join("php-cgi.exe")
        })
        .find(|path| path.exists())
}

fn write_runtime_php_ini(store: &Store, php_cgi: &Path) -> AppResult<PathBuf> {
    let php_dir = php_cgi
        .parent()
        .ok_or_else(|| "Cannot resolve PHP folder.".to_string())?;
    let ext_dir = php_dir.join("ext");
    let runtime = store.dir.join("configs").join("php-runtime");
    let temp = php_runtime_temp_dir(store);
    let upload_temp = store.dir.join("temp").join("php-upload");
    let session_temp = store.dir.join("temp").join("php-sessions");
    fs::create_dir_all(&runtime).map_err(|err| format!("Cannot create PHP runtime: {err}"))?;
    for path in [&temp, &upload_temp, &session_temp] {
        fs::create_dir_all(path)
            .map_err(|err| format!("Cannot create PHP temp folder {}: {err}", path.display()))?;
    }
    let extensions = [
        "mysqli",
        "pdo_mysql",
        "openssl",
        "curl",
        "mbstring",
        "fileinfo",
        "gd",
        "zip",
        "intl",
    ]
    .iter()
    .filter(|name| ext_dir.join(format!("php_{name}.dll")).exists())
    .map(|name| format!("extension={name}"))
    .collect::<Vec<_>>()
    .join("\n");
    let ini = format!(
        r#"extension_dir="{ext_dir}"
cgi.force_redirect=0
cgi.fix_pathinfo=1
memory_limit=512M
upload_max_filesize=64M
post_max_size=64M
max_execution_time=120
display_errors=On
log_errors=On
date.timezone=UTC
sys_temp_dir="{temp}"
upload_tmp_dir="{upload_temp}"
session.save_handler=files
session.save_path="{session_temp}"
session.use_cookies=1
session.use_only_cookies=1
session.cookie_httponly=1
session.gc_probability=1
session.gc_divisor=1000
{extensions}
"#,
        ext_dir = slash(&ext_dir),
        temp = slash(&temp),
        upload_temp = slash(&upload_temp),
        session_temp = slash(&session_temp),
        extensions = extensions
    );
    fs::write(runtime.join("php.ini"), ini)
        .map_err(|err| format!("Cannot write PHP runtime config: {err}"))?;
    Ok(runtime)
}

fn php_runtime_temp_dir(store: &Store) -> PathBuf {
    store.dir.join("temp").join("php")
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(name))
            .find(|path| path.exists())
    })
}

fn slash(path: &Path) -> String {
    path.display()
        .to_string()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

fn node_proxy_script() -> &'static str {
    r#"const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const appData = process.env.APPDATA || '';
const statePath = path.join(appData, 'LocalStack', 'LocalStack Pro', 'data', 'state.json');
const processes = new Map();

function loadApps() {
  try {
    const state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
    return (state.hosts || [])
      .filter((host) => host.envVariables && host.envVariables.LOCALSTACK_NODE_PORT)
      .map((host) => ({
        domain: String(host.domain || '').toLowerCase(),
        root: host.rootFolder,
        port: Number(host.envVariables.LOCALSTACK_NODE_PORT),
        script: host.envVariables.LOCALSTACK_NODE_SCRIPT || 'dev'
      }));
  } catch {
    return [];
  }
}

function appForHost(hostHeader) {
  const host = String(hostHeader || '').split(':')[0].toLowerCase();
  return loadApps().find((app) => app.domain === host);
}

function ensureApp(app) {
  if (!app || processes.has(app.domain)) return;
  const npm = process.platform === 'win32' ? 'npm.cmd' : 'npm';
  const child = spawn(npm, ['run', app.script, '--', '--port', String(app.port)], {
    cwd: app.root,
    env: { ...process.env, PORT: String(app.port), LOCALSTACK_NODE_PORT: String(app.port), BROWSER: 'none' },
    shell: process.platform === 'win32',
    windowsHide: true,
    stdio: 'ignore'
  });
  processes.set(app.domain, child);
  child.on('error', () => processes.delete(app.domain));
  child.on('exit', () => processes.delete(app.domain));
}

function startConfiguredApps() {
  loadApps().forEach(ensureApp);
}

function proxyBuffered(req, res, targetPort, app, body, startedAt) {
  const headers = { ...req.headers, host: req.headers.host || 'localhost' };
  if (body.length > 0) headers['content-length'] = String(body.length);
  const proxy = http.request({
    hostname: '127.0.0.1',
    port: targetPort,
    path: req.url,
    method: req.method,
    headers
  }, (upstream) => {
    res.writeHead(upstream.statusCode || 502, upstream.headers);
    upstream.pipe(res);
  });
  proxy.on('error', (err) => {
    if (Date.now() - startedAt < 45000 && !res.writableEnded) {
      setTimeout(() => proxyBuffered(req, res, targetPort, app, body, startedAt), 500);
      return;
    }
    if (!res.headersSent) {
      res.writeHead(503, { 'content-type': 'text/plain; charset=utf-8' });
    }
    res.end(`LocalStack Pro could not reach ${app ? app.domain : 'the app'} on 127.0.0.1:${targetPort}\n${err.message}`);
  });
  if (body.length > 0) proxy.write(body);
  proxy.end();
}

const server = http.createServer((req, res) => {
  const app = appForHost(req.headers.host);
  const targetPort = app ? app.port : Number(process.env.LOCALSTACK_PROXY_TARGET || 3000);
  if (app) ensureApp(app);
  const chunks = [];
  req.on('data', (chunk) => chunks.push(chunk));
  req.on('end', () => proxyBuffered(req, res, targetPort, app, Buffer.concat(chunks), Date.now()));
  req.on('error', (err) => {
    if (!res.headersSent) res.writeHead(500, { 'content-type': 'text/plain; charset=utf-8' });
    res.end(err.message);
  });
});
startConfiguredApps();
setInterval(startConfiguredApps, 10000);
server.listen(3000, '127.0.0.1');
"#
}

pub fn try_run_service_helper() -> bool {
    if std::env::args().any(|arg| arg == "--localstack-dns-helper") {
        run_dns_helper();
        return true;
    }
    false
}

fn run_dns_helper() {
    let port = std::env::var("LOCALSTACK_DNS_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(5353);
    let Ok(socket) = UdpSocket::bind(("127.0.0.1", port)) else {
        return;
    };
    let mut buffer = [0_u8; 512];
    loop {
        let Ok((len, peer)) = socket.recv_from(&mut buffer) else {
            continue;
        };
        if let Some(response) = dns_response(&buffer[..len]) {
            let _ = socket.send_to(&response, peer);
        }
    }
}

fn dns_response(query: &[u8]) -> Option<Vec<u8>> {
    if query.len() < 17 {
        return None;
    }
    let mut question_end = 12;
    while question_end < query.len() && query[question_end] != 0 {
        question_end += query[question_end] as usize + 1;
    }
    if question_end + 5 > query.len() {
        return None;
    }
    let question_len = question_end + 5 - 12;
    let mut response = Vec::with_capacity(12 + question_len + 16);
    response.extend_from_slice(&query[0..2]);
    response.extend_from_slice(&[0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
    response.extend_from_slice(&query[12..12 + question_len]);
    response.extend_from_slice(&[
        0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x04, 127, 0, 0, 1,
    ]);
    Some(response)
}
