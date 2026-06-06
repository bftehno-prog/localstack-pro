use crate::state::{AppResult, AppSettings, LogLevel, Store};
use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn save_settings(settings: AppSettings) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let startup_changed = snapshot.settings.launch_on_startup != settings.launch_on_startup;
    let startup_enabled = settings.launch_on_startup;
    snapshot.settings = settings;
    if startup_changed {
        apply_startup(startup_enabled)?;
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Settings",
        "Settings saved",
        None,
    );
    store.save(&snapshot)?;
    crate::tray::rebuild_menu_for_settings_change();
    Ok(snapshot)
}

pub fn export_settings(path: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let target = if path.ends_with(".json") {
        let target = PathBuf::from(path);
        if target.is_absolute() {
            target
        } else {
            store.dir.join(target)
        }
    } else {
        store.dir.join("settings-export.json")
    };
    let text = serde_json::to_string_pretty(&snapshot.settings)
        .map_err(|err| format!("Cannot serialize settings: {err}"))?;
    fs::write(&target, text).map_err(|err| format!("Cannot export settings: {err}"))?;
    Ok(target.display().to_string())
}

pub fn import_settings(path: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let target = resolve_data_path(&store, &path);
    if !target.exists() {
        let mut snapshot = store.load_static()?;
        let text = serde_json::to_string_pretty(&snapshot.settings)
            .map_err(|err| format!("Cannot serialize current settings: {err}"))?;
        fs::write(&target, text)
            .map_err(|err| format!("Cannot create settings export {}: {err}", target.display()))?;
        store.log(
            &mut snapshot,
            LogLevel::Warning,
            "Settings",
            format!(
                "Settings import file was missing, so LocalStack Pro created {}",
                target.display()
            ),
            None,
        );
        store.save(&snapshot)?;
        return Ok(snapshot);
    }
    let text = fs::read_to_string(&target)
        .map_err(|err| format!("Cannot read settings JSON {}: {err}", target.display()))?;
    let settings: AppSettings =
        serde_json::from_str(&text).map_err(|err| format!("Cannot parse settings JSON: {err}"))?;
    let mut snapshot = store.load_static()?;
    let startup_changed = snapshot.settings.launch_on_startup != settings.launch_on_startup;
    let startup_enabled = settings.launch_on_startup;
    snapshot.settings = settings;
    if startup_changed {
        apply_startup(startup_enabled)?;
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Settings",
        format!("Settings imported from {}", target.display()),
        None,
    );
    store.save(&snapshot)?;
    crate::tray::rebuild_menu_for_settings_change();
    Ok(snapshot)
}

pub fn reset_settings() -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let old_file = store.dir.join("state.json");
    let backup = store.dir.join("state.reset-backup.json");
    let _ = fs::copy(&old_file, &backup);
    let disable_startup = snapshot.settings.launch_on_startup;
    snapshot.settings.language = "English (US)".to_string();
    snapshot.settings.preferred_browser = "Default System Browser".to_string();
    snapshot.settings.minimize_to_tray = true;
    snapshot.settings.close_to_tray = true;
    snapshot.settings.launch_on_startup = false;
    snapshot.settings.show_notifications = true;
    snapshot.settings.play_sound = false;
    snapshot.settings.check_updates_on_startup = true;
    snapshot.settings.telemetry = false;
    snapshot.settings.ui_density = "Comfortable".to_string();
    snapshot.settings.theme = "Light".to_string();
    snapshot.settings.log_level = "Information".to_string();
    snapshot.settings.max_log_file_size = "50 MB".to_string();
    snapshot.settings.retain_logs_days = 30;
    snapshot.settings.show_timestamps = true;
    if disable_startup {
        apply_startup(false)?;
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "Settings",
        "Settings reset to defaults",
        None,
    );
    store.save(&snapshot)?;
    crate::tray::rebuild_menu_for_settings_change();
    Ok(snapshot)
}

pub fn create_app_backup(path: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let target = if path.trim().ends_with(".zip") {
        let candidate = PathBuf::from(path.trim());
        if candidate.is_absolute() {
            candidate
        } else {
            store.dir.join(candidate)
        }
    } else {
        PathBuf::from(&snapshot.settings.backups_folder).join(format!(
            "localstack-pro-backup-{}.zip",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ))
    };
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Cannot create backup folder {}: {err}", parent.display()))?;
    }
    let staging = store.dir.join("temp").join(format!(
        "backup-stage-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|err| {
        format!(
            "Cannot create backup staging folder {}: {err}",
            staging.display()
        )
    })?;
    let manifest = staging.join("backup-manifest.json");
    let manifest_text = serde_json::json!({
        "name": "LocalStack Pro Backup",
        "version": env!("CARGO_PKG_VERSION"),
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "appDataDir": store.dir,
        "included": ["state.json", "hosts", "configs", "certs", "keys", "tools", "www"]
    });
    fs::write(
        &manifest,
        serde_json::to_string_pretty(&manifest_text)
            .map_err(|err| format!("Cannot serialize backup manifest: {err}"))?,
    )
    .map_err(|err| format!("Cannot write backup manifest: {err}"))?;

    copy_path_filtered(&store.dir.join("state.json"), &staging.join("state.json"))?;
    for name in ["hosts", "configs", "certs", "keys", "tools", "www"] {
        let source = store.dir.join(name);
        if source.exists() {
            copy_path_filtered(&source, &staging.join(name))?;
        }
    }
    let sources = fs::read_dir(&staging)
        .map_err(|err| format!("Cannot read backup staging folder: {err}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    compress_paths(&sources, &target)?;
    let _ = fs::remove_dir_all(&staging);
    Ok(target.display().to_string())
}

pub fn restore_app_backup(path: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let source = PathBuf::from(path.trim());
    if !source.is_file() {
        return Err(format!(
            "Backup archive was not found: {}",
            source.display()
        ));
    }
    let pre_restore = store.dir.join("backups").join(format!(
        "pre-restore-{}.zip",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let _ = create_app_backup(pre_restore.display().to_string());

    let temp = store.dir.join("temp").join(format!(
        "restore-{}",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).map_err(|err| {
        format!(
            "Cannot create restore temp folder {}: {err}",
            temp.display()
        )
    })?;
    expand_archive(&source, &temp)?;
    let root = restore_root(&temp)?;
    if !root.join("state.json").is_file() {
        return Err("Backup archive does not contain state.json.".to_string());
    }
    for name in [
        "state.json",
        "hosts",
        "configs",
        "certs",
        "keys",
        "tools",
        "www",
    ] {
        let from = root.join(name);
        if from.exists() {
            let to = store.dir.join(name);
            if to.exists() {
                if to.is_dir() {
                    fs::remove_dir_all(&to)
                        .map_err(|err| format!("Cannot replace {}: {err}", to.display()))?;
                } else {
                    fs::remove_file(&to)
                        .map_err(|err| format!("Cannot replace {}: {err}", to.display()))?;
                }
            }
            copy_path(&from, &to)?;
        }
    }
    let _ = fs::remove_dir_all(&temp);
    let snapshot = store.load_static()?;
    Ok(snapshot)
}

pub fn open_certificate_store() -> AppResult<()> {
    let mut command = Command::new("cmd");
    command
        .args(["/C", "start", "", "certmgr.msc"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .map_err(|err| format!("Cannot open Windows Certificate Store: {err}"))?;
    Ok(())
}

pub fn open_documentation() -> AppResult<()> {
    let store = Store::new()?;
    let docs = store.dir.join("documentation");
    fs::create_dir_all(&docs)
        .map_err(|err| format!("Cannot create documentation folder: {err}"))?;
    let target = docs.join("LocalStack Pro Documentation.html");
    fs::write(&target, documentation_html())
        .map_err(|err| format!("Cannot write documentation: {err}"))?;
    open::that(&target)
        .map_err(|err| format!("Cannot open documentation {}: {err}", target.display()))
}

fn documentation_html() -> String {
    r#"<!doctype html>
<html lang="ru">
<head>
  <meta charset="utf-8">
  <title>LocalStack Pro Documentation</title>
  <style>
    body{font-family:Segoe UI,Arial,sans-serif;margin:0;background:#f6f7f9;color:#1f2328}
    main{max-width:1160px;margin:0 auto;padding:28px}
    header{display:flex;align-items:end;justify-content:space-between;gap:20px;margin:0 0 18px}
    section{background:#fff;border:1px solid #e2e6eb;border-radius:8px;padding:18px;margin:0 0 12px}
    .grid{display:grid;grid-template-columns:1fr 1fr;gap:12px}
    h1{font-size:28px;margin:0} h2{font-size:18px;margin:0 0 10px} h3{font-size:15px;margin:12px 0 6px}
    p{line-height:1.45;margin:0 0 8px;color:#414955} li{margin:5px 0;line-height:1.4}
    code{background:#eef2f7;padding:2px 6px;border-radius:4px}
    .lang{font-size:12px;color:#667085;text-transform:uppercase;letter-spacing:.04em;margin-bottom:8px}
    @media(max-width:860px){main{padding:16px}.grid{grid-template-columns:1fr}header{display:block}}
  </style>
</head>
<body><main>
  <header><div><h1>LocalStack Pro Documentation</h1><p>Local virtual server and environment manager for Windows web development.</p></div><code>v1.0.0</code></header>
  <div class="grid">
    <section><div class="lang">Русский</div><h2>Быстрый старт</h2><ol><li>Откройте <b>Сервисы</b> и нажмите <b>Detect</b>, чтобы найти установленные Apache, Nginx, PHP, базы данных и утилиты.</li><li>Если чего-то нет, нажмите <b>Install Missing</b>.</li><li>Запустите нужный web server, синхронизируйте hosts-файл и откройте сайт через <b>Открыть в браузере</b>.</li></ol></section>
    <section><div class="lang">English</div><h2>Quick Start</h2><ol><li>Open <b>Services</b> and click <b>Detect</b> to find installed Apache, Nginx, PHP, database engines and tools.</li><li>If something is missing, click <b>Install Missing</b>.</li><li>Start the required web server, sync the hosts file and open the site with <b>Open in Browser</b>.</li></ol></section>
    <section><div class="lang">Русский</div><h2>Сервисы</h2><p>Apache, Nginx, PHP, MySQL, MariaDB, PostgreSQL, Redis, Mailpit, Node proxy и DNS helper запускаются, останавливаются и перезапускаются со страницы сервисов.</p><p>Если нативный бинарник отсутствует, LocalStack Pro показывает понятную ошибку и предлагает установить зависимость через winget.</p></section>
    <section><div class="lang">English</div><h2>Services</h2><p>Apache, Nginx, PHP, MySQL, MariaDB, PostgreSQL, Redis, Mailpit, Node proxy and DNS helper can be started, stopped and restarted from the Services page.</p><p>If a native binary is missing, LocalStack Pro shows a clear error and offers installation through winget.</p></section>
    <section><div class="lang">Русский</div><h2>Node.js и Next.js</h2><p>В разделе CMS доступны шаблоны Next.js, Node.js Express и Vite React. LocalStack Pro создает проект, выполняет npm install, прописывает локальный домен, запускает Node proxy и открывает сайт через обычный .test домен без ручного терминала.</p></section>
    <section><div class="lang">English</div><h2>Node.js and Next.js</h2><p>The CMS page includes Next.js, Node.js Express and Vite React templates. LocalStack Pro creates the project, runs npm install, maps the local domain, starts Node proxy and serves the app through a regular .test domain without a manual terminal.</p></section>
    <section><div class="lang">Русский</div><h2>Гайд: установка любого Node.js сайта</h2>
      <h3>1. Подготовьте проект</h3><ol><li>Убедитесь, что в проекте есть <code>package.json</code>.</li><li>Откройте проект локально в папке без кириллицы и спецсимволов, например <code>C:\Projects\my-node-site</code>.</li><li>Если проекта еще нет, создайте хост через <b>CMS → Install CMS</b> и выберите шаблон <b>Next.js</b>, <b>Node.js Express</b> или <b>Vite React</b>.</li></ol>
      <h3>2. Проверьте scripts</h3><p>LocalStack запускает приложение через Node proxy командой <code>npm run SCRIPT -- --port PORT</code>. Поэтому в <code>package.json</code> должен быть рабочий script.</p><ul><li>Next.js: <code>"start": "next start --hostname 127.0.0.1"</code> или <code>"dev": "next dev --hostname 127.0.0.1"</code>.</li><li>Vite: <code>"dev": "vite --host 127.0.0.1"</code>, а в <code>vite.config</code> разрешите локальный домен через <code>server.allowedHosts</code>.</li><li>Express/Fastify/Koa: сервер должен читать порт из <code>process.env.PORT</code> или <code>process.env.LOCALSTACK_NODE_PORT</code>.</li></ul>
      <h3>3. Установите зависимости</h3><p>Для существующего проекта откройте root folder и выполните <code>npm install</code>. Если зависимости конфликтуют, используйте <code>npm install --legacy-peer-deps</code>. Для проектов с Prisma дополнительно выполните <code>npx prisma generate</code> и миграции/seed, которые указаны в README проекта.</p>
      <h3>4. Создайте или отредактируйте host</h3><ol><li>Откройте <b>Hosts</b> и создайте домен, например <code>myapp.test</code>.</li><li>Root folder укажите на папку проекта.</li><li>Document root для Node.js обычно <code>.</code>.</li><li>Web server оставьте <b>Apache</b>, SSL включайте только если локальный CA уже trusted.</li><li>Добавьте теги <code>node</code> и тип приложения: <code>nextjs</code>, <code>vite-react</code> или <code>node-express</code>.</li></ol>
      <h3>5. Env variables для Node proxy</h3><p>В host env variables добавьте:</p><ul><li><code>LOCALSTACK_NODE_PORT=3100</code> или другой свободный порт.</li><li><code>LOCALSTACK_NODE_SCRIPT=start</code> для production или <code>dev</code> для разработки.</li><li><code>LOCALSTACK_NODE_KIND=nextjs</code>, <code>vite-react</code> или <code>node</code>.</li><li><code>APP_URL=http://myapp.test</code>.</li></ul>
      <h3>6. Синхронизация и запуск</h3><ol><li>Нажмите <b>Sync Hosts File</b> и подтвердите права администратора, если Windows спросит.</li><li>Запустите <b>Apache</b> и <b>Node.js Proxy</b> в Services.</li><li>Откройте <code>http://myapp.test</code>. Первый запрос может занять несколько секунд, потому что Node proxy запускает npm script.</li></ol>
      <h3>7. Production режим</h3><p>Для Next.js выполните <code>npm run build</code>, затем поставьте <code>LOCALSTACK_NODE_SCRIPT=start</code>. Для Vite используйте production preview только если script принимает порт, либо оставьте dev для локальной разработки.</p>
      <h3>8. Частые ошибки</h3><ul><li><b>502 Bad Gateway</b>: Node-приложение еще запускается или script упал. Проверьте Logs и запустите тот же <code>npm run ...</code> вручную один раз для диагностики.</li><li><b>404</b>: неправильно выбран root folder, приложение не имеет нужного route или Vite/Next не принимает домен.</li><li><b>ERR_CONNECTION_RESET</b>: Apache/Node proxy не запущен или порт занят.</li><li><b>Host not allowed</b>: добавьте <code>.test</code> домен в настройки dev server.</li><li><b>Prisma/database error</b>: проверьте <code>.env</code>, выполните миграции и seed.</li></ul>
    </section>
    <section><div class="lang">English</div><h2>Guide: installing any Node.js site</h2>
      <h3>1. Prepare the project</h3><ol><li>Make sure the project has a <code>package.json</code>.</li><li>Use a plain local path such as <code>C:\Projects\my-node-site</code>.</li><li>For a new project, open <b>CMS → Install CMS</b> and choose <b>Next.js</b>, <b>Node.js Express</b>, or <b>Vite React</b>.</li></ol>
      <h3>2. Check scripts</h3><p>LocalStack starts apps through Node proxy as <code>npm run SCRIPT -- --port PORT</code>, so the selected script must accept or ignore a port argument.</p><ul><li>Next.js: <code>"start": "next start --hostname 127.0.0.1"</code> or <code>"dev": "next dev --hostname 127.0.0.1"</code>.</li><li>Vite: <code>"dev": "vite --host 127.0.0.1"</code> and allow the local domain in <code>server.allowedHosts</code>.</li><li>Express/Fastify/Koa: read the port from <code>process.env.PORT</code> or <code>process.env.LOCALSTACK_NODE_PORT</code>.</li></ul>
      <h3>3. Install dependencies</h3><p>Run <code>npm install</code> in the project root. If dependencies conflict, use <code>npm install --legacy-peer-deps</code>. For Prisma projects, run <code>npx prisma generate</code> and the migrations/seed commands documented by the project.</p>
      <h3>4. Create or edit the host</h3><ol><li>Create a domain such as <code>myapp.test</code>.</li><li>Set root folder to the project folder.</li><li>For Node.js, document root is usually <code>.</code>.</li><li>Keep web server as <b>Apache</b>; enable SSL only after the local CA is trusted.</li><li>Add tags <code>node</code> and the app type: <code>nextjs</code>, <code>vite-react</code>, or <code>node-express</code>.</li></ol>
      <h3>5. Node proxy env variables</h3><ul><li><code>LOCALSTACK_NODE_PORT=3100</code> or another free port.</li><li><code>LOCALSTACK_NODE_SCRIPT=start</code> for production or <code>dev</code> for development.</li><li><code>LOCALSTACK_NODE_KIND=nextjs</code>, <code>vite-react</code>, or <code>node</code>.</li><li><code>APP_URL=http://myapp.test</code>.</li></ul>
      <h3>6. Sync and run</h3><ol><li>Click <b>Sync Hosts File</b> and approve elevation if Windows asks.</li><li>Start <b>Apache</b> and <b>Node.js Proxy</b>.</li><li>Open <code>http://myapp.test</code>. The first request can take a few seconds while npm starts.</li></ol>
      <h3>7. Production mode</h3><p>For Next.js, run <code>npm run build</code>, then set <code>LOCALSTACK_NODE_SCRIPT=start</code>. For Vite, use a preview script only if it accepts a port; otherwise keep dev mode for local work.</p>
      <h3>8. Common errors</h3><ul><li><b>502 Bad Gateway</b>: the Node app is still starting or the script crashed. Check Logs.</li><li><b>404</b>: root folder, app routes, or dev-server host allowlist is wrong.</li><li><b>ERR_CONNECTION_RESET</b>: Apache/Node proxy is stopped or the port is busy.</li><li><b>Host not allowed</b>: allow the <code>.test</code> domain in the dev server config.</li><li><b>Prisma/database error</b>: check <code>.env</code>, migrations and seed.</li></ul>
    </section>
    <section><div class="lang">Русский</div><h2>Хосты</h2><p>Хост хранит домен, корневую папку, document root, PHP-версию, web server, SSL, порты, базу данных, mail service, env-переменные и заметки.</p><p>Папки проекта и логов создаются автоматически. Синхронизация Windows hosts-файла может запросить права администратора.</p></section>
    <section><div class="lang">English</div><h2>Hosts</h2><p>A host stores domain, root folder, document root, PHP version, web server, SSL, ports, database, mail service, env variables and notes.</p><p>Project and log folders are created automatically. Windows hosts-file sync may request administrator rights.</p></section>
    <section><div class="lang">Русский</div><h2>Базы данных</h2><p>Создание базы выполняется через настоящий mysql/psql client, создает базу, пользователя и права. Export/backup использует mysqldump/pg_dump.</p><p>Если native client не найден или сервис не запущен, LocalStack Pro показывает ошибку и не создает фейковую запись.</p></section>
    <section><div class="lang">English</div><h2>Database</h2><p>Database creation runs through a real mysql/psql client and creates the database, user and privileges. Export/backup uses mysqldump/pg_dump.</p><p>If the native client is missing or the service is not running, LocalStack Pro shows an error and does not create a fake record.</p></section>
    <section><div class="lang">Русский</div><h2>SSL</h2><p>Сертификаты создаются локально в папках <code>certs</code> и <code>keys</code>. SAN-домены сохраняются вместе с сертификатом.</p><p>Trust/Repair Trust работает через Windows Certificate Store и может требовать повышенных прав.</p></section>
    <section><div class="lang">English</div><h2>SSL</h2><p>Certificates are generated locally into <code>certs</code> and <code>keys</code>. SAN domains are stored with the certificate.</p><p>Trust and Repair Trust use the Windows Certificate Store and may require elevated permissions.</p></section>
    <section><div class="lang">Русский</div><h2>Настройки</h2><p>Язык, тема, плотность интерфейса, автозапуск, поведение трея, папки, порты, уведомления, backups и logs сохраняются локально.</p><p>Настройки применяются без перезапуска там, где это возможно. Переключатель автозапуска меняет файл в Startup только при реальном изменении значения.</p></section>
    <section><div class="lang">English</div><h2>Settings</h2><p>Language, theme, UI density, startup, tray behavior, folders, ports, notifications, backups and logs are saved locally.</p><p>Settings apply without restart where possible. Startup integration changes the Windows Startup entry only when the startup value changes.</p></section>
    <section><div class="lang">Русский</div><h2>Диагностика</h2><ul><li>При ошибке соединения сначала проверьте, что Apache или Nginx запущен и домен есть в Windows hosts-файле.</li><li>Если порт занят, LocalStack Pro показывает порт и не запускает второй конфликтующий процесс.</li><li>Логи, экспорты и настройки находятся в AppData LocalStack Pro.</li></ul></section>
    <section><div class="lang">English</div><h2>Troubleshooting</h2><ul><li>If the browser shows a connection error, first check that Apache or Nginx is running and the domain exists in the Windows hosts file.</li><li>If a port is busy, LocalStack Pro reports the port and does not start a second conflicting process.</li><li>Logs, exports and settings are stored in the LocalStack Pro AppData folder.</li></ul></section>
    <section><div class="lang">Русский</div><h2>Health Check и One-click Fix</h2><p>В <b>Settings → Need Help?</b> кнопка <b>Health Check</b> проверяет папки данных, бинарники сервисов, порты, hosts-файл, vhost-конфиги, PHP, базы данных и инструменты.</p><p><b>One-click Fix</b> последовательно ищет зависимости, синхронизирует Windows hosts, чинит окружение и запускает финальную проверку. Для hosts и SSL Windows может запросить права администратора.</p></section>
    <section><div class="lang">English</div><h2>Health Check and One-click Fix</h2><p>In <b>Settings → Need Help?</b>, <b>Health Check</b> verifies data folders, service binaries, ports, the Windows hosts file, vhost configs, PHP, databases and tools.</p><p><b>One-click Fix</b> detects dependencies, syncs the Windows hosts file, repairs the environment and runs a final check. Windows may request administrator rights for hosts and SSL operations.</p></section>
  </div>
</main></body></html>"#.to_string()
}

pub fn open_path(path: String) -> AppResult<()> {
    if path.trim().is_empty() {
        return Err("Path is empty.".to_string());
    }
    let requested = PathBuf::from(path.trim());
    let target = ensure_open_target(&requested)?;
    open::that(&target).map_err(|err| format!("Cannot open path {}: {err}", target.display()))
}

pub fn open_url(url: String) -> AppResult<()> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("URL must start with http:// or https://.".to_string());
    }
    open_browser_target(&url)
}

pub fn open_host(host_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let host = snapshot
        .hosts
        .iter()
        .find(|host| host.id == host_id || host.domain == host_id)
        .cloned()
        .ok_or_else(|| "Host was not found.".to_string())?;
    let service_id = if host.web_server.eq_ignore_ascii_case("nginx") {
        "nginx"
    } else {
        "apache"
    };
    let service = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
        .ok_or_else(|| format!("{} service is not configured.", host.web_server))?;
    let service_name = service.name.clone();
    let service_status = service.status.clone();
    if service_status != crate::state::ServiceStatus::Running {
        return Err(format!(
            "{} is not running. Start {} before opening {}.",
            service_name, service_name, host.domain
        ));
    }
    let cert_ready = !host.ssl || host_certificate_files_ready(&store, &snapshot, &host.domain);
    let candidates = host_open_candidates(&host, cert_ready);
    if crate::hosts::hosts_file_maps_domain(&host.domain)
        && !web_runtime_needs_refresh(&store, service_id, &host.domain)
    {
        if let Some((scheme, browser_host, port)) = candidates
            .iter()
            .find(|(candidate_scheme, candidate_host, candidate_port)| {
                endpoint_ready(candidate_scheme, candidate_host, *candidate_port)
            })
            .cloned()
        {
            return open_ready_host(&store, &host, scheme, browser_host, port);
        }
    }
    if !crate::hosts::hosts_file_maps_domain(&host.domain) {
        crate::hosts::sync_hosts_file().map_err(|err| {
            format!(
                "{} is not mapped in the Windows hosts file and automatic sync failed: {err}",
                host.domain
            )
        })?;
        if !crate::hosts::hosts_file_maps_domain(&host.domain) {
            return Err(format!(
                "{} is still not mapped in the Windows hosts file. Approve the administrator prompt, then open the site again.",
                host.domain
            ));
        }
    }
    crate::hosts::sync_proxy_bypass_for_hosts(&snapshot).map_err(|err| {
        format!(
            "Cannot add {} to Windows proxy bypass list: {err}",
            host.domain
        )
    })?;
    if web_runtime_needs_refresh(&store, service_id, &host.domain) {
        crate::services::restart_service(service_id.to_string()).map_err(|err| {
            format!(
                "{} runtime config is stale or does not contain {} and could not be repaired automatically: {err}",
                service_name, host.domain
            )
        })?;
    }
    ensure_host_database_service(&store, &snapshot, &host)?;
    let Some((scheme, browser_host, port)) = candidates
        .iter()
        .find(|(candidate_scheme, candidate_host, candidate_port)| {
            endpoint_ready(candidate_scheme, candidate_host, *candidate_port)
        })
        .cloned()
    else {
        let tried = candidates
            .iter()
            .map(|(scheme, candidate_host, candidate_port)| {
                format!("{scheme}://{candidate_host}:{candidate_port}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "{} did not answer on any configured endpoint. Tried: {tried}. Start {} or sync the Windows hosts file.",
            host.domain, service_name
        ));
    };
    open_ready_host(&store, &host, scheme, browser_host, port)
}

fn open_ready_host(
    store: &Store,
    host: &crate::state::HostInfo,
    scheme: &'static str,
    browser_host: String,
    port: u16,
) -> AppResult<crate::state::AppSnapshot> {
    let default_port = (scheme == "https" && port == 443) || (scheme == "http" && port == 80);
    let url = if default_port {
        format!("{scheme}://{browser_host}")
    } else {
        format!("{scheme}://{browser_host}:{port}")
    };
    let mut snapshot = store.load_static()?;
    open_browser_target(&url)
        .map_err(|err| format!("Cannot open host {} at {url}: {err}", host.domain))?;
    store.log(
        &mut snapshot,
        LogLevel::Info,
        &host.domain,
        format!("Opened host {} at {url}", host.domain),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

fn ensure_host_database_service(
    store: &Store,
    snapshot: &crate::state::AppSnapshot,
    host: &crate::state::HostInfo,
) -> AppResult<()> {
    if host.database.trim().is_empty() {
        return Ok(());
    }
    let service_id = snapshot
        .databases
        .iter()
        .find(|database| {
            database.id.eq_ignore_ascii_case(&host.database)
                || database.name.eq_ignore_ascii_case(&host.database)
        })
        .map(|database| database_service_id(&database.engine))
        .unwrap_or("mysql");
    let Some(service) = snapshot
        .services
        .iter()
        .find(|service| service.id == service_id)
    else {
        return Ok(());
    };
    let service_name = service.name.clone();
    let service_status = service.status.clone();
    if service_status == crate::state::ServiceStatus::Running {
        return Ok(());
    }
    crate::services::start_service(service_id.to_string()).map_err(|err| {
        format!(
            "{} requires database service {}, but it could not be started: {err}",
            host.domain, service_name
        )
    })?;
    let mut snapshot = store.load_static()?;
    store.log(
        &mut snapshot,
        LogLevel::Info,
        &host.domain,
        format!(
            "Database service {} was started before opening {}",
            service_name, host.domain
        ),
        None,
    );
    store.save(&snapshot)?;
    Ok(())
}

fn database_service_id(engine: &str) -> &'static str {
    match engine.to_lowercase().as_str() {
        "mariadb" => "mariadb",
        "postgresql" | "postgres" => "postgresql",
        _ => "mysql",
    }
}

fn host_open_candidates(
    host: &crate::state::HostInfo,
    cert_ready: bool,
) -> Vec<(&'static str, String, u16)> {
    let mut candidates = Vec::new();
    candidates.push(("http", host.domain.clone(), host.http_port));
    if host.ssl && cert_ready {
        candidates.push(("https", host.domain.clone(), host.https_port));
    }
    candidates.dedup();
    candidates
}

fn host_certificate_files_ready(
    store: &Store,
    snapshot: &crate::state::AppSnapshot,
    domain: &str,
) -> bool {
    snapshot
        .certificates
        .iter()
        .find(|cert| cert.domain.eq_ignore_ascii_case(domain))
        .map(|cert| Path::new(&cert.cert_path).is_file() && Path::new(&cert.key_path).is_file())
        .unwrap_or_else(|| {
            store
                .dir
                .join("certs")
                .join(format!("{domain}.crt"))
                .is_file()
                && store
                    .dir
                    .join("keys")
                    .join(format!("{domain}.key"))
                    .is_file()
        })
}

fn web_runtime_needs_refresh(store: &Store, service_id: &str, domain: &str) -> bool {
    let config = match service_id {
        "nginx" => store
            .dir
            .join("configs")
            .join("nginx-runtime")
            .join("conf")
            .join("nginx.conf"),
        _ => store
            .dir
            .join("configs")
            .join("apache-runtime")
            .join("httpd.conf"),
    };
    let Ok(text) = fs::read_to_string(config) else {
        return true;
    };
    let missing_host = !text.lines().any(|line| {
        line.trim()
            .eq_ignore_ascii_case(&format!("ServerName {domain}"))
            || line
                .trim()
                .eq_ignore_ascii_case(&format!("server_name  {domain};"))
            || line
                .trim()
                .eq_ignore_ascii_case(&format!("server_name {domain};"))
    });
    missing_host || (service_id == "apache" && apache_runtime_text_needs_refresh(&text))
}

fn apache_runtime_needs_refresh(store: &Store) -> bool {
    let config = store
        .dir
        .join("configs")
        .join("apache-runtime")
        .join("httpd.conf");
    fs::read_to_string(config)
        .map(|text| apache_runtime_text_needs_refresh(&text))
        .unwrap_or(true)
}

fn apache_runtime_text_needs_refresh(text: &str) -> bool {
    !text.contains("Timeout 90")
        || !text.contains("SetEnv PHPRC")
        || !text.contains("SetEnv TMP")
        || !text.contains("Alias /localstack-tools/")
}

pub fn open_database_admin(kind: String) -> AppResult<()> {
    let normalized = kind.to_lowercase();
    let store = Store::new()?;
    let tools_root = store.dir.join("tools").join("public");
    fs::create_dir_all(&tools_root)
        .map_err(|err| format!("Cannot create LocalStack tools folder: {err}"))?;
    let (tool_name, route) = install_database_admin_tool_at(&store, &tools_root, &normalized)?;
    let snapshot = store.load_static()?;
    let apache_running = snapshot.services.iter().any(|service| {
        service.id == "apache" && service.status == crate::state::ServiceStatus::Running
    });
    if !apache_running {
        crate::services::start_service("apache".to_string())
            .map_err(|err| format!("Cannot start Apache before opening {tool_name}: {err}"))?;
    } else if web_runtime_needs_refresh(&store, "apache", "localhost")
        || (normalized == "phpmyadmin" && apache_runtime_needs_refresh(&store))
    {
        crate::services::restart_service("apache".to_string()).map_err(|err| {
            format!("Cannot refresh Apache tools route before opening {tool_name}: {err}")
        })?;
    }
    let url = format!("http://127.0.0.1/localstack-tools/{route}");
    open_browser_target(&url).map_err(|err| format!("Cannot open {tool_name} at {url}: {err}"))
}

pub fn install_database_admin_tool(kind: String) -> AppResult<String> {
    let normalized = kind.to_lowercase();
    let store = Store::new()?;
    let tools_root = store.dir.join("tools").join("public");
    fs::create_dir_all(&tools_root)
        .map_err(|err| format!("Cannot create LocalStack tools folder: {err}"))?;
    let (tool_name, route) = install_database_admin_tool_at(&store, &tools_root, &normalized)?;
    Ok(format!("{tool_name}=localstack-tools/{route}"))
}

fn install_database_admin_tool_at(
    store: &Store,
    tools_root: &Path,
    normalized: &str,
) -> AppResult<(&'static str, &'static str)> {
    if normalized == "adminer" {
        ensure_adminer(tools_root)?;
        Ok(("Adminer", "adminer.php"))
    } else {
        ensure_phpmyadmin(store, tools_root)?;
        Ok(("phpMyAdmin", "phpmyadmin/"))
    }
}

fn tool_placeholder_needs_download(target: &Path) -> bool {
    fs::read_to_string(target)
        .map(|text| text.contains("The database tool route is ready."))
        .unwrap_or(false)
}

fn ensure_adminer(tools_root: &Path) -> AppResult<()> {
    let target = tools_root.join("adminer.php");
    if !target.exists() || tool_placeholder_needs_download(&target) {
        download_tool("Adminer", "https://www.adminer.org/latest.php", &target)?;
    }
    Ok(())
}

fn ensure_phpmyadmin(store: &Store, tools_root: &Path) -> AppResult<()> {
    let target = tools_root.join("phpmyadmin");
    let index = target.join("index.php");
    if index.exists() && !tool_placeholder_needs_download(&index) {
        ensure_phpmyadmin_config(&target)?;
        return Ok(());
    }
    let temp = store.dir.join("temp").join("phpmyadmin-download");
    let archive = temp.join("phpmyadmin.zip");
    let extract = temp.join("extract");
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&extract)
        .map_err(|err| format!("Cannot create phpMyAdmin temp folder: {err}"))?;
    download_archive(
        "phpMyAdmin",
        "https://www.phpmyadmin.net/downloads/phpMyAdmin-latest-all-languages.zip",
        &archive,
        &extract,
    )?;
    let source = extracted_tool_root(&extract, "index.php")?;
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(&target).map_err(|err| format!("Cannot create phpMyAdmin folder: {err}"))?;
    copy_dir_all(&source, &target)?;
    ensure_phpmyadmin_config(&target)?;
    let _ = fs::remove_dir_all(&temp);
    Ok(())
}

fn ensure_phpmyadmin_config(target: &Path) -> AppResult<()> {
    let config = target.join("config.inc.php");
    let tmp = target.join("tmp");
    let sessions = tmp.join("sessions");
    fs::create_dir_all(&sessions)
        .map_err(|err| format!("Cannot create phpMyAdmin session folder: {err}"))?;
    let mut content = if config.exists() {
        fs::read_to_string(&config)
            .map_err(|err| format!("Cannot read phpMyAdmin config: {err}"))?
    } else {
        let blowfish = uuid::Uuid::new_v4().simple().to_string();
        format!(
            r#"<?php
$cfg['blowfish_secret'] = '{blowfish}';
$i = 0;
$i++;
$cfg['Servers'][$i]['auth_type'] = 'cookie';
$cfg['Servers'][$i]['host'] = '127.0.0.1';
$cfg['Servers'][$i]['port'] = '3306';
$cfg['Servers'][$i]['compress'] = false;
$cfg['Servers'][$i]['AllowNoPassword'] = true;
$cfg['TempDir'] = __DIR__ . '/tmp';
"#
        )
    };
    let session_fix = r#"
if (!is_dir(__DIR__ . '/tmp')) { @mkdir(__DIR__ . '/tmp', 0777, true); }
if (!is_dir(__DIR__ . '/tmp/sessions')) { @mkdir(__DIR__ . '/tmp/sessions', 0777, true); }
@ini_set('sys_temp_dir', __DIR__ . '/tmp');
@ini_set('upload_tmp_dir', __DIR__ . '/tmp');
@ini_set('session.save_handler', 'files');
@ini_set('session.save_path', __DIR__ . '/tmp/sessions');
$cfg['TempDir'] = __DIR__ . '/tmp';
"#;
    if !content.contains("session.save_path") {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        if let Some(close_tag) = content.rfind("?>") {
            content.insert_str(close_tag, session_fix);
        } else {
            content.push_str(session_fix);
        }
    }
    fs::write(&config, content).map_err(|err| format!("Cannot write phpMyAdmin config: {err}"))?;
    Ok(())
}

fn download_tool(name: &str, url: &str, target: &Path) -> AppResult<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Cannot create tools folder {}: {err}", parent.display()))?;
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}",
        powershell_quote(url),
        powershell_quote(&target.display().to_string())
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
        .map_err(|err| format!("Cannot start {name} downloader: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot download {name}. {detail}"));
    }
    Ok(())
}

fn compress_paths(sources: &[PathBuf], target: &Path) -> AppResult<()> {
    let source_list = sources
        .iter()
        .map(|path| powershell_quote(&path.display().to_string()))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Compress-Archive -LiteralPath @({source_list}) -DestinationPath {} -Force",
        powershell_quote(&target.display().to_string())
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
        .map_err(|err| format!("Cannot start backup compressor: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot create backup archive. {detail}"));
    }
    Ok(())
}

fn expand_archive(source: &Path, target: &Path) -> AppResult<()> {
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        powershell_quote(&source.display().to_string()),
        powershell_quote(&target.display().to_string())
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
        .map_err(|err| format!("Cannot start backup extractor: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot extract backup archive. {detail}"));
    }
    Ok(())
}

fn restore_root(temp: &Path) -> AppResult<PathBuf> {
    if temp.join("state.json").is_file() {
        return Ok(temp.to_path_buf());
    }
    for entry in fs::read_dir(temp).map_err(|err| format!("Cannot read restore temp: {err}"))? {
        let entry = entry.map_err(|err| format!("Cannot read restore entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() && path.join("state.json").is_file() {
            return Ok(path);
        }
    }
    Err("Backup archive does not contain a LocalStack Pro data root.".to_string())
}

fn copy_path(source: &Path, target: &Path) -> AppResult<()> {
    if source.is_dir() {
        fs::create_dir_all(target)
            .map_err(|err| format!("Cannot create {}: {err}", target.display()))?;
        for entry in fs::read_dir(source)
            .map_err(|err| format!("Cannot read {}: {err}", source.display()))?
        {
            let entry = entry.map_err(|err| format!("Cannot read backup entry: {err}"))?;
            copy_path(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Cannot create {}: {err}", parent.display()))?;
        }
        fs::copy(source, target).map_err(|err| {
            format!(
                "Cannot restore {} to {}: {err}",
                source.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

fn copy_path_filtered(source: &Path, target: &Path) -> AppResult<()> {
    if should_skip_backup_path(source) {
        return Ok(());
    }
    if source.is_dir() {
        fs::create_dir_all(target)
            .map_err(|err| format!("Cannot create {}: {err}", target.display()))?;
        for entry in fs::read_dir(source)
            .map_err(|err| format!("Cannot read {}: {err}", source.display()))?
        {
            let entry = entry.map_err(|err| format!("Cannot read backup entry: {err}"))?;
            copy_path_filtered(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Cannot create {}: {err}", parent.display()))?;
        }
        fs::copy(source, target).map_err(|err| {
            format!(
                "Cannot stage {} to {}: {err}",
                source.display(),
                target.display()
            )
        })?;
    }
    Ok(())
}

fn should_skip_backup_path(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    matches!(extension.to_lowercase().as_str(), "log" | "pid" | "lock")
}

fn download_archive(name: &str, url: &str, archive: &Path, extract: &Path) -> AppResult<()> {
    if let Some(parent) = archive.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Cannot create tools temp folder {}: {err}",
                parent.display()
            )
        })?;
    }
    let script = format!(
        "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; [Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}; Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        powershell_quote(url),
        powershell_quote(&archive.display().to_string()),
        powershell_quote(&archive.display().to_string()),
        powershell_quote(&extract.display().to_string())
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
        .map_err(|err| format!("Cannot start {name} downloader: {err}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot download or extract {name}. {detail}"));
    }
    Ok(())
}

fn extracted_tool_root(extract: &Path, marker: &str) -> AppResult<PathBuf> {
    if extract.join(marker).is_file() {
        return Ok(extract.to_path_buf());
    }
    for entry in
        fs::read_dir(extract).map_err(|err| format!("Cannot read extracted tool: {err}"))?
    {
        let entry = entry.map_err(|err| format!("Cannot read extracted tool entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() && path.join(marker).is_file() {
            return Ok(path);
        }
    }
    Err(format!("Extracted package does not contain {marker}."))
}

fn copy_dir_all(source: &Path, target: &Path) -> AppResult<()> {
    fs::create_dir_all(target)
        .map_err(|err| format!("Cannot create {}: {err}", target.display()))?;
    for entry in
        fs::read_dir(source).map_err(|err| format!("Cannot read {}: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("Cannot read tool entry: {err}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else {
            fs::copy(&source_path, &target_path)
                .map_err(|err| format!("Cannot copy {}: {err}", target_path.display()))?;
        }
    }
    Ok(())
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn open_terminal(path: String) -> AppResult<()> {
    let target = ensure_open_target(Path::new(path.trim()))?;
    let mut terminal = Command::new("wt.exe");
    terminal
        .args(["-d", &target.display().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    terminal.creation_flags(CREATE_NO_WINDOW);
    if terminal.spawn().is_ok() {
        return Ok(());
    }
    let mut fallback = Command::new("cmd");
    fallback.args([
        "/C",
        "start",
        "cmd",
        "/K",
        &format!("cd /d \"{}\"", target.display()),
    ]);
    #[cfg(windows)]
    fallback.creation_flags(CREATE_NO_WINDOW);
    fallback
        .spawn()
        .map_err(|err| format!("Cannot open terminal at {}: {err}", target.display()))?;
    Ok(())
}

fn open_browser_target(target: &str) -> AppResult<()> {
    let preferred = Store::new()
        .and_then(|store| store.load_static())
        .map(|snapshot| snapshot.settings.preferred_browser)
        .unwrap_or_else(|_| "Default System Browser".to_string());
    if launch_preferred_browser(&preferred, target).is_ok() {
        return Ok(());
    }
    open::that(target).map_err(|err| format!("Cannot open {target}: {err}"))
}

fn launch_preferred_browser(preferred: &str, target: &str) -> AppResult<()> {
    let normalized = preferred.to_lowercase();
    let candidates: &[&str] = if normalized.contains("chrome") {
        &[
            "chrome",
            "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
            "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        ]
    } else if normalized.contains("edge") {
        &[
            "msedge",
            "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
            "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
        ]
    } else if normalized.contains("firefox") {
        &[
            "firefox",
            "C:\\Program Files\\Mozilla Firefox\\firefox.exe",
            "C:\\Program Files (x86)\\Mozilla Firefox\\firefox.exe",
        ]
    } else {
        return Err("Default browser selected.".to_string());
    };
    for candidate in candidates {
        let mut command = Command::new(candidate);
        command
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        if command.spawn().is_ok() {
            return Ok(());
        }
    }
    Err(format!("Preferred browser {preferred} was not found."))
}

fn ensure_open_target(requested: &Path) -> AppResult<PathBuf> {
    let extension = requested
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if requested.exists() {
        if extension == "exe" {
            return requested.parent().map(Path::to_path_buf).ok_or_else(|| {
                format!("Cannot resolve parent folder for {}.", requested.display())
            });
        }
        return Ok(requested.to_path_buf());
    }
    if extension == "exe" {
        let parent = requested
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("Cannot resolve parent folder for {}.", requested.display()))?;
        fs::create_dir_all(&parent)
            .map_err(|err| format!("Cannot create folder {}: {err}", parent.display()))?;
        return Ok(parent);
    }
    let file_like = matches!(
        extension.as_str(),
        "conf" | "ini" | "json" | "log" | "txt" | "pem" | "crt" | "key" | "sql"
    );
    if file_like {
        if let Some(parent) = requested.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Cannot create folder {}: {err}", parent.display()))?;
        }
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(requested)
            .map_err(|err| format!("Cannot create file {}: {err}", requested.display()))?;
    } else {
        fs::create_dir_all(requested)
            .map_err(|err| format!("Cannot create folder {}: {err}", requested.display()))?;
    }
    Ok(requested.to_path_buf())
}

fn resolve_data_path(store: &Store, path: &str) -> PathBuf {
    let target = PathBuf::from(path);
    if target.is_absolute() {
        target
    } else {
        store.dir.join(target)
    }
}

fn tcp_ready(host: &str, port: u16) -> bool {
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    let addresses = addresses.collect::<Vec<_>>();
    for _ in 0..4 {
        if addresses
            .iter()
            .any(|address| TcpStream::connect_timeout(address, Duration::from_millis(180)).is_ok())
        {
            return true;
        }
        thread::sleep(Duration::from_millis(60));
    }
    false
}

fn endpoint_ready(scheme: &str, host: &str, port: u16) -> bool {
    if scheme == "http" {
        return http_endpoint_ready(host, port);
    }
    tcp_ready(host, port)
}

fn http_endpoint_ready(host: &str, port: u16) -> bool {
    let Ok(addresses) = (host, port).to_socket_addrs() else {
        return false;
    };
    let addresses = addresses.collect::<Vec<_>>();
    for path in ["/check.php", "/install.php", "/"] {
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nUser-Agent: LocalStackPro/1.0\r\n\r\n"
        );
        for address in &addresses {
            let Ok(mut stream) = TcpStream::connect_timeout(address, Duration::from_millis(250))
            else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
            if std::io::Write::write_all(&mut stream, request.as_bytes()).is_err() {
                continue;
            }
            let mut buffer = [0_u8; 128];
            let Ok(size) = std::io::Read::read(&mut stream, &mut buffer) else {
                continue;
            };
            let head = String::from_utf8_lossy(&buffer[..size]);
            if head.starts_with("HTTP/")
                && !head.contains(" 502 ")
                && !head.contains(" 503 ")
                && !head.contains(" 504 ")
            {
                return true;
            }
        }
    }
    for address in addresses {
        let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_millis(250)) else {
            continue;
        };
        let _ = stream.shutdown(std::net::Shutdown::Both);
        return true;
    }
    false
}

fn apply_startup(enabled: bool) -> AppResult<()> {
    let appdata =
        std::env::var("APPDATA").map_err(|err| format!("Cannot resolve APPDATA: {err}"))?;
    let startup_dir =
        std::path::Path::new(&appdata).join("Microsoft\\Windows\\Start Menu\\Programs\\Startup");
    let startup = startup_dir.join("LocalStack Pro.vbs");
    let legacy_cmd = startup_dir.join("LocalStack Pro.cmd");
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|err| format!("Cannot resolve current executable: {err}"))?;
        let script = format!(
            "Set shell = CreateObject(\"WScript.Shell\")\r\nshell.Run \"\"\"{}\"\"\", 0, False\r\n",
            exe.display()
        );
        fs::write(&startup, script)
            .map_err(|err| format!("Cannot create startup shortcut command: {err}"))?;
        let _ = fs::remove_file(&legacy_cmd);
    } else {
        if startup.exists() {
            fs::remove_file(&startup)
                .map_err(|err| format!("Cannot remove startup command: {err}"))?;
        }
        if legacy_cmd.exists() {
            fs::remove_file(&legacy_cmd)
                .map_err(|err| format!("Cannot remove legacy startup command: {err}"))?;
        }
    }
    Ok(())
}
