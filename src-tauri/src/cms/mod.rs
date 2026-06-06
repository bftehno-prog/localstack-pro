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

pub fn cms_templates() -> Vec<CmsTemplate> {
    vec![
        template(
            "nextjs",
            "Next.js",
            "React full-stack application with App Router and local dev server.",
            "Node.js",
            "localstack://node/nextjs",
            ".",
            false,
            "MySQL",
        ),
        template(
            "node-express",
            "Node.js Express",
            "Minimal Express application with local API routes.",
            "Node.js",
            "localstack://node/express",
            ".",
            false,
            "MySQL",
        ),
        template(
            "vite-react",
            "Vite React",
            "Fast React single-page application powered by Vite.",
            "Node.js",
            "localstack://node/vite-react",
            ".",
            false,
            "MySQL",
        ),
        template(
            "wordpress",
            "WordPress",
            "Classic PHP CMS for blogs, shops and company sites.",
            "CMS",
            "https://wordpress.org/latest.zip",
            "public",
            true,
            "MySQL",
        ),
        template(
            "joomla",
            "Joomla",
            "Full package from the official Joomla latest-release channel.",
            "CMS",
            "https://downloads.joomla.org/cms/joomla6/6-1-0/Joomla_6-1-0-Stable-Full_Package.zip?format=zip",
            "public",
            true,
            "MySQL",
        ),
        template(
            "drupal",
            "Drupal",
            "Latest recommended Drupal core ZIP from Drupal.org.",
            "CMS",
            "https://www.drupal.org/download-latest/zip",
            "public",
            true,
            "MySQL",
        ),
        template(
            "grav",
            "Grav",
            "Fast flat-file CMS, no database required.",
            "Flat-file",
            "https://getgrav.org/download/core/grav/latest",
            "public",
            false,
            "MySQL",
        ),
    ]
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
    if !use_existing_files {
        let archive = temp.join("package.zip");
        let extracted = temp.join("extract");
        fs::create_dir_all(&extracted)
            .map_err(|err| format!("Cannot create temp folder: {err}"))?;
        download_and_extract(&template.download_url, &archive, &extracted)?;
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
            format!("{} installed at {}", template.name, request.domain)
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

fn template(
    id: &str,
    name: &str,
    description: &str,
    category: &str,
    download_url: &str,
    document_root: &str,
    requires_database: bool,
    default_database_engine: &str,
) -> CmsTemplate {
    CmsTemplate {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        category: category.to_string(),
        download_url: download_url.to_string(),
        document_root: document_root.to_string(),
        requires_database,
        default_database_engine: default_database_engine.to_string(),
    }
}

fn is_node_template(template_id: &str) -> bool {
    matches!(template_id, "nextjs" | "node-express" | "vite-react")
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
        write_node_template_files(&template.id, &template.name, &root, &request)?;
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
    env.insert("LOCALSTACK_NODE_KIND".to_string(), template.id.clone());

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
        tags: vec!["node".to_string(), template.id.clone()],
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
            format!(
                "lsp_{}",
                Uuid::new_v4().simple().to_string()[..12].to_string()
            )
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

fn download_and_extract(url: &str, archive: &Path, extracted: &Path) -> AppResult<()> {
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
        if !public.join("configuration.php").is_file() {
            issues.push("configuration.php is missing".to_string());
        }
    } else if template.id == "drupal" && database.is_some() {
        if !public
            .join("sites")
            .join("default")
            .join("settings.php")
            .is_file()
        {
            issues.push("sites/default/settings.php is missing".to_string());
        }
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
        "joomla" => write_joomla_config(public, database, request),
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
    for _ in 0..8 {
        config = config.replacen(
            "put your unique phrase here",
            &Uuid::new_v4().to_string(),
            1,
        );
    }
    fs::write(target, config).map_err(|err| format!("Cannot write WordPress config: {err}"))
}

fn write_joomla_config(
    public: &Path,
    database: Option<&CmsDatabaseCredentials>,
    request: &CmsInstallRequest,
) -> AppResult<()> {
    let Some(database) = database else {
        return Ok(());
    };
    let log_path = public.join("administrator").join("logs");
    let tmp_path = public.join("tmp");
    fs::create_dir_all(&log_path)
        .map_err(|err| format!("Cannot create Joomla log folder: {err}"))?;
    fs::create_dir_all(&tmp_path)
        .map_err(|err| format!("Cannot create Joomla temp folder: {err}"))?;
    let content = format!(
        "<?php\nclass JConfig {{\n\tpublic $offline = false;\n\tpublic $sitename = '{}';\n\tpublic $editor = 'tinymce';\n\tpublic $captcha = '0';\n\tpublic $list_limit = 20;\n\tpublic $access = 1;\n\tpublic $debug = false;\n\tpublic $debug_lang = false;\n\tpublic $dbtype = '{}';\n\tpublic $host = '{}';\n\tpublic $user = '{}';\n\tpublic $password = '{}';\n\tpublic $db = '{}';\n\tpublic $dbprefix = 'lsp_';\n\tpublic $live_site = '{}';\n\tpublic $secret = '{}';\n\tpublic $gzip = false;\n\tpublic $error_reporting = 'default';\n\tpublic $helpurl = 'https://help.joomla.org/proxy?keyref=Help{{major}}{{minor}}:{{keyref}}';\n\tpublic $ftp_enable = false;\n\tpublic $offset = 'UTC';\n\tpublic $mailonline = true;\n\tpublic $mailer = 'mail';\n\tpublic $caching = 0;\n\tpublic $cache_handler = 'file';\n\tpublic $cachetime = 15;\n\tpublic $MetaDesc = '';\n\tpublic $MetaKeys = '';\n\tpublic $MetaTitle = true;\n\tpublic $MetaAuthor = true;\n\tpublic $sef = true;\n\tpublic $sef_rewrite = false;\n\tpublic $sef_suffix = false;\n\tpublic $unicodeslugs = false;\n\tpublic $feed_limit = 10;\n\tpublic $log_path = '{}';\n\tpublic $tmp_path = '{}';\n\tpublic $session_handler = 'database';\n}}\n",
        php_escape(&request.domain),
        php_escape(joomla_db_type(&database.engine)),
        php_escape(database_host(&database.engine)),
        php_escape(&database.user),
        php_escape(&database.password),
        php_escape(&database.name),
        php_escape(&format!("{}://{}", if request.ssl { "https" } else { "http" }, request.domain)),
        php_escape(&Uuid::new_v4().simple().to_string()),
        php_escape(&log_path.display().to_string()),
        php_escape(&tmp_path.display().to_string())
    );
    fs::write(public.join("configuration.php"), content)
        .map_err(|err| format!("Cannot write Joomla configuration.php: {err}"))
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

fn joomla_db_type(engine: &str) -> &'static str {
    match engine {
        "PostgreSQL" => "pgsql",
        _ => "mysqli",
    }
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
