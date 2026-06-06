use crate::state::{AppResult, Store};
use chrono::Utc;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFileTail {
    pub source: String,
    pub path: String,
    pub generated_at: String,
    pub lines: Vec<String>,
}

pub fn clear_logs() -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    snapshot.logs.clear();
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn export_logs(path: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let target = if path.ends_with(".txt") {
        let target = PathBuf::from(path);
        if target.is_absolute() {
            target
        } else {
            store.dir.join(target)
        }
    } else {
        store.dir.join("logs").join("logs-export.txt")
    };
    let lines = snapshot
        .logs
        .iter()
        .map(|log| {
            format!(
                "{} {:?} [{}] {}",
                log.timestamp, log.level, log.service, log.message
            )
        })
        .collect::<Vec<_>>();
    fs::write(&target, lines.join("\n")).map_err(|err| format!("Cannot export logs: {err}"))?;
    Ok(target.display().to_string())
}

pub fn tail_log_file(source: String, lines: Option<u32>) -> AppResult<LogFileTail> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    store.ensure_host_files(&snapshot);
    store.ensure_service_files(&snapshot);
    let source = source.trim();
    let source = if source.is_empty() {
        "application"
    } else {
        source
    };
    let path = resolve_log_path(&store, &snapshot, source)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Cannot create log folder {}: {err}", parent.display()))?;
    }
    if source.eq_ignore_ascii_case("application") {
        materialize_application_log(&store, &snapshot, &path)?;
    } else if !path.exists() {
        fs::write(&path, "")
            .map_err(|err| format!("Cannot create log file {}: {err}", path.display()))?;
    }
    let limit = lines.unwrap_or(200).clamp(25, 2000) as usize;
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("Cannot read log file {}: {err}", path.display()))?;
    let mut tail = text
        .lines()
        .rev()
        .take(limit)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    tail.reverse();
    if tail.is_empty() {
        tail.push(format!("{} is empty.", path.display()));
    }
    Ok(LogFileTail {
        source: source.to_string(),
        path: path.display().to_string(),
        generated_at: Utc::now().to_rfc3339(),
        lines: tail,
    })
}

pub fn tail_log_for_cli(source: String) -> AppResult<String> {
    let tail = tail_log_file(source, Some(80))?;
    Ok(format!(
        "source={} path={} lines={}",
        tail.source,
        tail.path,
        tail.lines.len()
    ))
}

fn resolve_log_path(
    store: &Store,
    snapshot: &crate::state::AppSnapshot,
    source: &str,
) -> AppResult<PathBuf> {
    if source.eq_ignore_ascii_case("application") {
        return Ok(store.dir.join("logs").join("application.log"));
    }
    if let Some(service) = snapshot
        .services
        .iter()
        .find(|service| service.id.eq_ignore_ascii_case(source))
    {
        return Ok(PathBuf::from(&service.log_path));
    }
    let host_key = source.strip_prefix("host:").unwrap_or(source);
    if let Some(host) = snapshot.hosts.iter().find(|host| {
        host.domain.eq_ignore_ascii_case(host_key) || host.id.eq_ignore_ascii_case(host_key)
    }) {
        let logs = PathBuf::from(&host.root_folder).join("logs");
        let candidates = [
            logs.join(format!("{}-ssl-error.log", host.domain)),
            logs.join(format!("{}-error.log", host.domain)),
            logs.join("error.log"),
            logs.join("access.log"),
        ];
        return Ok(candidates
            .iter()
            .find(|candidate| candidate.exists())
            .cloned()
            .unwrap_or_else(|| logs.join(format!("{}-error.log", host.domain))));
    }
    let direct = PathBuf::from(source);
    if direct.is_absolute() && is_allowed_direct_path(store, snapshot, &direct) {
        return Ok(direct);
    }
    Err(format!(
        "Unknown log source '{source}'. Choose application, a service id, or host:<domain>."
    ))
}

fn is_allowed_direct_path(
    store: &Store,
    snapshot: &crate::state::AppSnapshot,
    path: &Path,
) -> bool {
    path.starts_with(&store.dir)
        || snapshot
            .services
            .iter()
            .any(|service| path == Path::new(&service.log_path))
        || snapshot
            .hosts
            .iter()
            .any(|host| path.starts_with(PathBuf::from(&host.root_folder).join("logs")))
}

fn materialize_application_log(
    store: &Store,
    snapshot: &crate::state::AppSnapshot,
    path: &Path,
) -> AppResult<()> {
    let lines = snapshot
        .logs
        .iter()
        .map(|log| {
            format!(
                "{} {:?} [{}] {}{}",
                log.timestamp,
                log.level,
                log.service,
                log.message,
                log.detail
                    .as_ref()
                    .map(|detail| format!(" | {detail}"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let content = if lines.is_empty() {
        format!(
            "{} INFO [LocalStack Pro] No application log entries yet.",
            Utc::now().to_rfc3339()
        )
    } else {
        lines.join("\n")
    };
    fs::write(path, content).map_err(|err| {
        format!(
            "Cannot update application log in {}: {err}",
            store.dir.join("logs").display()
        )
    })
}
