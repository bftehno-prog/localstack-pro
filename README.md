# LocalStack Pro

Creator: [Farid Leonov](https://artnext.ru)

LocalStack Pro is a Windows desktop app built with Tauri, React, TypeScript, and Rust. It manages local web-development environments through the Rust backend: process start/stop/restart for configured native executables, AppData persistence, host config files, Windows hosts-file elevation, certificate generation/trust/revoke requests, database command execution, logs, settings, and system tray actions.

The default visual mode is the Wet Asphalt theme with a lime LocalStack Pro logo. The desktop window starts maximized, and the NSIS installer creates a current-user desktop shortcut with the same lime application icon.

## Кратко по-русски

LocalStack Pro — Windows desktop-приложение для локальной веб-разработки. Оно управляет реальными сервисами Apache, Nginx, PHP, MySQL, MariaDB, PostgreSQL, Redis, Mailpit, Node.js Proxy и DNS Helper, создает хосты `.test`, синхронизирует Windows hosts-файл, генерирует SSL-сертификаты, устанавливает CMS и дает двухпанельный файловый менеджер с редактором кода.

Основные изменения версии `1.0.1`:

- приложение открывается сразу развернутым на весь экран;
- установщик создает ярлык на рабочем столе с lime-иконкой;
- вся документация доступна на русском и английском;
- расширены переводы интерфейса при переключении языка на русский;
- обновлен lime-логотип для приложения, установщика и tray-состояний.

## Stack

- Tauri 2
- React 19
- TypeScript
- Rust
- Windows NSIS installer

## Development

```powershell
npm install
npm run tauri:dev
```

The React-only preview is available with:

```powershell
npm run dev
```

The browser preview uses fallback data because Tauri commands are only available inside the desktop runtime. The installed desktop app reads live state from the Rust backend.

## Production Build

```powershell
npm run tauri:build
```

Build outputs:

- App executable: `src-tauri\target\release\localstack-pro.exe`
- Windows installer: `src-tauri\target\release\bundle\nsis\LocalStack Pro_1.0.1_x64-setup.exe`
- Convenience copy: `release\LocalStack Pro_1.0.1_x64-setup.exe`

The installer uses `src-tauri\icons\icon.ico` for both the installer and the installed desktop shortcut. The shortcut is created by `src-tauri\installer-hooks.nsh`.

## App Data

On first launch the backend creates a data directory under Windows AppData:

```text
%APPDATA%\LocalStack\LocalStack Pro\data
```

It contains:

- `state.json`
- `configs`
- `hosts`
- `logs`
- `backups`
- `certs`
- `keys`

## Documentation

In the app, open `Settings -> Need Help? -> Open Documentation`. LocalStack Pro writes a bilingual HTML guide to:

```text
%APPDATA%\LocalStack\LocalStack Pro\data\documentation\LocalStack Pro Documentation.html
```

The bundled documentation covers:

- first launch and service detection
- hosts and Windows hosts-file sync
- Apache/Nginx/PHP runtime behavior
- database creation, import, export and backups
- SSL certificate generation and trust repair
- CMS installation
- Node.js, Next.js, Vite and Express hosting
- the two-pane file manager and code editor
- themes, tray behavior, backups and diagnostics

## Service Management

Services are configured in `state.json` with executable paths. Start, stop, and restart commands operate on those executables and stored PIDs.

LocalStack Pro no longer fakes service status with a managed helper. If a native executable is missing, the service remains stopped/error and the UI shows the missing path. Install the native binary or update the executable path before starting the service.

The Services page includes:

- `Detect`: finds native binaries installed through winget, PATH, Program Files, and common Windows locations.
- `Install Missing`: installs supported missing dependencies through winget.
- Per-service `Install`: installs or detects one selected service dependency.

Automatic dependency IDs:

- Apache: `ApacheLounge.httpd`
- Nginx: `nginxinc.nginx`
- MySQL: `Oracle.MySQL`
- MariaDB: `MariaDB.Server`
- PostgreSQL: `PostgreSQL.PostgreSQL.16`
- Redis: `Redis.Redis`
- Mailpit: `axllent.mailpit`
- Node.js Proxy: `OpenJS.NodeJS.LTS`

Default services:

- Apache
- Nginx
- MySQL
- MariaDB
- PostgreSQL
- Redis
- Mailpit
- Node.js Proxy
- DNS Helper

If a configured port is already occupied, LocalStack Pro refuses to start the service and reports the port conflict.

Some engines, such as MariaDB, PostgreSQL, and Redis, can be installed as Windows Services. LocalStack Pro detects those services and treats already-running Windows Services as live runtime state instead of trying to launch a duplicate process.

Apache and Nginx receive generated runtime configs under AppData so winget installs can start without manual terminal commands. Node.js Proxy runs a generated local proxy script on `127.0.0.1:3000`. DNS Helper runs through the LocalStack Pro executable in helper mode; if UDP `5353` is already occupied by mDNS, it selects the next available local helper port.

## Windows Hosts File

The `sync_hosts_file` command writes a temporary PowerShell script and launches it with `RunAs`, so Windows prompts for administrator approval before editing:

```text
%WINDIR%\System32\drivers\etc\hosts
```

## Host Configs

Saving a host writes:

- `hosts\<domain>.json`
- `configs\apache\vhosts\<domain>.conf`
- `configs\nginx\vhosts\<domain>.conf`

Include those generated vhost snippets from your Apache/Nginx configuration to serve the host through the real web server.

## SSL

Local certificates are generated with Rust `rcgen` and written to AppData. Trust and revoke operations call Windows `certutil` through an elevated process request and update UI state only after the elevated command returns successfully.

## Database Operations

Database creation, deletion, and backup require the corresponding database service to be running and the proper client tools (`mysql.exe`, `mysqldump.exe`, `psql.exe`, `pg_dump.exe`) to exist near the configured service executable.
When those native client tools are not available, LocalStack Pro returns a clear error instead of creating fake database records. Admin credentials can be provided through environment variables:

- `LOCALSTACK_MYSQL_ADMIN_USER`
- `LOCALSTACK_MYSQL_ADMIN_PASSWORD`
- `LOCALSTACK_MARIADB_ADMIN_USER`
- `LOCALSTACK_MARIADB_ADMIN_PASSWORD`
- `LOCALSTACK_POSTGRES_ADMIN_USER`
- `LOCALSTACK_POSTGRES_ADMIN_PASSWORD`

## CMS Installer

The CMS page can install popular CMS packages directly from the app:

- WordPress from `https://wordpress.org/latest.zip`
- Joomla from the official Joomla 6 full package ZIP
- Drupal from `https://www.drupal.org/download-latest/zip`
- Grav from the official latest core package

Installation downloads the package without opening a terminal window, extracts it into the selected project folder, creates a LocalStack host, writes host configuration, records the installation in `state.json`, and can create a database when the selected database service is running.

For database-backed CMS installs, start MySQL, MariaDB, or PostgreSQL first and make sure the native client tool and admin credentials are available. Grav can be installed without a database.

## Installing Any Node.js Site

LocalStack Pro can run arbitrary Node.js, Next.js, Vite, Express, Fastify, Koa, Nest, Nuxt, Astro, and similar projects through Apache + Node.js Proxy.

### 1. Prepare the project

- Put the project in a clean Windows path, for example `C:\Projects\my-node-site`.
- Make sure the project has `package.json`.
- For an existing GitHub project, clone it into `C:\Projects`, then run `npm install`.
- If dependency resolution fails, try `npm install --legacy-peer-deps`.
- For Prisma projects, run the project-specific setup, usually:

```powershell
npx prisma generate
npx prisma db push
npx prisma migrate deploy
npx prisma db seed
```

Use only the commands that exist in that project.

### 2. Configure package scripts

LocalStack Pro starts Node apps through Node.js Proxy with this pattern:

```powershell
npm run <script> -- --port <port>
```

The selected script must accept the port argument or safely ignore it.

Recommended scripts:

```json
{
  "scripts": {
    "dev": "next dev --hostname 127.0.0.1",
    "start": "next start --hostname 127.0.0.1"
  }
}
```

For Vite:

```json
{
  "scripts": {
    "dev": "vite --host 127.0.0.1"
  }
}
```

For Express/Fastify/Koa:

```js
const port = Number(process.env.PORT || process.env.LOCALSTACK_NODE_PORT || 3100);
app.listen(port, "127.0.0.1");
```

### 3. Create or edit the LocalStack host

Create a host in `Hosts` or install one from `CMS`:

- Domain: `myapp.test`
- Root folder: `C:\Projects\my-node-site`
- Document root: `.`
- Web server: `Apache`
- SSL: disabled until the LocalStack CA is trusted
- Tags: `node` plus one of `nextjs`, `vite-react`, `node-express`

Add these env variables to the host:

```text
APP_URL=http://myapp.test
LOCALSTACK_NODE_PORT=3100
LOCALSTACK_NODE_SCRIPT=start
LOCALSTACK_NODE_KIND=nextjs
```

Use `LOCALSTACK_NODE_SCRIPT=dev` while developing. Use `start` after a production build.

### 4. Sync and start

1. Click `Sync Hosts File`.
2. Approve the administrator prompt if Windows asks.
3. Start `Apache`.
4. Start `Node.js Proxy`.
5. Open `http://myapp.test`.

The first request can return a temporary `502` for a few seconds while npm starts the app. Refresh once after startup.

### 5. Production mode

For Next.js:

```powershell
npm run build
```

Then set:

```text
LOCALSTACK_NODE_SCRIPT=start
```

For Vite, use a preview script only when it accepts a port argument:

```json
{
  "scripts": {
    "preview": "vite preview --host 127.0.0.1"
  }
}
```

Then set:

```text
LOCALSTACK_NODE_SCRIPT=preview
```

### 6. Troubleshooting Node apps

- `502 Bad Gateway`: the Node app is still starting or the npm script crashed. Check Logs and run the same `npm run ...` command manually once to see the error.
- `404`: wrong root folder, missing app route, or a dev server that rejected the host.
- `ERR_CONNECTION_RESET`: Apache or Node.js Proxy is stopped, or a port is already occupied.
- `Host not allowed`: add the `.test` domain to the dev-server allowlist. For Vite, configure `server.allowedHosts`.
- Prisma/database error: verify `.env`, run migrations/seed, and make sure the database file or server is available.
- Next.js opens slowly in `dev`: run `npm run build` and switch to `LOCALSTACK_NODE_SCRIPT=start`.
- Port conflict: change `LOCALSTACK_NODE_PORT` to another free local port, then restart Node.js Proxy.

### 7. Example: installing a GitHub Next.js project

```powershell
git clone https://github.com/example/project.git C:\Projects\project
cd C:\Projects\project
npm install --legacy-peer-deps
npm run build
```

Then create a LocalStack host:

```text
domain: project.test
rootFolder: C:\Projects\project
documentRoot: .
webServer: Apache
tags: node,nextjs
APP_URL=http://project.test
LOCALSTACK_NODE_PORT=3100
LOCALSTACK_NODE_SCRIPT=start
LOCALSTACK_NODE_KIND=nextjs
```

Sync hosts, start Apache + Node.js Proxy, and open `http://project.test`.

## Verification

Completed checks:

```powershell
npm run build
cargo check
npm run tauri:build
```

The NSIS installer was successfully generated.

### Responsive Visual Audit

Run the visual responsive audit with:

```powershell
npm run audit:responsive
```

The audit starts a local Vite server, opens every main page at desktop, tablet, and mobile viewport sizes, captures screenshots, and checks for:

- horizontal page overflow
- clipped text inside controls/cards/table cells
- overlapping visible controls
- blank/broken pages

Outputs are generated locally and ignored by Git:

- HTML report: `reports\responsive\responsive-report.html`
- JSON report: `reports\responsive\responsive-report.json`
- screenshots: `reports\responsive\*.png`

Local runtime smoke checks on this Windows machine:

- Apache: `http://127.0.0.1/` returned `200`
- Nginx: `http://127.0.0.1:8080/` returned `200`
- Node.js Proxy: `http://127.0.0.1:3000/` returned `200`
- Mailpit UI: `http://127.0.0.1:8025/` returned `200`
- DNS Helper returned `127.0.0.1` for a `.test` A-record query
