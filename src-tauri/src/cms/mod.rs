use crate::{
    database, hosts, services,
    state::{AppResult, CmsInstallInfo, DatabaseInfo, HostInfo, LogLevel, ServiceStatus, Store},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmsTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub download_url: String,
    pub document_root: String,
    pub requires_database: bool,
    pub default_database_engine: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CmsInstallRequest {
    pub template_id: String,
    pub domain: String,
    pub root_folder: String,
    pub php_version: String,
    pub web_server: String,
    pub ssl: bool,
    pub database_engine: String,
    pub create_database: bool,
    #[serde(default)]
    pub database_name: Option<String>,
    #[serde(default)]
    pub database_user: Option<String>,
    #[serde(default)]
    pub database_password: Option<String>,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CmsUpdateInfo {
    pub domain: String,
    pub template_id: String,
    pub name: String,
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub can_update: bool,
    pub message: String,
}

pub fn cms_templates() -> Vec<CmsTemplate> {
    vec![
        template(TemplateSpec {
            id: "nextjs",
            name: "Next.js",
            description: "React full-stack application with App Router and local dev server.",
            category: "Node.js",
            download_url: "localstack://node/nextjs",
            document_root: ".",
            requires_database: false,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "node-express",
            name: "Node.js Express",
            description: "Minimal Express application with local API routes.",
            category: "Node.js",
            download_url: "localstack://node/express",
            document_root: ".",
            requires_database: false,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "vite-react",
            name: "Vite React",
            description: "Fast React single-page application powered by Vite.",
            category: "Node.js",
            download_url: "localstack://node/vite-react",
            document_root: ".",
            requires_database: false,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "meteor-blog-cms",
            name: "Meteor Blog CMS",
            description: "Meteor 3 + Blaze blog CMS with admin area and local MongoDB.",
            category: "Node.js",
            download_url: "localstack://node/meteor-blog-cms",
            document_root: ".",
            requires_database: false,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "wordpress",
            name: "WordPress",
            description: "Classic PHP CMS for blogs, shops and company sites.",
            category: "CMS",
            download_url: "https://wordpress.org/latest.zip",
            document_root: "public",
            requires_database: true,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "joomla",
            name: "Joomla",
            description: "Latest full package from the official Joomla release channel.",
            category: "CMS",
            download_url: "https://downloads.joomla.org/latest",
            document_root: "public",
            requires_database: true,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "drupal",
            name: "Drupal",
            description: "Latest recommended Drupal core ZIP from Drupal.org.",
            category: "CMS",
            download_url: "https://www.drupal.org/download-latest/zip",
            document_root: "public",
            requires_database: true,
            default_database_engine: "MySQL",
        }),
        template(TemplateSpec {
            id: "grav",
            name: "Grav",
            description: "Fast flat-file CMS, no database required.",
            category: "Flat-file",
            download_url: "https://getgrav.org/download/core/grav/latest",
            document_root: "public",
            requires_database: false,
            default_database_engine: "MySQL",
        }),
    ]
}

pub fn check_cms_updates() -> AppResult<Vec<CmsUpdateInfo>> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    snapshot
        .cms_installations
        .iter()
        .map(cms_update_info)
        .collect()
}

pub fn update_cms(domain: String) -> AppResult<CmsUpdateInfo> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let installation = snapshot
        .cms_installations
        .iter()
        .find(|item| item.domain.eq_ignore_ascii_case(domain.trim()))
        .cloned()
        .ok_or_else(|| format!("CMS installation for {} was not found.", domain.trim()))?;
    let template = cms_templates()
        .into_iter()
        .find(|item| item.id == installation.template_id)
        .ok_or_else(|| format!("CMS template {} was not found.", installation.template_id))?;
    if is_node_template(&template.id) {
        return Err(format!(
            "{} is managed with its package manager, not the CMS updater.",
            template.name
        ));
    }

    let before = cms_update_info(&installation)?;
    if !before.can_update {
        return Err(before.message);
    }
    if !before.update_available {
        return Ok(before);
    }

    let host = snapshot
        .hosts
        .iter()
        .find(|item| item.domain.eq_ignore_ascii_case(&installation.domain))
        .cloned()
        .ok_or_else(|| format!("Host {} was not found for this CMS.", installation.domain))?;
    let backup = PathBuf::from(&snapshot.settings.backups_folder).join(format!(
        "{}-{}-cms-update.zip",
        installation.domain,
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    crate::tools::backup_host(host.id.clone(), backup.display().to_string())?;

    let public = PathBuf::from(&installation.root_folder).join(&installation.document_root);
    let temp = store
        .dir
        .join("temp")
        .join(format!("cms-update-{}", Uuid::new_v4()));
    let archive = temp.join("package.zip");
    let extracted = temp.join("extract");
    fs::create_dir_all(&extracted)
        .map_err(|err| format!("Cannot create CMS update temp folder: {err}"))?;
    let installer_url = resolve_latest_installer_url(&template)?;
    let result = (|| -> AppResult<()> {
        download_and_extract(&installer_url, &archive, &extracted)?;
        let source = extracted_content_root(&extracted)?;
        copy_cms_update_files(&template.id, &source, &public)
    })();
    let _ = fs::remove_dir_all(&temp);
    result?;
    clear_wordpress_maintenance_marker(&template.id, &public)?;

    let mut updated_snapshot = store.load_static()?;
    store.log(
        &mut updated_snapshot,
        LogLevel::Info,
        "CMS",
        format!(
            "{} updated for {}. Backup: {}",
            template.name,
            installation.domain,
            backup.display()
        ),
        None,
    );
    store.save(&updated_snapshot)?;
    if host.status == ServiceStatus::Running {
        let service_id = if host.web_server.eq_ignore_ascii_case("nginx") {
            "nginx"
        } else {
            "apache"
        };
        let _ = services::restart_service(service_id.to_string());
    }

    let mut after = cms_update_info(&installation)?;
    after.message = format!("Updated successfully. Backup: {}", backup.display());
    Ok(after)
}

pub fn update_all_cms() -> AppResult<Vec<CmsUpdateInfo>> {
    let updates = check_cms_updates()?;
    let mut results = Vec::with_capacity(updates.len());
    for update in updates {
        if update.can_update && update.update_available {
            match update_cms(update.domain.clone()) {
                Ok(result) => results.push(result),
                Err(error) => results.push(CmsUpdateInfo {
                    message: error,
                    ..update
                }),
            }
        } else {
            results.push(update);
        }
    }
    Ok(results)
}

pub fn run_configured_auto_updates() {
    let Ok(store) = Store::new() else {
        return;
    };
    let Ok(snapshot) = store.load_static() else {
        return;
    };
    if snapshot.settings.auto_update_cms {
        let _ = update_all_cms();
    }
}

fn cms_update_info(installation: &CmsInstallInfo) -> AppResult<CmsUpdateInfo> {
    let template = cms_templates()
        .into_iter()
        .find(|item| item.id == installation.template_id)
        .ok_or_else(|| format!("CMS template {} was not found.", installation.template_id))?;
    if is_node_template(&template.id) {
        return Ok(CmsUpdateInfo {
            domain: installation.domain.clone(),
            template_id: template.id,
            name: template.name.clone(),
            current_version: "Package-managed".to_string(),
            latest_version: "-".to_string(),
            update_available: false,
            can_update: false,
            message: "Use npm or the project package manager to update this application."
                .to_string(),
        });
    }

    let public = PathBuf::from(&installation.root_folder).join(&installation.document_root);
    if !cms_files_match(&template.id, &public) {
        return Ok(CmsUpdateInfo {
            domain: installation.domain.clone(),
            template_id: template.id,
            name: template.name.clone(),
            current_version: "Unknown".to_string(),
            latest_version: "Unknown".to_string(),
            update_available: false,
            can_update: false,
            message: format!(
                "{} is not a valid {} installation.",
                public.display(),
                template.name
            ),
        });
    }

    let current_version =
        installed_cms_version(&template.id, &public).unwrap_or_else(|| "Unknown".to_string());
    let latest_version = latest_cms_version(&template)?;
    let update_available = version_is_newer(&latest_version, &current_version);
    Ok(CmsUpdateInfo {
        domain: installation.domain.clone(),
        template_id: template.id,
        name: template.name,
        current_version,
        latest_version,
        update_available,
        can_update: true,
        message: if update_available {
            "A core update is available. A host backup is created before installation.".to_string()
        } else {
            "CMS core is up to date.".to_string()
        },
    })
}

fn installed_cms_version(template_id: &str, public: &Path) -> Option<String> {
    let file = match template_id {
        "wordpress" => public.join("wp-includes").join("version.php"),
        "joomla" => public.join("libraries").join("src").join("Version.php"),
        "drupal" => public.join("core").join("lib").join("Drupal.php"),
        "grav" => public.join("system").join("defines.php"),
        _ => return None,
    };
    let text = fs::read_to_string(file).ok()?;
    match template_id {
        "wordpress" => capture_version(&text, "$wp_version = '")
            .or_else(|| capture_version(&text, "$wp_version = \"")),
        "joomla" => {
            let major = capture_version(&text, "MAJOR_VERSION = '")?;
            let minor = capture_version(&text, "MINOR_VERSION = '")?;
            let patch = capture_version(&text, "PATCH_VERSION = '")?;
            Some(format!("{major}.{minor}.{patch}"))
        }
        "drupal" => capture_version(&text, "const VERSION = '")
            .or_else(|| capture_version(&text, "const VERSION = \"")),
        "grav" => capture_version(&text, "GRAV_VERSION', '").or_else(|| {
            capture_version(&text, "GRAV_VERSION = '")
                .or_else(|| capture_version(&text, "GRAV_VERSION = \""))
        }),
        _ => None,
    }
}

fn capture_version(text: &str, prefix: &str) -> Option<String> {
    let value = text.split_once(prefix)?.1;
    let end = value.find(['\'', '"'])?;
    let version = value[..end].trim();
    (!version.is_empty()).then(|| version.to_string())
}

fn latest_cms_version(template: &CmsTemplate) -> AppResult<String> {
    let script = match template.id.as_str() {
        "wordpress" => "(Invoke-RestMethod -UseBasicParsing -Uri 'https://api.wordpress.org/core/version-check/1.7/').offers | Select-Object -First 1 -ExpandProperty version".to_string(),
        "joomla" => {
            let url = resolve_latest_installer_url(template)?;
            return url
                .split("Joomla_")
                .nth(1)
                .and_then(|part| part.split("-Stable").next())
                .map(|value| value.replace('_', "."))
                .ok_or_else(|| "Cannot read the Joomla version from the official installer URL.".to_string());
        }
        "drupal" => "[xml]$feed=(Invoke-WebRequest -UseBasicParsing -Uri 'https://updates.drupal.org/release-history/drupal/current').Content; ($feed.project.releases.release | Select-Object -First 1).version".to_string(),
        "grav" => "(Invoke-RestMethod -UseBasicParsing -Uri 'https://api.github.com/repos/getgrav/grav/releases/latest').tag_name.TrimStart('v')".to_string(),
        _ => return Err(format!("CMS updater does not support {}.", template.name)),
    };
    let output = run_hidden_powershell(&script, "Cannot check the latest CMS version")?;
    let version = output.trim().trim_start_matches('v').to_string();
    if version.is_empty() {
        return Err(format!(
            "The latest {} version was not returned by its official channel.",
            template.name
        ));
    }
    Ok(version)
}

fn run_hidden_powershell(script: &str, context: &str) -> AppResult<String> {
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
        .map_err(|err| format!("{context}: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let parts = |value: &str| {
        value
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    let latest = parts(latest);
    let current = parts(current);
    if latest.is_empty() || current.is_empty() {
        return false;
    }
    let width = latest.len().max(current.len());
    (0..width)
        .find_map(|index| {
            let left = *latest.get(index).unwrap_or(&0);
            let right = *current.get(index).unwrap_or(&0);
            (left != right).then_some(left > right)
        })
        .unwrap_or(false)
}

fn copy_cms_update_files(template_id: &str, source: &Path, target: &Path) -> AppResult<()> {
    let protected: &[&str] = match template_id {
        "wordpress" => &["wp-content", "wp-config.php", ".htaccess"],
        "joomla" => &[
            "configuration.php",
            "images",
            "media",
            "cache",
            "tmp",
            "logs",
            ".htaccess",
        ],
        "drupal" => &["sites", "modules", "themes", "profiles", ".htaccess"],
        "grav" => &["user", "backup", "cache", "logs", "images", ".htaccess"],
        _ => &[],
    };
    fs::create_dir_all(target).map_err(|err| format!("Cannot prepare CMS target folder: {err}"))?;
    for entry in
        fs::read_dir(source).map_err(|err| format!("Cannot read CMS update package: {err}"))?
    {
        let entry = entry.map_err(|err| format!("Cannot read CMS update entry: {err}"))?;
        let name = entry.file_name();
        let normalized = name.to_string_lossy().to_ascii_lowercase();
        if protected.iter().any(|item| *item == normalized) {
            continue;
        }
        let destination = target.join(&name);
        if entry.path().is_dir() {
            copy_dir_all(&entry.path(), &destination, true)?;
        } else {
            fs::copy(entry.path(), &destination).map_err(|err| {
                format!("Cannot update CMS file {}: {err}", destination.display())
            })?;
        }
    }
    Ok(())
}

fn clear_wordpress_maintenance_marker(template_id: &str, public: &Path) -> AppResult<()> {
    if template_id != "wordpress" {
        return Ok(());
    }
    let marker = public.join(".maintenance");
    if marker.is_file() {
        fs::remove_file(&marker).map_err(|err| {
            format!(
                "Cannot remove the completed WordPress maintenance marker {}: {err}",
                marker.display()
            )
        })?;
    }
    Ok(())
}

pub fn install_cms(request: CmsInstallRequest) -> AppResult<crate::state::AppSnapshot> {
    validate_request(&request)?;
    let template = cms_templates()
        .into_iter()
        .find(|item| item.id == request.template_id)
        .ok_or_else(|| "CMS template was not found.".to_string())?;
    if is_node_template(&template.id) {
        return install_node_project(template, request);
    }
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let existing_host = snapshot
        .hosts
        .iter()
        .find(|host| host.domain.eq_ignore_ascii_case(&request.domain))
        .cloned();

    let root = PathBuf::from(request.root_folder.trim());
    let public = root.join(&template.document_root);
    let use_existing_files = public.exists()
        && !request.overwrite
        && !directory_is_empty(&public)?
        && cms_files_match(&template.id, &public);
    if public.exists() && !request.overwrite && !directory_is_empty(&public)? && !use_existing_files
    {
        return Err(format!(
            "{} is not empty and does not look like {}. Enable overwrite or choose another folder.",
            public.display(),
            template.name
        ));
    }
    fs::create_dir_all(&public).map_err(|err| format!("Cannot create project folder: {err}"))?;

    let database = if request.create_database && template.requires_database {
        Some(ensure_database(&snapshot, &request)?)
    } else {
        None
    };

    let temp = store
        .dir
        .join("temp")
        .join(format!("cms-{}", Uuid::new_v4()));
    let installer_url = (!use_existing_files)
        .then(|| resolve_latest_installer_url(&template))
        .transpose()?;
    if !use_existing_files {
        let archive = temp.join("package.zip");
        let extracted = temp.join("extract");
        fs::create_dir_all(&extracted)
            .map_err(|err| format!("Cannot create temp folder: {err}"))?;
        download_and_extract(
            installer_url
                .as_deref()
                .ok_or_else(|| "CMS installer URL was not resolved.".to_string())?,
            &archive,
            &extracted,
        )?;
        let source = extracted_content_root(&extracted)?;
        copy_dir_all(&source, &public, request.overwrite)?;
    }

    write_cms_config(&template, &public, database.as_ref(), &request)?;
    remove_localstack_placeholder_index(&public)?;
    validate_installed_cms(&template, &public, database.as_ref())?;
    write_install_metadata(
        &root,
        &template,
        &request,
        database.as_ref().map(|item| item.name.as_str()),
    )?;
    let _ = fs::remove_dir_all(&temp);

    let now = Utc::now().to_rfc3339();
    let web_server = "Apache".to_string();
    let mut env = HashMap::new();
    env.insert("APP_ENV".to_string(), "local".to_string());
    env.insert("APP_DEBUG".to_string(), "true".to_string());
    env.insert(
        "APP_URL".to_string(),
        format!(
            "{}://{}",
            if request.ssl { "https" } else { "http" },
            request.domain
        ),
    );
    if let Some(database) = &database {
        env.insert("DB_DATABASE".to_string(), database.name.clone());
        env.insert("DB_USERNAME".to_string(), database.user.clone());
        env.insert("DB_PASSWORD".to_string(), database.password.clone());
        env.insert(
            "DB_HOST".to_string(),
            database_host(&database.engine).to_string(),
        );
        env.insert("DB_PORT".to_string(), database.port.to_string());
    }

    let host = HostInfo {
        id: request.domain.clone(),
        domain: request.domain.clone(),
        root_folder: root.display().to_string(),
        document_root: template.document_root.clone(),
        php_version: request.php_version.clone(),
        web_server: web_server.clone(),
        ssl: request.ssl,
        environment: "Development".to_string(),
        http_port: 80,
        https_port: 443,
        database: database
            .as_ref()
            .map(|item| item.name.clone())
            .unwrap_or_default(),
        mail_service: "Mailpit".to_string(),
        env_variables: env,
        rewrite_rules: String::new(),
        notes: format!("{} installed by LocalStack Pro.", template.name),
        status: ServiceStatus::Stopped,
        tags: vec!["cms".to_string(), template.id.clone()],
        created_at: existing_host
            .as_ref()
            .map(|host| host.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
    };
    let _ = hosts::save_host(host)?;
    if template.id == "meteor-blog-cms" {
        cleanup_meteor_project_after_host_save(&root)?;
    }

    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if let Some(position) = snapshot
        .hosts
        .iter()
        .position(|host| host.domain.eq_ignore_ascii_case(&request.domain))
    {
        let host = snapshot.hosts.remove(position);
        snapshot.hosts.insert(0, host);
    }
    let install = CmsInstallInfo {
        id: Uuid::new_v4().to_string(),
        template_id: template.id.clone(),
        name: template.name.clone(),
        domain: request.domain.clone(),
        root_folder: root.display().to_string(),
        document_root: template.document_root.clone(),
        database: database.map(|item| item.name),
        installed_at: now,
        status: "installed".to_string(),
    };
    if let Some(existing) = snapshot.cms_installations.iter_mut().find(|item| {
        item.domain.eq_ignore_ascii_case(&request.domain)
            || (item.template_id == template.id
                && item
                    .root_folder
                    .eq_ignore_ascii_case(&root.display().to_string()))
    }) {
        *existing = install;
        if let Some(position) = snapshot
            .cms_installations
            .iter()
            .position(|item| item.domain.eq_ignore_ascii_case(&request.domain))
        {
            let item = snapshot.cms_installations.remove(position);
            snapshot.cms_installations.insert(0, item);
        }
    } else {
        snapshot.cms_installations.insert(0, install);
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "CMS",
        if use_existing_files {
            format!("{} attached at {}", template.name, request.domain)
        } else {
            format!(
                "{} latest installer downloaded and installed at {}",
                template.name, request.domain
            )
        },
        None,
    );
    store.save(&snapshot)?;
    let service_id = "apache".to_string();
    services::restart_service(service_id).map_err(|err| {
        format!(
            "{} was installed, but LocalStack Pro could not apply the web server runtime config: {err}",
            template.name
        )
    })?;
    Ok(snapshot)
}

struct TemplateSpec<'a> {
    id: &'a str,
    name: &'a str,
    description: &'a str,
    category: &'a str,
    download_url: &'a str,
    document_root: &'a str,
    requires_database: bool,
    default_database_engine: &'a str,
}

fn template(spec: TemplateSpec<'_>) -> CmsTemplate {
    CmsTemplate {
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        description: spec.description.to_string(),
        category: spec.category.to_string(),
        download_url: spec.download_url.to_string(),
        document_root: spec.document_root.to_string(),
        requires_database: spec.requires_database,
        default_database_engine: spec.default_database_engine.to_string(),
    }
}

fn is_node_template(template_id: &str) -> bool {
    matches!(
        template_id,
        "nextjs" | "node-express" | "vite-react" | "meteor-blog-cms"
    )
}

fn install_node_project(
    template: CmsTemplate,
    request: CmsInstallRequest,
) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let existing_host = snapshot
        .hosts
        .iter()
        .find(|host| host.domain.eq_ignore_ascii_case(&request.domain))
        .cloned();
    let root = PathBuf::from(request.root_folder.trim());
    if root.exists()
        && !request.overwrite
        && !directory_is_empty(&root)?
        && !root.join("package.json").is_file()
    {
        return Err(format!(
            "{} is not empty and does not look like a Node.js project. Enable overwrite or choose another folder.",
            root.display()
        ));
    }
    fs::create_dir_all(&root)
        .map_err(|err| format!("Cannot create Node.js project folder: {err}"))?;
    if request.overwrite || !root.join("package.json").is_file() {
        if template.id == "meteor-blog-cms" {
            copy_bundled_meteor_project(&root, request.overwrite)?;
        } else {
            write_node_template_files(&template.id, &template.name, &root, &request)?;
        }
    }
    if template.id == "meteor-blog-cms" {
        prepare_meteor_project(&root)?;
        ensure_meteor_runtime()?;
    }
    ensure_node_runtime(&snapshot)?;
    npm_install(&root)?;

    let port = allocate_node_port(&snapshot, &request.domain);
    let now = Utc::now().to_rfc3339();
    let mut env = HashMap::new();
    env.insert("APP_ENV".to_string(), "local".to_string());
    env.insert("APP_URL".to_string(), format!("http://{}", request.domain));
    env.insert("LOCALSTACK_NODE_PORT".to_string(), port.to_string());
    env.insert("LOCALSTACK_NODE_SCRIPT".to_string(), "dev".to_string());
    env.insert(
        "LOCALSTACK_NODE_KIND".to_string(),
        if template.id == "meteor-blog-cms" {
            "meteor".to_string()
        } else {
            template.id.clone()
        },
    );
    if template.id == "meteor-blog-cms" {
        env.insert("ROOT_URL".to_string(), format!("http://{}", request.domain));
        env.insert(
            "TOOL_NODE_FLAGS".to_string(),
            "--max-old-space-size=8192".to_string(),
        );
        env.insert(
            "METEOR_SETTINGS".to_string(),
            root.join("settings.json").display().to_string(),
        );
    }

    let host = HostInfo {
        id: request.domain.clone(),
        domain: request.domain.clone(),
        root_folder: root.display().to_string(),
        document_root: ".".to_string(),
        php_version: request.php_version.clone(),
        web_server: "Apache".to_string(),
        ssl: false,
        environment: "Development".to_string(),
        http_port: 80,
        https_port: 443,
        database: String::new(),
        mail_service: "Disabled".to_string(),
        env_variables: env,
        rewrite_rules: String::new(),
        notes: format!("{} installed by LocalStack Pro.", template.name),
        status: ServiceStatus::Stopped,
        tags: if template.id == "meteor-blog-cms" {
            vec![
                "node".to_string(),
                "meteor".to_string(),
                template.id.clone(),
            ]
        } else {
            vec!["node".to_string(), template.id.clone()]
        },
        created_at: existing_host
            .as_ref()
            .map(|host| host.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
    };
    let _ = hosts::save_host(host)?;

    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if let Some(position) = snapshot
        .hosts
        .iter()
        .position(|host| host.domain.eq_ignore_ascii_case(&request.domain))
    {
        let host = snapshot.hosts.remove(position);
        snapshot.hosts.insert(0, host);
    }
    let install = CmsInstallInfo {
        id: Uuid::new_v4().to_string(),
        template_id: template.id.clone(),
        name: template.name.clone(),
        domain: request.domain.clone(),
        root_folder: root.display().to_string(),
        document_root: ".".to_string(),
        database: None,
        installed_at: now,
        status: "installed".to_string(),
    };
    if let Some(existing) = snapshot
        .cms_installations
        .iter_mut()
        .find(|item| item.domain.eq_ignore_ascii_case(&request.domain))
    {
        *existing = install;
    } else {
        snapshot.cms_installations.insert(0, install);
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Node.js",
        format!("{} installed at {}", template.name, request.domain),
        None,
    );
    store.save(&snapshot)?;
    let _ = services::start_service("node-proxy".to_string())?;
    services::restart_service("apache".to_string()).map_err(|err| {
        format!(
            "{} was installed, but LocalStack Pro could not apply the Apache proxy config: {err}",
            template.name
        )
    })
}

fn copy_bundled_meteor_project(root: &Path, overwrite: bool) -> AppResult<()> {
    let archive = bundled_cms_archive("meteor-blog-cms.zip")?;
    let store = Store::new()?;
    let temp = store
        .dir
        .join("temp")
        .join(format!("cms-meteor-{}", Uuid::new_v4()));
    fs::create_dir_all(&temp)
        .map_err(|err| format!("Cannot create Meteor CMS temp folder: {err}"))?;
    extract_zip_archive(&archive, &temp)?;
    let source = extracted_content_root(&temp)?;
    copy_dir_all(&source, root, overwrite)?;
    let _ = fs::remove_dir_all(&temp);
    Ok(())
}

fn bundled_cms_archive(name: &str) -> AppResult<PathBuf> {
    let exe_parent = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = Vec::new();
    if let Some(parent) = exe_parent {
        candidates.push(parent.join("bundled-services").join("cms").join(name));
        candidates.push(
            parent
                .join("resources")
                .join("bundled-services")
                .join("cms")
                .join(name),
        );
        candidates.push(
            parent
                .join("_up_")
                .join("bundled-services")
                .join("cms")
                .join(name),
        );
    }
    candidates.push(manifest_dir.join("bundled-services").join("cms").join(name));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("Bundled CMS archive {name} was not found."))
}

fn extract_zip_archive(archive: &Path, destination: &Path) -> AppResult<()> {
    let file = fs::File::open(archive).map_err(|err| {
        format!(
            "Cannot open bundled CMS archive {}: {err}",
            archive.display()
        )
    })?;
    let mut zip = zip::ZipArchive::new(file).map_err(|err| {
        format!(
            "Cannot read bundled CMS archive {}: {err}",
            archive.display()
        )
    })?;
    const MAX_ENTRIES: usize = 10_000;
    const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    if zip.len() > MAX_ENTRIES {
        return Err("CMS archive contains too many entries.".to_string());
    }
    let mut unpacked_bytes = 0_u64;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|err| format!("Cannot read CMS archive entry: {err}"))?;
        unpacked_bytes = unpacked_bytes.saturating_add(entry.size());
        if unpacked_bytes > MAX_UNPACKED_BYTES {
            return Err("CMS archive expands beyond the 2 GB safety limit.".to_string());
        }
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let target = destination.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|err| format!("Cannot create CMS folder {}: {err}", target.display()))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    format!("Cannot create CMS folder {}: {err}", parent.display())
                })?;
            }
            let mut output = fs::File::create(&target)
                .map_err(|err| format!("Cannot create CMS file {}: {err}", target.display()))?;
            std::io::copy(&mut entry, &mut output)
                .map_err(|err| format!("Cannot extract CMS file {}: {err}", target.display()))?;
        }
    }
    Ok(())
}

fn ensure_meteor_runtime() -> AppResult<()> {
    let meteor_home = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join(".meteor"))
        .map_err(|err| format!("Cannot resolve LOCALAPPDATA for Meteor: {err}"))?;
    if !meteor_home.join("meteor.bat").is_file() {
        let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
        let output = Command::new(npx)
            .arg("meteor")
            .env("TOOL_NODE_FLAGS", "--max-old-space-size=8192")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|err| format!("Cannot install Meteor runtime with npx: {err}"))?;
        if !output.status.success() {
            let detail = format!(
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return Err(format!("Meteor runtime installation failed. {detail}"));
        }
    }
    patch_meteor_tool(&meteor_home)
}

fn prepare_meteor_project(root: &Path) -> AppResult<()> {
    let packages_path = root.join(".meteor").join("packages");
    if packages_path.is_file() {
        let packages = fs::read_to_string(&packages_path)
            .map_err(|err| format!("Cannot read Meteor packages file: {err}"))?;
        let mut lines = packages
            .lines()
            .filter(|line| !matches!(line.trim(), "rspack" | "hot-module-replacement"))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !lines.iter().any(|line| line.trim() == "jquery") {
            if let Some(position) = lines
                .iter()
                .position(|line| line.trim() == "blaze-html-templates")
            {
                lines.insert(position, "jquery".to_string());
            } else {
                lines.push("jquery".to_string());
            }
        }
        let cleaned = lines.join("\n");
        fs::write(&packages_path, format!("{cleaned}\n"))
            .map_err(|err| format!("Cannot write Meteor packages file: {err}"))?;
    }
    let package_path = root.join("package.json");
    let text = fs::read_to_string(&package_path)
        .map_err(|err| format!("Cannot read Meteor package.json: {err}"))?;
    let mut package: serde_json::Value = serde_json::from_str(&text)
        .map_err(|err| format!("Cannot parse Meteor package.json: {err}"))?;
    if let Some(meteor) = package
        .get_mut("meteor")
        .and_then(serde_json::Value::as_object_mut)
    {
        meteor.remove("mainModule");
    }
    remove_json_dependency(&mut package, "dependencies", "@swc/helpers");
    remove_json_dependency(&mut package, "devDependencies", "@meteorjs/rspack");
    remove_json_dependency(&mut package, "devDependencies", "@rsdoctor/rspack-plugin");
    remove_json_dependency(&mut package, "devDependencies", "@rspack/cli");
    remove_json_dependency(&mut package, "devDependencies", "@rspack/core");
    ensure_json_dependency(&mut package, "dependencies", "jquery", "^3.7.1");
    let formatted = serde_json::to_string_pretty(&package)
        .map_err(|err| format!("Cannot serialize Meteor package.json: {err}"))?;
    fs::write(package_path, format!("{formatted}\n"))
        .map_err(|err| format!("Cannot write Meteor package.json: {err}"))
}

fn remove_json_dependency(package: &mut serde_json::Value, section: &str, name: &str) {
    if let Some(map) = package
        .get_mut(section)
        .and_then(serde_json::Value::as_object_mut)
    {
        map.remove(name);
    }
}

fn ensure_json_dependency(
    package: &mut serde_json::Value,
    section: &str,
    name: &str,
    version: &str,
) {
    if package.get(section).is_none() {
        package[section] = serde_json::json!({});
    }
    if let Some(map) = package
        .get_mut(section)
        .and_then(serde_json::Value::as_object_mut)
    {
        map.insert(name.to_string(), serde_json::json!(version));
    }
}

fn cleanup_meteor_project_after_host_save(root: &Path) -> AppResult<()> {
    for relative in ["index.html", "rspack.config.js"] {
        let path = root.join(relative);
        if path.is_file() {
            fs::remove_file(&path).map_err(|err| {
                format!(
                    "Cannot remove Meteor-incompatible file {}: {err}",
                    path.display()
                )
            })?;
        }
    }
    for relative in ["_build", ".rsdoctor"] {
        let path = root.join(relative);
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|err| {
                format!(
                    "Cannot remove Meteor build folder {}: {err}",
                    path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn patch_meteor_tool(meteor_home: &Path) -> AppResult<()> {
    let tool_root = meteor_home.join("packages").join("meteor-tool");
    if !tool_root.is_dir() {
        return Ok(());
    }
    patch_meteor_tool_dir(&tool_root)
}

fn patch_meteor_tool_dir(path: &Path) -> AppResult<()> {
    for entry in
        fs::read_dir(path).map_err(|err| format!("Cannot read Meteor tool folder: {err}"))?
    {
        let entry = entry.map_err(|err| format!("Cannot read Meteor tool entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            patch_meteor_tool_dir(&path)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("js") {
            patch_meteor_tool_file(&path)?;
        }
    }
    Ok(())
}

fn patch_meteor_tool_file(path: &Path) -> AppResult<()> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("Cannot read Meteor tool file {}: {err}", path.display()))?;
    let patched = text
        .replace(
            "LRUCache = v;",
            "LRUCache = v && (v.default || v.LRUCache || v);",
        )
        .replace(
            "sourcemapConsumer.destroy();",
            "if (typeof sourcemapConsumer.destroy === \"function\") sourcemapConsumer.destroy();",
        );
    if patched != text {
        fs::write(path, patched)
            .map_err(|err| format!("Cannot patch Meteor tool file {}: {err}", path.display()))?;
    }
    Ok(())
}

fn allocate_node_port(snapshot: &crate::state::AppSnapshot, domain: &str) -> u16 {
    if let Some(existing) = snapshot
        .hosts
        .iter()
        .find(|host| host.domain.eq_ignore_ascii_case(domain))
        .and_then(|host| host.env_variables.get("LOCALSTACK_NODE_PORT"))
        .and_then(|value| value.parse::<u16>().ok())
    {
        return existing;
    }
    let used = snapshot
        .hosts
        .iter()
        .filter_map(|host| host.env_variables.get("LOCALSTACK_NODE_PORT"))
        .filter_map(|value| value.parse::<u16>().ok())
        .collect::<Vec<_>>();
    (3100..3400)
        .find(|port| !used.contains(port))
        .unwrap_or(3100)
}

fn ensure_node_runtime(snapshot: &crate::state::AppSnapshot) -> AppResult<()> {
    let has_node = snapshot
        .services
        .iter()
        .find(|service| service.id == "node-proxy")
        .is_some_and(|service| Path::new(&service.executable_path).exists());
    if has_node {
        return Ok(());
    }
    crate::dependencies::install_service_dependency("node-proxy".to_string())?;
    Ok(())
}

fn npm_install(root: &Path) -> AppResult<()> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = Command::new(npm)
        .arg("install")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| format!("Cannot run npm install: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .lines()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ");
        Err(format!("npm install failed. {detail}"))
    }
}

fn write_node_template_files(
    template_id: &str,
    template_name: &str,
    root: &Path,
    request: &CmsInstallRequest,
) -> AppResult<()> {
    match template_id {
        "nextjs" => write_nextjs_project(template_name, root, request),
        "node-express" => write_express_project(template_name, root, request),
        "vite-react" => write_vite_react_project(template_name, root, request),
        _ => Ok(()),
    }
}

fn write_nextjs_project(
    template_name: &str,
    root: &Path,
    request: &CmsInstallRequest,
) -> AppResult<()> {
    fs::create_dir_all(root.join("app"))
        .map_err(|err| format!("Cannot create Next.js app folder: {err}"))?;
    write_text(
        root.join("package.json"),
        &format!(
            r#"{{
  "name": "{}",
  "private": true,
  "scripts": {{
    "dev": "next dev --hostname 127.0.0.1",
    "build": "next build",
    "start": "next start --hostname 127.0.0.1"
  }},
  "dependencies": {{
    "next": "latest",
    "react": "latest",
    "react-dom": "latest"
  }},
  "devDependencies": {{
    "typescript": "latest",
    "@types/node": "latest",
    "@types/react": "latest",
    "@types/react-dom": "latest"
  }}
}}
"#,
            npm_package_name(&request.domain)
        ),
    )?;
    write_text(
        root.join("app").join("layout.tsx"),
        r#"import type { Metadata } from "next";
import "./style.css";

export const metadata: Metadata = {
  title: "LocalStack Pro Next.js",
  description: "Next.js app installed by LocalStack Pro"
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html lang="en"><body>{children}</body></html>;
}
"#,
    )?;
    write_text(
        root.join("app").join("page.tsx"),
        &node_page_markup(template_name, &request.domain),
    )?;
    write_text(root.join("app").join("style.css"), node_page_css())?;
    write_text(
        root.join("tsconfig.json"),
        r#"{"compilerOptions":{"target":"es5","lib":["dom","dom.iterable","esnext"],"allowJs":true,"skipLibCheck":true,"strict":true,"noEmit":true,"esModuleInterop":true,"module":"esnext","moduleResolution":"bundler","resolveJsonModule":true,"isolatedModules":true,"jsx":"preserve","incremental":true,"plugins":[{"name":"next"}]},"include":["next-env.d.ts","**/*.ts","**/*.tsx",".next/types/**/*.ts"],"exclude":["node_modules"]}"#,
    )
}

fn write_express_project(
    template_name: &str,
    root: &Path,
    request: &CmsInstallRequest,
) -> AppResult<()> {
    fs::create_dir_all(root.join("public"))
        .map_err(|err| format!("Cannot create public folder: {err}"))?;
    write_text(
        root.join("package.json"),
        &format!(
            r#"{{
  "name": "{}",
  "private": true,
  "type": "module",
  "scripts": {{
    "dev": "node server.js",
    "start": "node server.js"
  }},
  "dependencies": {{
    "express": "latest"
  }}
}}
"#,
            npm_package_name(&request.domain)
        ),
    )?;
    write_text(
        root.join("server.js"),
        &format!(
            r#"import express from "express";
import path from "node:path";
import {{ fileURLToPath }} from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const app = express();
const port = Number(process.env.PORT || process.env.LOCALSTACK_NODE_PORT || 3100);

app.use(express.static(path.join(__dirname, "public")));
app.get("/api/health", (_req, res) => res.json({{ ok: true, app: "{}" }}));
app.use((_req, res) => res.sendFile(path.join(__dirname, "public", "index.html")));

app.listen(port, "127.0.0.1", () => {{
  console.log(`{} listening on http://127.0.0.1:${{port}}`);
}});
"#,
            js_escape(template_name),
            js_escape(template_name)
        ),
    )?;
    write_text(
        root.join("public").join("index.html"),
        &static_node_html(template_name, &request.domain),
    )
}

fn write_vite_react_project(
    template_name: &str,
    root: &Path,
    request: &CmsInstallRequest,
) -> AppResult<()> {
    fs::create_dir_all(root.join("src"))
        .map_err(|err| format!("Cannot create Vite src folder: {err}"))?;
    write_text(
        root.join("package.json"),
        &format!(
            r#"{{
  "name": "{}",
  "private": true,
  "type": "module",
  "scripts": {{
    "dev": "vite --host 127.0.0.1",
    "build": "vite build",
    "preview": "vite preview --host 127.0.0.1"
  }},
  "dependencies": {{
    "@vitejs/plugin-react": "latest",
    "vite": "latest",
    "typescript": "latest",
    "react": "latest",
    "react-dom": "latest"
  }},
  "devDependencies": {{}}
}}
"#,
            npm_package_name(&request.domain)
        ),
    )?;
    write_text(
        root.join("index.html"),
        r#"<div id="root"></div><script type="module" src="/src/main.jsx"></script>"#,
    )?;
    write_text(
        root.join("vite.config.js"),
        r#"import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    allowedHosts: true
  }
});
"#,
    )?;
    write_text(
        root.join("src").join("main.jsx"),
        &format!(
            r#"import React from "react";
import {{ createRoot }} from "react-dom/client";
import "./style.css";

function App() {{
  return (
    <main className="shell">
      <p>LocalStack Pro</p>
      <h1>{}</h1>
      <span>{}</span>
    </main>
  );
}}

createRoot(document.getElementById("root")).render(<App />);
"#,
            html_escape(template_name),
            html_escape(&request.domain)
        ),
    )?;
    write_text(root.join("src").join("style.css"), node_page_css())
}

fn write_text(path: PathBuf, content: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Cannot create {}: {err}", parent.display()))?;
    }
    fs::write(&path, content).map_err(|err| format!("Cannot write {}: {err}", path.display()))
}

fn npm_package_name(domain: &str) -> String {
    domain
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn node_page_markup(name: &str, domain: &str) -> String {
    format!(
        r#"export default function Page() {{
  return <main className="shell"><p>LocalStack Pro</p><h1>{}</h1><span>{}</span></main>;
}}
"#,
        html_escape(name),
        html_escape(domain)
    )
}

fn static_node_html(name: &str, domain: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{}</title><link rel="stylesheet" href="/style.css"></head><body><main class="shell"><p>LocalStack Pro</p><h1>{}</h1><span>{}</span></main></body></html>"#,
        html_escape(name),
        html_escape(name),
        html_escape(domain)
    )
}

fn node_page_css() -> &'static str {
    r#"html,body{margin:0;min-height:100%;font-family:Segoe UI,Arial,sans-serif;background:#f7f8fb;color:#111827}.shell{min-height:100vh;display:grid;place-content:center;gap:10px;text-align:center}.shell p{margin:0;color:#1463df;font-weight:700}.shell h1{margin:0;font-size:42px;letter-spacing:0}.shell span{color:#5f6b7a}"#
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn js_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn validate_request(request: &CmsInstallRequest) -> AppResult<()> {
    if !request.domain.contains('.') || request.domain.contains(' ') {
        return Err("Domain must be a valid local domain like cms.test.".to_string());
    }
    if request.root_folder.trim().is_empty() {
        return Err("Project folder is required.".to_string());
    }
    if request.php_version.trim().is_empty() {
        return Err("PHP version is required.".to_string());
    }
    if request.create_database {
        if let Some(name) = &request.database_name {
            if !name.trim().is_empty() && !is_database_token(name.trim()) {
                return Err(
                    "Database name can contain only letters, numbers and underscores.".to_string(),
                );
            }
        }
        if let Some(user) = &request.database_user {
            if !user.trim().is_empty() && !is_database_token(user.trim()) {
                return Err(
                    "Database user can contain only letters, numbers and underscores.".to_string(),
                );
            }
        }
        if let Some(password) = &request.database_password {
            if !password.is_empty() && password.len() < 8 {
                return Err("Database password must be at least 8 characters.".to_string());
            }
        }
    }
    if !["Apache", "Nginx"]
        .iter()
        .any(|value| value.eq_ignore_ascii_case(&request.web_server))
    {
        return Err("Web server must be Apache or Nginx.".to_string());
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct CmsDatabaseCredentials {
    name: String,
    user: String,
    password: String,
    engine: String,
    port: u16,
}

fn ensure_database(
    snapshot: &crate::state::AppSnapshot,
    request: &CmsInstallRequest,
) -> AppResult<CmsDatabaseCredentials> {
    let domain_name = request.domain.split('.').next().unwrap_or("cms");
    let db_name = request
        .database_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| sanitize_database_name(domain_name));
    if let Some(existing) = snapshot.databases.iter().find(|database| {
        database.name.eq_ignore_ascii_case(&db_name) || database.id.eq_ignore_ascii_case(&db_name)
    }) {
        let user = request
            .database_user
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| existing.user.clone());
        let password = request
            .database_password
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or(&existing.password)
            .to_string();
        let mut database = existing.clone();
        database.user = user.clone();
        database.password = password.clone();
        database::ensure_database_access(&database).map_err(|err| {
            format!(
                "Cannot update CMS database access for {}. Start {} and check admin credentials first. {err}",
                database.name, database.engine
            )
        })?;
        persist_database_credentials(&database)?;
        return Ok(CmsDatabaseCredentials {
            name: database.name,
            user,
            password,
            engine: database.engine,
            port: database.port,
        });
    }
    let engine = if request.database_engine.trim().is_empty() {
        "MySQL"
    } else {
        request.database_engine.trim()
    };
    let port = match engine {
        "PostgreSQL" => 5432,
        "MariaDB" => 3307,
        _ => 3306,
    };
    let user = request
        .database_user
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}_user", db_name));
    let password = request
        .database_password
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            let token = Uuid::new_v4().simple().to_string();
            format!("lsp_{}", &token[..12])
        });
    let database = DatabaseInfo {
        id: db_name.clone(),
        name: db_name.clone(),
        description: format!("CMS database for {}", request.domain),
        engine: engine.to_string(),
        version: String::new(),
        schemas: 0,
        user: user.clone(),
        password: password.clone(),
        port,
        status: ServiceStatus::Stopped,
        size_mb: 0.0,
        created_at: Utc::now().to_rfc3339(),
    };
    database::create_database(database).map_err(|err| {
        format!(
            "Cannot create CMS database. Start {} and check admin credentials first. {err}",
            engine
        )
    })?;
    Ok(CmsDatabaseCredentials {
        name: db_name.clone(),
        user,
        password,
        engine: engine.to_string(),
        port,
    })
}

fn persist_database_credentials(database: &DatabaseInfo) -> AppResult<()> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if let Some(existing) = snapshot
        .databases
        .iter_mut()
        .find(|item| item.id == database.id || item.name == database.name)
    {
        existing.user = database.user.clone();
        existing.password = database.password.clone();
        existing.engine = database.engine.clone();
        existing.port = database.port;
        store.save(&snapshot)?;
    }
    Ok(())
}

fn sanitize_database_name(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if cleaned.is_empty() {
        "cms_site".to_string()
    } else {
        cleaned
    }
}

fn is_database_token(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn resolve_latest_installer_url(template: &CmsTemplate) -> AppResult<String> {
    if template.id != "joomla" {
        return Ok(template.download_url.clone());
    }

    let script = r#"
$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12
$page = Invoke-WebRequest -UseBasicParsing -Uri 'https://downloads.joomla.org/latest'
$link = $page.Links |
  Where-Object { $_.href -match '^/cms/joomla\d+/[^/]+/Joomla_[^/]+-Stable-Full_Package\.zip\?format=zip$' } |
  Select-Object -First 1 -ExpandProperty href
if ([string]::IsNullOrWhiteSpace($link)) { throw 'The official Joomla latest package link was not found.' }
[Uri]::new([Uri]'https://downloads.joomla.org', $link).AbsoluteUri
"#;
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
        .map_err(|err| format!("Cannot check the latest Joomla installer: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "Cannot check the latest Joomla installer: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    ensure_allowed_cms_download_url(&url)?;
    if !url.ends_with("format=zip") {
        return Err("The resolved Joomla installer is not a ZIP package.".to_string());
    }
    Ok(url)
}

fn download_and_extract(url: &str, archive: &Path, extracted: &Path) -> AppResult<()> {
    ensure_allowed_cms_download_url(url)?;
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}; Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        powershell_quote(url),
        powershell_quote(&archive.display().to_string()),
        powershell_quote(&archive.display().to_string()),
        powershell_quote(&extracted.display().to_string())
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
        .stdin(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot start CMS downloader: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot download or extract CMS package. {detail}"));
    }
    Ok(())
}

fn ensure_allowed_cms_download_url(url: &str) -> AppResult<()> {
    ensure_https_url_host(
        url,
        &[
            "wordpress.org",
            "downloads.joomla.org",
            "www.drupal.org",
            "getgrav.org",
        ],
    )
}

fn ensure_https_url_host(url: &str, allowed_hosts: &[&str]) -> AppResult<()> {
    let lower = url.trim().to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return Err("Download URL must use HTTPS.".to_string());
    }
    let host = lower
        .trim_start_matches("https://")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    if allowed_hosts.contains(&host) {
        Ok(())
    } else {
        Err(format!("Download host is not allowed: {host}"))
    }
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn extracted_content_root(extracted: &Path) -> AppResult<PathBuf> {
    let entries = fs::read_dir(extracted)
        .map_err(|err| format!("Cannot read extracted CMS package: {err}"))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    let directories = entries
        .iter()
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    let files = entries
        .iter()
        .filter(|entry| entry.path().is_file())
        .count();
    if files == 0 && directories.len() == 1 {
        Ok(directories[0].path())
    } else {
        Ok(extracted.to_path_buf())
    }
}

fn copy_dir_all(source: &Path, target: &Path, overwrite: bool) -> AppResult<()> {
    fs::create_dir_all(target).map_err(|err| format!("Cannot create target folder: {err}"))?;
    for entry in fs::read_dir(source).map_err(|err| format!("Cannot read package folder: {err}"))? {
        let entry = entry.map_err(|err| format!("Cannot read package entry: {err}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &target_path, overwrite)?;
        } else if overwrite || !target_path.exists() {
            fs::copy(&source_path, &target_path)
                .map_err(|err| format!("Cannot copy CMS file {}: {err}", target_path.display()))?;
        }
    }
    Ok(())
}

fn directory_is_empty(path: &Path) -> AppResult<bool> {
    let mut entries = fs::read_dir(path).map_err(|err| format!("Cannot read folder: {err}"))?;
    Ok(entries.next().is_none())
}

fn cms_files_match(template_id: &str, public: &Path) -> bool {
    match template_id {
        "wordpress" => {
            public.join("wp-admin").is_dir()
                && public.join("wp-includes").is_dir()
                && public.join("index.php").is_file()
        }
        "joomla" => {
            public.join("administrator").is_dir()
                && public.join("index.php").is_file()
                && (public.join("installation").is_dir()
                    || public.join("configuration.php").is_file())
        }
        "drupal" => {
            public.join("core").is_dir()
                && public.join("sites").is_dir()
                && public.join("index.php").is_file()
        }
        "grav" => {
            public.join("system").is_dir()
                && public.join("user").is_dir()
                && public.join("index.php").is_file()
        }
        _ => false,
    }
}

fn validate_installed_cms(
    template: &CmsTemplate,
    public: &Path,
    database: Option<&CmsDatabaseCredentials>,
) -> AppResult<()> {
    let mut issues = Vec::new();
    if !public.join("index.php").is_file() {
        issues.push(format!("{} is missing", public.join("index.php").display()));
    }
    if !cms_files_match(&template.id, public) {
        issues.push(format!(
            "{} does not look like a valid {} document root",
            public.display(),
            template.name
        ));
    }
    if template.id == "wordpress" {
        if !public.join("wp-config.php").is_file() {
            issues.push("wp-config.php is missing".to_string());
        }
        if database.is_some() {
            let config = fs::read_to_string(public.join("wp-config.php")).unwrap_or_default();
            for placeholder in ["database_name_here", "username_here", "password_here"] {
                if config.contains(placeholder) {
                    issues.push(format!("WordPress config still contains {placeholder}"));
                }
            }
        }
    } else if template.id == "joomla" && database.is_some() {
        if !public.join("installation").is_dir() && !public.join("configuration.php").is_file() {
            issues.push("Joomla installer or configuration.php is missing".to_string());
        }
    } else if template.id == "drupal"
        && database.is_some()
        && !public
            .join("sites")
            .join("default")
            .join("settings.php")
            .is_file()
    {
        issues.push("sites/default/settings.php is missing".to_string());
    }
    if issues.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} post-install validation failed: {}",
            template.name,
            issues.join("; ")
        ))
    }
}

fn write_cms_config(
    template: &CmsTemplate,
    public: &Path,
    database: Option<&CmsDatabaseCredentials>,
    request: &CmsInstallRequest,
) -> AppResult<()> {
    match template.id.as_str() {
        "wordpress" => write_wordpress_config(public, database, request),
        "joomla" => write_joomla_install_helper(public, database),
        "drupal" => write_drupal_config(public, database, request),
        _ => Ok(()),
    }
}

fn write_wordpress_config(
    public: &Path,
    database: Option<&CmsDatabaseCredentials>,
    _request: &CmsInstallRequest,
) -> AppResult<()> {
    let Some(database) = database else {
        return Ok(());
    };
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
        .replace("localhost", database_host(&database.engine));
    config = set_wordpress_define(&config, "DB_NAME", &database.name);
    config = set_wordpress_define(&config, "DB_USER", &database.user);
    config = set_wordpress_define(&config, "DB_PASSWORD", &database.password);
    config = set_wordpress_define(&config, "DB_HOST", database_host(&database.engine));
    config = ensure_wordpress_direct_filesystem(config);
    for _ in 0..8 {
        config = config.replacen(
            "put your unique phrase here",
            &Uuid::new_v4().to_string(),
            1,
        );
    }
    fs::write(target, config).map_err(|err| format!("Cannot write WordPress config: {err}"))
}

fn write_joomla_install_helper(
    public: &Path,
    database: Option<&CmsDatabaseCredentials>,
) -> AppResult<()> {
    let Some(database) = database else {
        return Ok(());
    };
    fs::create_dir_all(public.join("administrator").join("logs"))
        .map_err(|err| format!("Cannot create Joomla log folder: {err}"))?;
    fs::create_dir_all(public.join("tmp"))
        .map_err(|err| format!("Cannot create Joomla temp folder: {err}"))?;
    let content = format!(
        "<?php\nreturn [\n  'host' => '{}',\n  'database' => '{}',\n  'username' => '{}',\n  'password' => '{}',\n  'port' => '{}',\n  'prefix' => 'jos_',\n];\n",
        php_escape(database_host(&database.engine)),
        php_escape(&database.name),
        php_escape(&database.user),
        php_escape(&database.password),
        database.port
    );
    fs::write(public.join("localstack-database.php"), content)
        .map_err(|err| format!("Cannot write Joomla database helper: {err}"))
}

fn write_drupal_config(
    public: &Path,
    database: Option<&CmsDatabaseCredentials>,
    _request: &CmsInstallRequest,
) -> AppResult<()> {
    let Some(database) = database else {
        return Ok(());
    };
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
        php_escape(database_host_only(&database.engine)),
        database.port,
        php_escape(drupal_driver(&database.engine)),
        php_escape(drupal_driver(&database.engine)),
        php_escape(&Uuid::new_v4().to_string())
    ));
    fs::write(target, config).map_err(|err| format!("Cannot write Drupal settings.php: {err}"))
}

fn remove_localstack_placeholder_index(public: &Path) -> AppResult<()> {
    let html = public.join("index.html");
    if public.join("index.php").is_file() && html.is_file() {
        let text = fs::read_to_string(&html).unwrap_or_default();
        if text.contains("LocalStack Pro host is ready.") {
            fs::remove_file(&html)
                .map_err(|err| format!("Cannot remove LocalStack placeholder index.html: {err}"))?;
        }
    }
    Ok(())
}

fn set_wordpress_define(config: &str, key: &str, value: &str) -> String {
    let needle = format!("define( '{key}'");
    let compact_needle = format!("define('{key}'");
    config
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&needle) || trimmed.starts_with(&compact_needle) {
                format!("define( '{key}', '{}' );", value.replace('\'', "\\'"))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn ensure_wordpress_direct_filesystem(config: String) -> String {
    let updated = set_wordpress_define(&config, "FS_METHOD", "direct");
    if updated != config {
        return updated;
    }

    let define = "define( 'FS_METHOD', 'direct' );\n";
    if let Some(index) = config.find("/* That's all, stop editing!") {
        format!("{}{}{}", &config[..index], define, &config[index..])
    } else {
        format!("{config}\n{define}")
    }
}

fn database_host(engine: &str) -> &'static str {
    match engine {
        "MariaDB" => "127.0.0.1:3307",
        "PostgreSQL" => "127.0.0.1:5432",
        _ => "127.0.0.1:3306",
    }
}

fn database_host_only(_engine: &str) -> &'static str {
    "127.0.0.1"
}

fn drupal_driver(engine: &str) -> &'static str {
    match engine {
        "PostgreSQL" => "pgsql",
        _ => "mysql",
    }
}

fn php_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn write_install_metadata(
    root: &Path,
    template: &CmsTemplate,
    request: &CmsInstallRequest,
    database: Option<&str>,
) -> AppResult<()> {
    let content = serde_json::json!({
        "templateId": template.id,
        "name": template.name,
        "domain": request.domain,
        "documentRoot": template.document_root,
        "database": database,
        "installedAt": Utc::now().to_rfc3339()
    });
    let text = serde_json::to_string_pretty(&content)
        .map_err(|err| format!("Cannot serialize CMS metadata: {err}"))?;
    fs::write(root.join("localstack-cms.json"), text)
        .map_err(|err| format!("Cannot write CMS metadata: {err}"))
}

#[cfg(test)]
mod tests {
    use super::{capture_version, version_is_newer};

    #[test]
    fn detects_only_newer_versions() {
        assert!(version_is_newer("6.1.2", "6.1.1"));
        assert!(version_is_newer("6.2.0", "6.1.99"));
        assert!(!version_is_newer("6.1.2", "6.1.2"));
        assert!(!version_is_newer("6.1.1", "6.1.2"));
    }

    #[test]
    fn reads_quoted_versions() {
        assert_eq!(
            capture_version("$wp_version = '6.8.1';", "$wp_version = '"),
            Some("6.8.1".to_string())
        );
    }
}
