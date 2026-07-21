mod cms;
mod database;
mod dependencies;
mod health;
mod hosts;
mod logs;
mod php;
mod services;
mod settings;
mod ssl;
mod state;
mod tools;
mod tray;

use state::{
    AppResult, AppSettings, AppSnapshot, CertificateInfo, DatabaseInfo, HostInfo, PhpVersion,
    ServiceInfo, Store,
};
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::{Manager, WindowEvent};

static STATE_CACHE: OnceLock<Mutex<Option<(Instant, AppSnapshot)>>> = OnceLock::new();

fn cache_snapshot(snapshot: AppSnapshot) -> AppSnapshot {
    let cache = STATE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), snapshot.clone()));
    }
    snapshot
}

fn with_snapshot_cache(result: AppResult<AppSnapshot>) -> AppResult<AppSnapshot> {
    result.map(|snapshot| {
        invalidate_state_cache();
        cache_snapshot(snapshot)
    })
}

fn invalidate_state_cache() {
    let cache = STATE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = cache.lock() {
        *guard = None;
    }
}

fn with_cache_invalidation<T>(result: AppResult<T>) -> AppResult<T> {
    if result.is_ok() {
        invalidate_state_cache();
    }
    result
}

fn handle_close_to_tray(app: &tauri::AppHandle, window_label: &str) -> AppResult<bool> {
    let close_to_tray = Store::new()
        .and_then(|store| store.load_static())
        .map(|snapshot| snapshot.settings.close_to_tray)
        .unwrap_or(false);
    if close_to_tray {
        let window = app
            .get_webview_window(window_label)
            .ok_or_else(|| format!("Window {window_label} was not found."))?;
        window
            .hide()
            .map_err(|err| format!("Cannot hide window to tray: {err}"))?;
        Ok(true)
    } else {
        app.exit(0);
        Ok(false)
    }
}

pub fn try_run_service_helper() -> bool {
    services::try_run_service_helper()
}

pub fn start_all_for_cli() -> AppResult<String> {
    let snapshot = services::start_all()?;
    let running = snapshot
        .services
        .iter()
        .filter(|service| service.status == state::ServiceStatus::Running)
        .count();
    Ok(format!("running={running}/{}", snapshot.services.len()))
}

pub fn stop_all_for_cli() -> AppResult<String> {
    let snapshot = services::stop_all()?;
    let stopped = snapshot
        .services
        .iter()
        .filter(|service| service.status == state::ServiceStatus::Stopped)
        .count();
    Ok(format!("stopped={stopped}/{}", snapshot.services.len()))
}

pub fn start_service_for_cli(service_id: String) -> AppResult<String> {
    let snapshot = services::start_service(service_id.clone())?;
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .ok_or_else(|| format!("Service {service_id} was not found after start."))?;
    Ok(format!("{}={:?}", service.id, service.status))
}

pub fn restart_service_for_cli(service_id: String) -> AppResult<String> {
    let snapshot = services::restart_service(service_id.clone())?;
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .ok_or_else(|| format!("Service {service_id} was not found after restart."))?;
    Ok(format!("{}={:?}", service.id, service.status))
}

pub fn check_service_for_cli(service_id: String) -> AppResult<String> {
    let snapshot = Store::new()?.load_static()?;
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .ok_or_else(|| format!("Service {service_id} was not found."))?;
    Ok(format!(
        "{}={:?} pid={} ports={}",
        service.id,
        service.status,
        service
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string()),
        service
            .ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    ))
}

pub fn diagnose_host_for_cli(host_id: String) -> AppResult<String> {
    let report = hosts::diagnose_host(host_id)?;
    Ok(format!(
        "{} ok={} warnings={} errors={} summary={}",
        report.domain, report.ok, report.warnings, report.errors, report.summary
    ))
}

pub fn repair_host_for_cli(host_id: String) -> AppResult<String> {
    let report = hosts::repair_host(host_id)?;
    Ok(format!(
        "{} ok={} warnings={} errors={} summary={}",
        report.domain, report.ok, report.warnings, report.errors, report.summary
    ))
}

pub fn install_db_tool_for_cli(kind: String) -> AppResult<String> {
    settings::install_database_admin_tool(kind)
}

pub fn install_service_dependency_for_cli(service_id: String) -> AppResult<String> {
    let snapshot = dependencies::install_service_dependency(service_id.clone())?;
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .ok_or_else(|| format!("Service {service_id} was not found after install."))?;
    Ok(format!(
        "{} executable={}",
        service.id, service.executable_path
    ))
}

pub fn test_database_for_cli(database_id: String) -> AppResult<String> {
    let report = database::test_database_connection(database_id)?;
    Ok(format!(
        "{} ok={} warnings={} errors={} summary={}",
        report.database, report.ok, report.warnings, report.errors, report.summary
    ))
}

pub fn create_backup_for_cli(path: String) -> AppResult<String> {
    let target = settings::create_app_backup(path)?;
    Ok(format!("backup={target}"))
}

pub fn tail_log_for_cli(source: String) -> AppResult<String> {
    logs::tail_log_for_cli(source)
}

pub fn install_cms_for_cli(
    template_id: String,
    domain: String,
    root_folder: String,
) -> AppResult<String> {
    let snapshot = Store::new()?.load_static()?;
    let php_version = snapshot
        .php_versions
        .iter()
        .find(|php| php.default)
        .or_else(|| snapshot.php_versions.first())
        .map(|php| php.version.clone())
        .unwrap_or_else(|| "8.3".to_string());
    let request = cms::CmsInstallRequest {
        template_id,
        domain: domain.clone(),
        root_folder,
        php_version,
        web_server: "Apache".to_string(),
        ssl: false,
        database_engine: "MySQL".to_string(),
        create_database: true,
        database_name: None,
        database_user: None,
        database_password: None,
        overwrite: false,
    };
    let snapshot = cms::install_cms(request)?;
    let installed = snapshot
        .hosts
        .iter()
        .any(|host| host.domain.eq_ignore_ascii_case(&domain));
    Ok(format!("cms_host_installed={installed} domain={domain}"))
}

pub fn detect_dependencies_for_cli() -> AppResult<String> {
    let snapshot = dependencies::detect_dependencies()?;
    let detected = snapshot
        .services
        .iter()
        .filter(|service| std::path::Path::new(&service.executable_path).exists())
        .count();
    Ok(format!("detected={detected}/{}", snapshot.services.len()))
}

pub fn health_check_for_cli() -> AppResult<String> {
    let report = health::run_health_check()?;
    let issues = report
        .checks
        .iter()
        .filter(|check| check.severity != "ok")
        .map(|check| format!("{}: {}", check.title, check.message))
        .collect::<Vec<_>>()
        .join(" | ");
    Ok(format!(
        "summary={} ok={} warnings={} errors={}{}",
        report.summary,
        report.ok,
        report.warnings,
        report.errors,
        if issues.is_empty() {
            String::new()
        } else {
            format!(" issues={issues}")
        }
    ))
}

pub fn repair_environment_for_cli() -> AppResult<String> {
    let report = health::repair_environment()?;
    Ok(format!(
        "summary={} ok={} warnings={} errors={}",
        report.summary, report.ok, report.warnings, report.errors
    ))
}

#[tauri::command]
fn get_app_state() -> AppResult<AppSnapshot> {
    current_state()
}

pub fn current_state() -> AppResult<AppSnapshot> {
    let cache = STATE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(guard) = cache.lock() {
        if let Some((created_at, snapshot)) = guard.as_ref() {
            if created_at.elapsed() < Duration::from_secs(5) {
                return Ok(snapshot.clone());
            }
        }
    }
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let snapshot = store.refresh_runtime(snapshot);
    if let Ok(mut guard) = cache.lock() {
        *guard = Some((Instant::now(), snapshot.clone()));
    }
    Ok(snapshot)
}

#[tauri::command]
fn start_all() -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::start_all())
}
#[tauri::command]
fn stop_all() -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::stop_all())
}
#[tauri::command]
fn restart_all() -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::restart_all())
}
#[tauri::command]
fn start_service(service_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::start_service(service_id))
}
#[tauri::command]
fn start_service_profile(service_ids: Vec<String>) -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::start_service_profile(service_ids))
}
#[tauri::command]
fn stop_service(service_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::stop_service(service_id))
}
#[tauri::command]
fn restart_service(service_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::restart_service(service_id))
}
#[tauri::command]
fn save_service(service: ServiceInfo) -> AppResult<AppSnapshot> {
    with_snapshot_cache(services::save_service(service))
}
#[tauri::command]
fn install_service_dependency(service_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(dependencies::install_service_dependency(service_id))
}
#[tauri::command]
fn install_all_missing_dependencies() -> AppResult<AppSnapshot> {
    with_snapshot_cache(dependencies::install_all_missing_dependencies())
}
#[tauri::command]
fn detect_dependencies() -> AppResult<AppSnapshot> {
    with_snapshot_cache(dependencies::detect_dependencies())
}
#[tauri::command]
fn run_health_check() -> AppResult<health::HealthReport> {
    health::run_health_check()
}
#[tauri::command]
fn repair_environment() -> AppResult<health::HealthReport> {
    with_cache_invalidation(health::repair_environment())
}

#[tauri::command]
fn save_host(host: HostInfo) -> AppResult<AppSnapshot> {
    with_snapshot_cache(hosts::save_host(host))
}
#[tauri::command]
fn delete_host(host_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(hosts::delete_host(host_id))
}
#[tauri::command]
fn duplicate_host(host_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(hosts::duplicate_host(host_id))
}
#[tauri::command]
fn import_hosts(path: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(hosts::import_hosts(path))
}
#[tauri::command]
fn export_hosts(path: String) -> AppResult<String> {
    hosts::export_hosts(path)
}
#[tauri::command]
fn sync_hosts_file() -> AppResult<AppSnapshot> {
    with_snapshot_cache(hosts::sync_hosts_file())
}
#[tauri::command]
fn diagnose_host(host_id: String) -> AppResult<hosts::HostDiagnosticReport> {
    hosts::diagnose_host(host_id)
}
#[tauri::command]
fn repair_host(host_id: String) -> AppResult<hosts::HostDiagnosticReport> {
    with_cache_invalidation(hosts::repair_host(host_id))
}

#[tauri::command]
fn save_php_version(php: PhpVersion) -> AppResult<AppSnapshot> {
    with_snapshot_cache(php::save_php_version(php))
}
#[tauri::command]
fn install_php_version(version: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(php::install_php_version(version))
}
#[tauri::command]
fn remove_php_version(version: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(php::remove_php_version(version))
}
#[tauri::command]
fn set_default_php(version: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(php::set_default_php(version))
}

#[tauri::command]
fn create_database(database: DatabaseInfo) -> AppResult<AppSnapshot> {
    with_snapshot_cache(database::create_database(database))
}
#[tauri::command]
fn delete_database(database_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(database::delete_database(database_id))
}
#[tauri::command]
fn backup_database(database_id: String) -> AppResult<String> {
    with_cache_invalidation(database::backup_database(database_id))
}
#[tauri::command]
fn import_database_sql(database_id: String, path: String) -> AppResult<String> {
    with_cache_invalidation(database::import_database_sql(database_id, path))
}
#[tauri::command]
fn test_database_connection(database_id: String) -> AppResult<database::DatabaseDiagnosticReport> {
    database::test_database_connection(database_id)
}

#[tauri::command]
fn get_cms_templates() -> AppResult<Vec<cms::CmsTemplate>> {
    Ok(cms::cms_templates())
}
#[tauri::command]
fn install_cms(request: cms::CmsInstallRequest) -> AppResult<AppSnapshot> {
    with_snapshot_cache(cms::install_cms(request))
}

#[tauri::command]
fn generate_certificate(domain: String, san_domains: Vec<String>) -> AppResult<AppSnapshot> {
    with_snapshot_cache(ssl::generate_certificate(domain, san_domains))
}
#[tauri::command]
fn trust_certificate(certificate_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(ssl::trust_certificate(certificate_id))
}
#[tauri::command]
fn revoke_certificate(certificate_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(ssl::revoke_certificate(certificate_id))
}
#[tauri::command]
fn save_certificate(certificate: CertificateInfo) -> AppResult<AppSnapshot> {
    with_snapshot_cache(ssl::save_certificate(certificate))
}
#[tauri::command]
fn export_certificate(certificate_id: String, folder: String) -> AppResult<String> {
    ssl::export_certificate(certificate_id, folder)
}

#[tauri::command]
fn clear_logs() -> AppResult<AppSnapshot> {
    with_snapshot_cache(logs::clear_logs())
}
#[tauri::command]
fn export_logs(path: String) -> AppResult<String> {
    logs::export_logs(path)
}
#[tauri::command]
fn tail_log_file(source: String, lines: Option<u32>) -> AppResult<logs::LogFileTail> {
    logs::tail_log_file(source, lines)
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> AppResult<AppSnapshot> {
    with_snapshot_cache(settings::save_settings(settings))
}
#[tauri::command]
fn export_settings(path: String) -> AppResult<String> {
    settings::export_settings(path)
}
#[tauri::command]
fn import_settings(path: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(settings::import_settings(path))
}
#[tauri::command]
fn reset_settings() -> AppResult<AppSnapshot> {
    with_snapshot_cache(settings::reset_settings())
}
#[tauri::command]
fn create_app_backup(path: String) -> AppResult<String> {
    with_cache_invalidation(settings::create_app_backup(path))
}
#[tauri::command]
fn restore_app_backup(path: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(settings::restore_app_backup(path))
}
#[tauri::command]
fn open_certificate_store() -> AppResult<()> {
    settings::open_certificate_store()
}
#[tauri::command]
fn open_documentation() -> AppResult<()> {
    settings::open_documentation()
}
#[tauri::command]
fn open_path(path: String) -> AppResult<()> {
    settings::open_path(path)
}
#[tauri::command]
fn open_url(url: String) -> AppResult<()> {
    settings::open_url(url)
}
#[tauri::command]
fn open_terminal(path: String) -> AppResult<()> {
    settings::open_terminal(path)
}
#[tauri::command]
fn open_host(host_id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(settings::open_host(host_id))
}
#[tauri::command]
fn open_database_admin(kind: String) -> AppResult<()> {
    settings::open_database_admin(kind)
}
#[tauri::command]
fn scan_ports() -> AppResult<Vec<tools::PortInspection>> {
    tools::scan_ports()
}
#[tauri::command]
fn run_project_command(path: String, command_key: String) -> AppResult<String> {
    tools::run_project_command(path, command_key)
}
#[tauri::command]
fn clone_project_repository(url: String, folder: String) -> AppResult<String> {
    tools::clone_project_repository(url, folder)
}
#[tauri::command]
fn inspect_project(path: String) -> AppResult<tools::ProjectInspection> {
    tools::inspect_project(path)
}
#[tauri::command]
fn generate_env_template(
    path: String,
    kind: String,
    database: String,
    user: String,
    password: String,
    domain: String,
) -> AppResult<String> {
    tools::generate_env_template(path, kind, database, user, password, domain)
}
#[tauri::command]
fn preview_host(host_id: String) -> AppResult<tools::SitePreview> {
    tools::preview_host(host_id)
}
#[tauri::command]
fn export_portable_host(host_id: String, target: String) -> AppResult<String> {
    tools::export_portable_host(host_id, target)
}
#[tauri::command]
fn backup_host(host_id: String, target: String) -> AppResult<String> {
    tools::backup_host(host_id, target)
}
#[tauri::command]
fn restore_host_backup(path: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(tools::restore_host_backup(path))
}
#[tauri::command]
fn check_latest_release() -> AppResult<tools::ReleaseInfo> {
    tools::check_latest_release()
}
#[tauri::command]
fn download_latest_release_installer() -> AppResult<String> {
    tools::download_latest_release_installer()
}
#[tauri::command]
fn install_downloaded_update(path: String) -> AppResult<String> {
    with_cache_invalidation(tools::install_downloaded_update(path))
}
#[tauri::command]
fn read_config_file(path: String) -> AppResult<tools::ConfigFile> {
    tools::read_config_file(path)
}
#[tauri::command]
fn save_config_file(path: String, content: String) -> AppResult<String> {
    with_cache_invalidation(tools::save_config_file(path, content))
}
#[tauri::command]
fn create_diagnostic_bundle(target: String) -> AppResult<String> {
    tools::create_diagnostic_bundle(target)
}
#[tauri::command]
fn diagnose_ssl(domain: String) -> AppResult<tools::SslDiagnostic> {
    tools::diagnose_ssl(domain)
}
#[tauri::command]
fn inspect_installed_tools() -> AppResult<Vec<tools::InstalledTool>> {
    tools::inspect_installed_tools()
}
#[tauri::command]
fn list_files(path: String) -> AppResult<Vec<tools::FileEntry>> {
    tools::list_files(path)
}
#[tauri::command]
fn read_file(path: String) -> AppResult<tools::ConfigFile> {
    tools::read_file(path)
}
#[tauri::command]
fn read_file_with_encoding(path: String, encoding: String) -> AppResult<tools::ConfigFile> {
    tools::read_file_with_encoding(path, encoding)
}
#[tauri::command]
fn write_file(path: String, content: String) -> AppResult<String> {
    with_cache_invalidation(tools::write_file(path, content))
}
#[tauri::command]
fn write_file_with_encoding(path: String, content: String, encoding: String) -> AppResult<String> {
    with_cache_invalidation(tools::write_file_with_encoding(path, content, encoding))
}
#[tauri::command]
fn create_file(path: String) -> AppResult<String> {
    with_cache_invalidation(tools::create_file(path))
}
#[tauri::command]
fn create_folder(path: String) -> AppResult<String> {
    with_cache_invalidation(tools::create_folder(path))
}
#[tauri::command]
fn delete_path(path: String) -> AppResult<String> {
    with_cache_invalidation(tools::delete_path(path))
}
#[tauri::command]
fn trash_path(path: String) -> AppResult<tools::TrashRecord> {
    with_cache_invalidation(tools::trash_path(path))
}
#[tauri::command]
fn restore_trash_path(
    original_path: String,
    trash_path: String,
    overwrite: bool,
) -> AppResult<String> {
    with_cache_invalidation(tools::restore_trash_path(
        original_path,
        trash_path,
        overwrite,
    ))
}
#[tauri::command]
fn rename_path(path: String, new_name: String) -> AppResult<String> {
    with_cache_invalidation(tools::rename_path(path, new_name))
}
#[tauri::command]
fn duplicate_path(path: String) -> AppResult<String> {
    with_cache_invalidation(tools::duplicate_path(path))
}
#[tauri::command]
fn copy_path(source: String, target: String, overwrite: bool) -> AppResult<String> {
    with_cache_invalidation(tools::copy_path(source, target, overwrite))
}
#[tauri::command]
fn move_path(source: String, target: String, overwrite: bool) -> AppResult<String> {
    with_cache_invalidation(tools::move_path(source, target, overwrite))
}
#[tauri::command]
fn chmod_path(path: String, mode: String, read_only: bool) -> AppResult<String> {
    with_cache_invalidation(tools::chmod_path(path, mode, read_only))
}
#[tauri::command]
fn upload_files(
    sources: Vec<String>,
    destination: String,
    overwrite: bool,
) -> AppResult<Vec<String>> {
    with_cache_invalidation(tools::upload_files(sources, destination, overwrite))
}
#[tauri::command]
fn extract_archive_to(path: String, destination: String) -> AppResult<String> {
    with_cache_invalidation(tools::extract_archive_to(path, destination))
}
#[tauri::command]
fn create_archive(paths: Vec<String>, target: String) -> AppResult<String> {
    with_cache_invalidation(tools::create_archive(paths, target))
}
#[tauri::command]
fn search_file_contents(
    root: String,
    query: String,
    regexp: bool,
    case_sensitive: bool,
) -> AppResult<Vec<tools::FileSearchResult>> {
    tools::search_file_contents(root, query, regexp, case_sensitive)
}
#[tauri::command]
fn search_file_contents_advanced(
    root: String,
    query: String,
    regexp: bool,
    case_sensitive: bool,
    include_extensions: String,
    exclude_folders: String,
    limit: usize,
) -> AppResult<Vec<tools::FileSearchResult>> {
    tools::search_file_contents_advanced(
        root,
        query,
        regexp,
        case_sensitive,
        include_extensions,
        exclude_folders,
        limit,
    )
}
#[tauri::command]
fn list_archive_entries(path: String) -> AppResult<Vec<tools::ArchiveEntry>> {
    tools::list_archive_entries(path)
}
#[tauri::command]
fn apply_windows_acl(
    path: String,
    identity: String,
    rights: String,
    inherit: bool,
) -> AppResult<String> {
    tools::apply_windows_acl(path, identity, rights, inherit)
}
#[tauri::command]
fn list_environment_snapshots() -> AppResult<Vec<tools::EnvironmentSnapshotInfo>> {
    tools::list_environment_snapshots()
}
#[tauri::command]
fn create_environment_snapshot(name: String) -> AppResult<tools::EnvironmentSnapshotInfo> {
    tools::create_environment_snapshot(name)
}
#[tauri::command]
fn restore_environment_snapshot(id: String) -> AppResult<AppSnapshot> {
    with_snapshot_cache(tools::restore_environment_snapshot(id))
}
#[tauri::command]
fn list_node_scripts(path: String) -> AppResult<Vec<tools::NodeScript>> {
    tools::list_node_scripts(path)
}
#[tauri::command]
fn run_node_script(path: String, script: String) -> AppResult<String> {
    tools::run_node_script(path, script)
}
#[tauri::command]
fn resource_monitor() -> AppResult<Vec<tools::ResourceProcess>> {
    tools::resource_monitor()
}
#[tauri::command]
fn kill_process(pid: u32) -> AppResult<String> {
    tools::kill_process(pid)
}
#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn hide_tray_panel(app: tauri::AppHandle) {
    tray::hide_tray_panel(&app);
}

#[tauri::command]
fn open_main_page(app: tauri::AppHandle, page: Option<String>) {
    tray::open_main_page(&app, page);
}

#[tauri::command]
fn minimize_window(app: tauri::AppHandle) -> AppResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window was not found.".to_string())?;
    let minimize_to_tray = Store::new()
        .and_then(|store| store.load_static())
        .map(|snapshot| snapshot.settings.minimize_to_tray)
        .unwrap_or(false);
    if minimize_to_tray {
        return window
            .hide()
            .map_err(|err| format!("Cannot hide window to tray: {err}"));
    }
    window
        .minimize()
        .map_err(|err| format!("Cannot minimize window: {err}"))
}

#[tauri::command]
fn toggle_window_maximize(app: tauri::AppHandle) -> AppResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window was not found.".to_string())?;
    let maximized = window
        .is_maximized()
        .map_err(|err| format!("Cannot read window maximize state: {err}"))?;
    if maximized {
        window
            .unmaximize()
            .map_err(|err| format!("Cannot restore window: {err}"))
    } else {
        window
            .maximize()
            .map_err(|err| format!("Cannot maximize window: {err}"))
    }
}

#[tauri::command]
fn request_window_close(app: tauri::AppHandle) -> AppResult<()> {
    handle_close_to_tray(&app, "main").map(|_| ())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let store = Store::new().map_err(|err| tauri::Error::Anyhow(anyhow::anyhow!(err)))?;
            let _ = store
                .load()
                .map_err(|err| tauri::Error::Anyhow(anyhow::anyhow!(err)))?;
            std::thread::spawn(|| {
                let _ = state::bootstrap_bundled_services();
            });
            tray::setup(app.handle())?;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.maximize();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "tray-panel" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Ok(true) = handle_close_to_tray(window.app_handle(), window.label()) {
                    api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_app_state,
            start_all,
            stop_all,
            restart_all,
            start_service,
            start_service_profile,
            stop_service,
            restart_service,
            save_service,
            install_service_dependency,
            install_all_missing_dependencies,
            detect_dependencies,
            run_health_check,
            repair_environment,
            save_host,
            delete_host,
            duplicate_host,
            import_hosts,
            export_hosts,
            sync_hosts_file,
            diagnose_host,
            repair_host,
            save_php_version,
            install_php_version,
            remove_php_version,
            set_default_php,
            create_database,
            delete_database,
            backup_database,
            import_database_sql,
            test_database_connection,
            get_cms_templates,
            install_cms,
            generate_certificate,
            trust_certificate,
            revoke_certificate,
            save_certificate,
            export_certificate,
            clear_logs,
            export_logs,
            tail_log_file,
            save_settings,
            export_settings,
            import_settings,
            reset_settings,
            create_app_backup,
            restore_app_backup,
            open_certificate_store,
            open_documentation,
            open_path,
            open_url,
            open_terminal,
            open_host,
            open_database_admin,
            scan_ports,
            run_project_command,
            clone_project_repository,
            inspect_project,
            generate_env_template,
            preview_host,
            export_portable_host,
            backup_host,
            restore_host_backup,
            check_latest_release,
            download_latest_release_installer,
            install_downloaded_update,
            read_config_file,
            save_config_file,
            create_diagnostic_bundle,
            diagnose_ssl,
            inspect_installed_tools,
            list_files,
            read_file,
            read_file_with_encoding,
            write_file,
            write_file_with_encoding,
            create_file,
            create_folder,
            delete_path,
            trash_path,
            restore_trash_path,
            rename_path,
            duplicate_path,
            copy_path,
            move_path,
            chmod_path,
            upload_files,
            extract_archive_to,
            create_archive,
            search_file_contents,
            search_file_contents_advanced,
            list_archive_entries,
            apply_windows_acl,
            list_environment_snapshots,
            create_environment_snapshot,
            restore_environment_snapshot,
            list_node_scripts,
            run_node_script,
            resource_monitor,
            kill_process,
            quit_app,
            hide_tray_panel,
            open_main_page,
            minimize_window,
            toggle_window_maximize,
            request_window_close
        ])
        .run(tauri::generate_context!())
        .expect("error while running LocalStack Pro");
}
