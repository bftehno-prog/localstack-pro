use crate::state::{AppResult, DatabaseInfo, LogLevel, Store};
use chrono::Utc;
use serde::Serialize;
use std::{
    fs::{self, File},
    net::{TcpStream, ToSocketAddrs},
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
pub struct DatabaseDiagnosticCheck {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub message: String,
    pub detail: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDiagnosticReport {
    pub database_id: String,
    pub database: String,
    pub generated_at: String,
    pub summary: String,
    pub ok: u32,
    pub warnings: u32,
    pub errors: u32,
    pub checks: Vec<DatabaseDiagnosticCheck>,
}

pub fn create_database(mut database: DatabaseInfo) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    normalize_database_request(&mut database)?;
    if snapshot.databases.iter().any(|item| {
        item.name.eq_ignore_ascii_case(&database.name) || item.id.eq_ignore_ascii_case(&database.id)
    }) {
        return Err(format!(
            "Database {} already exists. Choose another database name.",
            database.name
        ));
    }
    run_database_command(&snapshot, &database, "create")?;
    snapshot.databases.push(database.clone());
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Database",
        format!("Database {} created", database.name),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

fn normalize_database_request(database: &mut DatabaseInfo) -> AppResult<()> {
    database.name = database.name.trim().to_string();
    database.id = database.id.trim().to_string();
    database.user = database.user.trim().to_string();
    if database.id.is_empty() {
        database.id = database.name.clone();
    }
    if database.description.trim().is_empty() {
        database.description = format!("{} database", database.name);
    }
    if database.name.is_empty() {
        return Err("Database name is required.".to_string());
    }
    if database.user.is_empty() {
        return Err("Database user is required.".to_string());
    }
    if !is_database_token(&database.name) {
        return Err("Database name can contain only letters, numbers and underscores.".to_string());
    }
    if !is_database_token(&database.user) {
        return Err("Database user can contain only letters, numbers and underscores.".to_string());
    }
    if database.password.len() < 8 {
        return Err("Database password must be at least 8 characters.".to_string());
    }
    Ok(())
}

fn is_database_token(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn ensure_database_access(database: &DatabaseInfo) -> AppResult<()> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    run_database_command(&snapshot, database, "create")
}

pub fn test_database_connection(database_id: String) -> AppResult<DatabaseDiagnosticReport> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let database = snapshot
        .databases
        .iter()
        .find(|item| item.id == database_id || item.name.eq_ignore_ascii_case(&database_id))
        .cloned()
        .ok_or_else(|| "Database not found.".to_string())?;
    let service_id = match database.engine.as_str() {
        "PostgreSQL" => "postgresql",
        "MariaDB" => "mariadb",
        _ => "mysql",
    };
    let mut checks = Vec::new();
    let service = snapshot.services.iter().find(|item| item.id == service_id);
    let service_running = service
        .map(|item| item.status == crate::state::ServiceStatus::Running)
        .unwrap_or(false);
    push_check(
        &mut checks,
        "service",
        "Database service",
        if service_running { "ok" } else { "error" },
        service
            .map(|item| format!("{} is {:?}.", item.name, item.status))
            .unwrap_or_else(|| format!("Service {service_id} is not configured.")),
        service.and_then(|item| item.last_error.clone()),
        if service_running {
            None
        } else {
            Some(format!("Start the {} service.", database.engine))
        },
    );
    let port_ready = tcp_ready("127.0.0.1", database.port);
    push_check(
        &mut checks,
        "port",
        "Database port",
        if port_ready { "ok" } else { "error" },
        if port_ready {
            format!("127.0.0.1:{} accepts TCP connections.", database.port)
        } else {
            format!("127.0.0.1:{} is not accepting connections.", database.port)
        },
        None,
        if port_ready {
            None
        } else {
            Some("Check service status and port conflicts.".to_string())
        },
    );
    if let Some(service) = service {
        let client = if database.engine == "PostgreSQL" {
            "psql.exe"
        } else {
            "mysql.exe"
        };
        let client_path = find_database_tool(&service.executable_path, client);
        let client_ok = client_path.exists();
        push_check(
            &mut checks,
            "client",
            "Native client",
            if client_ok { "ok" } else { "error" },
            if client_ok {
                format!("Found {}", client_path.display())
            } else {
                format!("Missing {}", client_path.display())
            },
            None,
            if client_ok {
                None
            } else {
                Some("Run Services > Detect or install the database client.".to_string())
            },
        );
        if client_ok && service_running && port_ready {
            let result = run_connection_probe(&client_path, &database);
            let ok = result.is_ok();
            let detail = result.err();
            push_check(
                &mut checks,
                "credentials",
                "Database credentials",
                if ok { "ok" } else { "error" },
                if ok {
                    format!("Connected to {} as {}.", database.name, database.user)
                } else {
                    format!("Cannot connect to {} as {}.", database.name, database.user)
                },
                detail,
                if ok {
                    None
                } else {
                    Some("Recreate the database/user or update the saved password.".to_string())
                },
            );
        }
    }

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
        format!("{errors} database issue(s) found for {}.", database.name)
    } else if warnings > 0 {
        format!("{} is usable with {warnings} warning(s).", database.name)
    } else {
        format!("{} connection is healthy.", database.name)
    };
    Ok(DatabaseDiagnosticReport {
        database_id: database.id,
        database: database.name,
        generated_at: Utc::now().to_rfc3339(),
        summary,
        ok,
        warnings,
        errors,
        checks,
    })
}

pub fn delete_database(database_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let database = snapshot
        .databases
        .iter()
        .find(|item| item.id == database_id)
        .cloned()
        .ok_or_else(|| "Database not found.".to_string())?;
    run_database_command(&snapshot, &database, "drop")?;
    snapshot.databases.retain(|item| item.id != database_id);
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn backup_database(database_id: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let database = snapshot
        .databases
        .iter()
        .find(|item| item.id == database_id)
        .ok_or_else(|| "Database not found.".to_string())?;
    let target = store.dir.join("backups").join(format!(
        "{}_{}.sql",
        database.name,
        Utc::now().format("%Y%m%d_%H%M%S")
    ));
    let service_id = if database.engine == "PostgreSQL" {
        "postgresql"
    } else if database.engine == "MariaDB" {
        "mariadb"
    } else {
        "mysql"
    };
    let service = snapshot
        .services
        .iter()
        .find(|item| item.id == service_id)
        .ok_or_else(|| format!("Service {service_id} is not configured."))?;
    let dump = if database.engine == "PostgreSQL" {
        "pg_dump.exe"
    } else {
        "mysqldump.exe"
    };
    let dump_path = find_database_tool(&service.executable_path, dump);
    if !dump_path.exists() {
        return Err(format!(
            "Cannot back up {} because the native dump utility was not found: {}.",
            database.name,
            dump_path.display()
        ));
    }
    if service.status != crate::state::ServiceStatus::Running {
        return Err(format!(
            "{} service must be running before backup.",
            database.engine
        ));
    }
    let output =
        File::create(&target).map_err(|err| format!("Cannot create backup file: {err}"))?;
    let mut command = if database.engine == "PostgreSQL" {
        let mut command = Command::new(dump_path);
        command
            .args(["-w", "-U", &admin_user(database), "-d", &database.name])
            .stdout(Stdio::from(output))
            .stderr(Stdio::null());
        command
    } else {
        let mut command = Command::new(dump_path);
        command
            .args([
                "--connect-timeout=5",
                "-u",
                &admin_user(database),
                "--databases",
                &database.name,
            ])
            .stdout(Stdio::from(output))
            .stderr(Stdio::null());
        command
    };
    apply_database_password(&mut command, database);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let status = command
        .status()
        .map_err(|err| format!("Cannot run database backup: {err}"))?;
    if !status.success() {
        return Err(format!(
            "Database backup failed with exit code {:?}",
            status.code()
        ));
    }
    Ok(target.display().to_string())
}

pub fn import_database_sql(database_id: String, path: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let database = snapshot
        .databases
        .iter()
        .find(|item| item.id == database_id)
        .ok_or_else(|| "Database not found.".to_string())?;
    let target = if path.trim().is_empty() {
        store
            .dir
            .join("backups")
            .join(format!("{}_import.sql", database.name))
    } else {
        let candidate = PathBuf::from(path.trim());
        if candidate.is_absolute() {
            candidate
        } else {
            store.dir.join(candidate)
        }
    };
    if !target.exists() {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!("Cannot create import folder {}: {err}", parent.display())
            })?;
        }
        fs::write(
            &target,
            format!(
                "-- Put SQL for database {} here, then click Import SQL again.\n",
                database.name
            ),
        )
        .map_err(|err| format!("Cannot create SQL import file {}: {err}", target.display()))?;
        return Ok(format!(
            "SQL import file created: {}. Add SQL there and click Import SQL again.",
            target.display()
        ));
    }
    run_database_import(&snapshot, database, &target)?;
    Ok(format!(
        "Imported SQL into {} from {}",
        database.name,
        target.display()
    ))
}

fn run_database_command(
    snapshot: &crate::state::AppSnapshot,
    database: &DatabaseInfo,
    action: &str,
) -> AppResult<()> {
    let service_id = match database.engine.as_str() {
        "PostgreSQL" => "postgresql",
        "MariaDB" => "mariadb",
        _ => "mysql",
    };
    let service = snapshot
        .services
        .iter()
        .find(|item| item.id == service_id)
        .ok_or_else(|| format!("Service {service_id} is not configured."))?;
    let client = if database.engine == "PostgreSQL" {
        "psql.exe"
    } else {
        "mysql.exe"
    };
    let client_path = find_database_tool(&service.executable_path, client);
    if !client_path.exists() {
        return Err(format!(
            "Cannot {action} database {} because the native client was not found: {}.",
            database.name,
            client_path.display()
        ));
    }
    if service.status != crate::state::ServiceStatus::Running {
        return Err(format!(
            "{} service must be running before database {}.",
            database.engine, action
        ));
    }
    let sql_commands = if database.engine == "PostgreSQL" {
        postgres_sql(database, action)
    } else if action == "create" {
        vec![mysql_create_sql(database)]
    } else {
        vec![format!(
            "DROP DATABASE IF EXISTS `{}`; DROP USER IF EXISTS '{}'@'localhost'; DROP USER IF EXISTS '{}'@'127.0.0.1'; DROP USER IF EXISTS '{}'@'::1'; DROP USER IF EXISTS '{}'@'%'; FLUSH PRIVILEGES;",
            escape_mysql_identifier(&database.name),
            escape_mysql_string(&database.user),
            escape_mysql_string(&database.user),
            escape_mysql_string(&database.user),
            escape_mysql_string(&database.user)
        )]
    };
    for sql in sql_commands {
        let mut command = Command::new(&client_path);
        if database.engine == "PostgreSQL" {
            command.args([
                "-w",
                "-U",
                &admin_user(database),
                "-d",
                "postgres",
                "-c",
                &sql,
            ]);
        } else {
            command.args([
                "--connect-timeout=5",
                "-u",
                &admin_user(database),
                "-e",
                &sql,
            ]);
        }
        apply_database_password(&mut command, database);
        command.stdin(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let output = command
            .output()
            .map_err(|err| format!("Cannot execute database command: {err}"))?;
        if !output.status.success() {
            let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let detail = if detail.is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                detail
            };
            return Err(format!(
                "Database command failed with exit code {:?}. {}",
                output.status.code(),
                detail
            ));
        }
    }
    Ok(())
}

fn run_database_import(
    snapshot: &crate::state::AppSnapshot,
    database: &DatabaseInfo,
    source: &Path,
) -> AppResult<()> {
    let service_id = match database.engine.as_str() {
        "PostgreSQL" => "postgresql",
        "MariaDB" => "mariadb",
        _ => "mysql",
    };
    let service = snapshot
        .services
        .iter()
        .find(|item| item.id == service_id)
        .ok_or_else(|| format!("Service {service_id} is not configured."))?;
    if service.status != crate::state::ServiceStatus::Running {
        return Err(format!(
            "{} service must be running before SQL import.",
            database.engine
        ));
    }
    let client = if database.engine == "PostgreSQL" {
        "psql.exe"
    } else {
        "mysql.exe"
    };
    let client_path = find_database_tool(&service.executable_path, client);
    if !client_path.exists() {
        return Err(format!(
            "Cannot import SQL into {} because the native client was not found: {}.",
            database.name,
            client_path.display()
        ));
    }
    let input = File::open(source)
        .map_err(|err| format!("Cannot open SQL import file {}: {err}", source.display()))?;
    let mut command = Command::new(client_path);
    if database.engine == "PostgreSQL" {
        command.args(["-w", "-U", &admin_user(database), "-d", &database.name]);
    } else {
        command.args([
            "--connect-timeout=5",
            "-u",
            &admin_user(database),
            &database.name,
        ]);
    }
    apply_database_password(&mut command, database);
    command
        .stdin(Stdio::from(input))
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot execute SQL import: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!(
            "SQL import failed with exit code {:?}. {}",
            output.status.code(),
            detail
        ));
    }
    Ok(())
}

fn run_connection_probe(client_path: &Path, database: &DatabaseInfo) -> AppResult<()> {
    let mut command = Command::new(client_path);
    if database.engine == "PostgreSQL" {
        command.args([
            "-w",
            "-h",
            "127.0.0.1",
            "-p",
            &database.port.to_string(),
            "-U",
            &database.user,
            "-d",
            &database.name,
            "-c",
            "SELECT 1;",
        ]);
        command.env("PGPASSWORD", &database.password);
    } else {
        command.args([
            "--connect-timeout=5",
            "-h",
            "127.0.0.1",
            "-P",
            &database.port.to_string(),
            "-u",
            &database.user,
            &database.name,
            "-e",
            "SELECT 1;",
        ]);
        command.env("MYSQL_PWD", &database.password);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start database client: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if detail.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            detail
        };
        Err(format!(
            "Database client exited with code {:?}. {}",
            output.status.code(),
            detail
        ))
    }
}

fn push_check(
    checks: &mut Vec<DatabaseDiagnosticCheck>,
    id: &str,
    title: &str,
    severity: &str,
    message: String,
    detail: Option<String>,
    action: Option<String>,
) {
    checks.push(DatabaseDiagnosticCheck {
        id: id.to_string(),
        title: title.to_string(),
        severity: severity.to_string(),
        message,
        detail,
        action,
    });
}

fn tcp_ready(host: &str, port: u16) -> bool {
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    addresses
        .into_iter()
        .any(|address| TcpStream::connect_timeout(&address, Duration::from_millis(180)).is_ok())
}

fn find_database_tool(executable_path: &str, tool: &str) -> PathBuf {
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

fn admin_user(database: &DatabaseInfo) -> String {
    std::env::var(match database.engine.as_str() {
        "PostgreSQL" => "LOCALSTACK_POSTGRES_ADMIN_USER",
        "MariaDB" => "LOCALSTACK_MARIADB_ADMIN_USER",
        _ => "LOCALSTACK_MYSQL_ADMIN_USER",
    })
    .unwrap_or_else(|_| {
        if database.engine == "PostgreSQL" {
            "postgres".to_string()
        } else {
            "root".to_string()
        }
    })
}

fn apply_database_password(command: &mut Command, database: &DatabaseInfo) {
    let key = match database.engine.as_str() {
        "PostgreSQL" => "LOCALSTACK_POSTGRES_ADMIN_PASSWORD",
        "MariaDB" => "LOCALSTACK_MARIADB_ADMIN_PASSWORD",
        _ => "LOCALSTACK_MYSQL_ADMIN_PASSWORD",
    };
    if let Ok(password) = std::env::var(key) {
        if database.engine == "PostgreSQL" {
            command.env("PGPASSWORD", password);
        } else {
            command.env("MYSQL_PWD", password);
        }
    }
}

fn escape_identifier(value: &str) -> String {
    value.replace('"', "\"\"")
}

fn escape_mysql_identifier(value: &str) -> String {
    value.replace('`', "``")
}

fn escape_mysql_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn mysql_create_sql(database: &DatabaseInfo) -> String {
    format!(
        "CREATE DATABASE IF NOT EXISTS `{db}`; CREATE USER IF NOT EXISTS '{user}'@'localhost' IDENTIFIED BY '{password}'; ALTER USER '{user}'@'localhost' IDENTIFIED BY '{password}'; GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'localhost'; CREATE USER IF NOT EXISTS '{user}'@'127.0.0.1' IDENTIFIED BY '{password}'; ALTER USER '{user}'@'127.0.0.1' IDENTIFIED BY '{password}'; GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'127.0.0.1'; CREATE USER IF NOT EXISTS '{user}'@'::1' IDENTIFIED BY '{password}'; ALTER USER '{user}'@'::1' IDENTIFIED BY '{password}'; GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'::1'; CREATE USER IF NOT EXISTS '{user}'@'%' IDENTIFIED BY '{password}'; ALTER USER '{user}'@'%' IDENTIFIED BY '{password}'; GRANT ALL PRIVILEGES ON `{db}`.* TO '{user}'@'%'; FLUSH PRIVILEGES;",
        db = escape_mysql_identifier(&database.name),
        user = escape_mysql_string(&database.user),
        password = escape_mysql_string(&database.password)
    )
}

fn postgres_sql(database: &DatabaseInfo, action: &str) -> Vec<String> {
    let db = escape_identifier(&database.name);
    let user = escape_identifier(&database.user);
    let user_literal = database.user.replace('\'', "''");
    let password = database.password.replace('\'', "''");
    if action == "create" {
        vec![
            format!(
                "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = '{user_literal}') THEN CREATE ROLE \"{user}\" LOGIN PASSWORD '{password}'; END IF; END $$;"
            ),
            format!("CREATE DATABASE \"{db}\" OWNER \"{user}\";"),
        ]
    } else {
        vec![
            format!("DROP DATABASE IF EXISTS \"{db}\";"),
            format!("DROP ROLE IF EXISTS \"{user}\";"),
        ]
    }
}
