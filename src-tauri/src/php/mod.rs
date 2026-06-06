use crate::state::{AppResult, LogLevel, PhpVersion, Store};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn install_php_version(version: String) -> AppResult<crate::state::AppSnapshot> {
    let requested = normalize_php_line(&version);
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let executable = find_php_executable(&requested)
        .or_else(|| {
            install_php_with_winget(&requested).ok()?;
            find_php_executable(&requested)
        })
        .or_else(|| install_php_portable(&store, &requested).ok())
        .ok_or_else(|| {
        format!(
            "PHP {requested} was not found or installed. LocalStack Pro could not install the portable PHP package. Check internet access or set a valid php.exe path."
        )
    })?;
    let detected = php_version_string(&executable).unwrap_or_else(|| requested.clone());
    let existing_by_path = snapshot.php_versions.iter().any(|php| {
        php.cli_path
            .eq_ignore_ascii_case(&executable.display().to_string())
    });
    if existing_by_path {
        store.log(
            &mut snapshot,
            LogLevel::Info,
            "PHP",
            format!(
                "PHP {detected} already detected at {}",
                executable.display()
            ),
            None,
        );
        store.save(&snapshot)?;
        return Ok(snapshot);
    }
    let make_default = snapshot
        .php_versions
        .iter()
        .find(|php| php.default)
        .map(|php| !Path::new(&php.cli_path).exists())
        .unwrap_or(true);
    let base = snapshot
        .php_versions
        .iter()
        .find(|php| php.default)
        .or_else(|| snapshot.php_versions.first())
        .cloned()
        .ok_or_else(|| "No PHP template exists in LocalStack Pro settings.".to_string())?;
    let next = PhpVersion {
        version: detected.clone(),
        label: detected.split('.').take(2).collect::<Vec<_>>().join("."),
        status: if make_default { "active" } else { "installed" }.to_string(),
        default: make_default,
        cli_path: executable.display().to_string(),
        sapi_mode: "CGI".to_string(),
        extensions: base.extensions,
        ini: base.ini,
        compatibility: if detected.starts_with("8.") {
            "Full".to_string()
        } else {
            "Legacy".to_string()
        },
    };
    write_ini(&store, &next)?;
    if make_default {
        for php in &mut snapshot.php_versions {
            php.default = false;
            if php.status == "active" {
                php.status = "installed".to_string();
            }
        }
    }
    if let Some(existing) = snapshot.php_versions.iter_mut().find(|php| {
        php.version == detected || php.version.starts_with(&requested) || php.label == requested
    }) {
        *existing = next;
    } else {
        snapshot.php_versions.push(next);
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "PHP",
        format!(
            "PHP {detected} installed or detected at {}",
            executable.display()
        ),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn save_php_version(php: PhpVersion) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if let Some(existing) = snapshot
        .php_versions
        .iter_mut()
        .find(|item| item.version == php.version)
    {
        *existing = php.clone();
    } else {
        snapshot.php_versions.push(php.clone());
    }
    write_ini(&store, &php)?;
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "PHP",
        format!("PHP {} settings saved", php.version),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn remove_php_version(version: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if snapshot
        .php_versions
        .iter()
        .any(|item| item.version == version && item.default)
    {
        return Err("Cannot remove the default PHP version. Switch default first.".to_string());
    }
    snapshot.php_versions.retain(|item| item.version != version);
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "PHP",
        format!("PHP {version} removed from LocalStack Pro configuration"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn set_default_php(version: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if !snapshot
        .php_versions
        .iter()
        .any(|item| item.version == version)
    {
        return Err(format!("PHP {version} is not installed in LocalStack Pro."));
    }
    for php in &mut snapshot.php_versions {
        php.default = php.version == version;
        php.status = if php.default { "active" } else { "installed" }.to_string();
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "PHP",
        format!("PHP {version} set as default"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

fn write_ini(store: &Store, php: &PhpVersion) -> AppResult<()> {
    let folder = store.dir.join("configs").join("php");
    fs::create_dir_all(&folder).map_err(|err| format!("Cannot create PHP config folder: {err}"))?;
    let mut lines = Vec::new();
    for (key, value) in &php.ini {
        lines.push(format!("{key}={value}"));
    }
    for ext in &php.extensions {
        if ext.enabled {
            lines.push(format!("extension={}", ext.name));
        } else {
            lines.push(format!(";extension={}", ext.name));
        }
    }
    fs::write(
        folder.join(format!("php-{}.ini", php.version)),
        lines.join("\n"),
    )
    .map_err(|err| format!("Cannot write php.ini: {err}"))
}

fn normalize_php_line(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.starts_with("8.1")
        || trimmed.starts_with("8.2")
        || trimmed.starts_with("8.3")
        || trimmed.starts_with("8.4")
        || trimmed.starts_with("8.5")
    {
        trimmed.split('.').take(2).collect::<Vec<_>>().join(".")
    } else {
        "8.4".to_string()
    }
}

fn install_php_with_winget(version: &str) -> AppResult<()> {
    let winget = which("winget.exe")
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .map(|path| {
                    path.join("Microsoft")
                        .join("WindowsApps")
                        .join("winget.exe")
                })
                .filter(|path| path.exists())
        })
        .ok_or_else(|| "Cannot install PHP: winget.exe was not found.".to_string())?;
    let package_id = format!("PHP.PHP.{version}");
    let mut command = Command::new(winget);
    command.args([
        "install",
        "--id",
        &package_id,
        "--exact",
        "--source",
        "winget",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--disable-interactivity",
        "--silent",
    ]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let status = command
        .status()
        .map_err(|err| format!("Cannot run winget for PHP {version}: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "winget failed while installing PHP {version}. Exit code: {:?}",
            status.code()
        ))
    }
}

fn install_php_portable(store: &Store, version: &str) -> AppResult<PathBuf> {
    let root = store.dir.join("services").join("php").join(version);
    let executable = root.join("php.exe");
    if executable.exists() && php_matches(&executable, version) {
        return Ok(executable);
    }
    let temp = store
        .dir
        .join("temp")
        .join(format!("php-{version}-portable"));
    let archive = temp.join("php.zip");
    let extract = temp.join("extract");
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&extract)
        .map_err(|err| format!("Cannot create PHP installer temp folder: {err}"))?;
    if let Some(parent) = root.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("Cannot create PHP services folder: {err}"))?;
    }
    let script = format!(
        r#"$ErrorActionPreference='Stop'
$ProgressPreference='SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12
$releases = Invoke-WebRequest -UseBasicParsing -Uri 'https://windows.php.net/downloads/releases/releases.json' | ConvertFrom-Json
$release = $releases.PSObject.Properties[{version_quoted}].Value
if (-not $release) {{ throw 'PHP release line {version} was not found in releases.json' }}
$zipName = $release.'nts-vs17-x64'.zip.path
if (-not $zipName) {{ $zipName = $release.'ts-vs17-x64'.zip.path }}
if (-not $zipName) {{ throw 'PHP release line {version} has no x64 zip package' }}
$url = 'https://windows.php.net/downloads/releases/' + $zipName
Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile {archive}
if (Test-Path -LiteralPath {root}) {{ Remove-Item -LiteralPath {root} -Recurse -Force }}
New-Item -ItemType Directory -Force -Path {root} | Out-Null
Expand-Archive -LiteralPath {archive} -DestinationPath {extract} -Force
$source = Get-ChildItem -LiteralPath {extract} -Directory | Where-Object {{ Test-Path -LiteralPath (Join-Path $_.FullName 'php.exe') }} | Select-Object -First 1
if (-not $source -and (Test-Path -LiteralPath (Join-Path {extract} 'php.exe'))) {{ $source = Get-Item -LiteralPath {extract} }}
if (-not $source) {{ throw 'Downloaded PHP package does not contain php.exe' }}
Copy-Item -Path (Join-Path $source.FullName '*') -Destination {root} -Recurse -Force
"#,
        version = version,
        version_quoted = powershell_quote(version),
        archive = powershell_quote(&archive.display().to_string()),
        extract = powershell_quote(&extract.display().to_string()),
        root = powershell_quote(&root.display().to_string()),
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
        .map_err(|err| format!("Cannot start portable PHP installer: {err}"))?;
    let _ = fs::remove_dir_all(&temp);
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("Cannot install portable PHP {version}. {detail}"));
    }
    if executable.exists() && php_matches(&executable, version) {
        Ok(executable)
    } else {
        Err(format!(
            "Portable PHP {version} was extracted, but php.exe is not usable at {}.",
            executable.display()
        ))
    }
}

fn find_php_executable(version: &str) -> Option<PathBuf> {
    let candidates = [
        format!("C:\\Program Files\\PHP\\{version}\\php.exe"),
        format!("C:\\Program Files\\PHP\\{version}.0\\php.exe"),
        format!("C:\\Program Files\\PHP\\{version}.20\\php.exe"),
        format!("C:\\tools\\php\\{version}\\php.exe"),
        "%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.4\\php.exe".to_string(),
        "%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.3\\php.exe".to_string(),
        "%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.2\\php.exe".to_string(),
        "%APPDATA%\\LocalStack\\LocalStack Pro\\data\\services\\php\\8.1\\php.exe".to_string(),
        "%LOCALAPPDATA%\\Microsoft\\WinGet\\Links\\php.exe".to_string(),
    ];
    candidates
        .iter()
        .map(|path| PathBuf::from(expand_env(path)))
        .find(|path| path.exists() && php_matches(path, version))
        .or_else(|| find_under(Path::new("C:\\Program Files\\PHP"), version))
        .or_else(|| which("php.exe").filter(|path| php_matches(path, version)))
}

fn php_matches(path: &Path, version: &str) -> bool {
    php_version_string(path)
        .map(|detected| detected.starts_with(version))
        .unwrap_or(false)
}

fn php_version_string(path: &Path) -> Option<String> {
    let mut command = Command::new(path);
    command.arg("-v").stdin(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.split_whitespace().find(|part| {
        part.chars().next().is_some_and(|ch| ch.is_ascii_digit()) && part.contains('.')
    })?;
    Some(
        version
            .trim_matches(|ch: char| !ch.is_ascii_digit() && ch != '.')
            .to_string(),
    )
}

fn find_under(root: &Path, version: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.file_name()?.to_string_lossy().starts_with(version) {
            let candidate = path.join("php.exe");
            if candidate.exists() && php_matches(&candidate, version) {
                return Some(candidate);
            }
        }
    }
    None
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|entry| entry.join(name))
        .find(|candidate| candidate.exists())
}

fn expand_env(path: &str) -> String {
    let mut output = path.to_string();
    for (key, value) in std::env::vars() {
        output = output.replace(&format!("%{key}%"), &value);
    }
    output
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
