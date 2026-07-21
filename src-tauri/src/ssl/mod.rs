use crate::state::{AppResult, CertificateInfo, LogLevel, Store};
use chrono::{Duration, Utc};
use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SanType,
};
use std::{
    ffi::OsStr,
    fs,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const LOCAL_CA_NAME: &str = "LocalStack Pro Local CA";
const LOCAL_CA_CERT_FILE: &str = "localstack-pro-ca.crt";
const LOCAL_CA_KEY_FILE: &str = "localstack-pro-ca.key";

pub fn generate_certificate(
    domain: String,
    san_domains: Vec<String>,
) -> AppResult<crate::state::AppSnapshot> {
    let domain = domain.trim().to_lowercase();
    if !domain.contains('.') || domain.contains(' ') {
        return Err("Certificate domain must be a valid local domain.".to_string());
    }
    let mut san_domains = san_domains
        .into_iter()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    if !san_domains.iter().any(|name| name == &domain) {
        san_domains.insert(0, domain.clone());
    }
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let (cert_path, key_path, pem_path) =
        write_host_certificate(&store, &domain, &san_domains, true)?;
    let ca_path = store.dir.join("certs").join(LOCAL_CA_CERT_FILE);
    let trusted = local_ca_trusted() || trust_local_ca_for_current_user(&ca_path).is_ok();
    let item = CertificateInfo {
        id: domain.clone(),
        domain: domain.clone(),
        status: "Valid".to_string(),
        trusted,
        expires_at: (Utc::now() + Duration::days(365)).to_rfc3339(),
        issuer: LOCAL_CA_NAME.to_string(),
        san_domains,
        auto_renew: true,
        cert_path: cert_path.display().to_string(),
        key_path: key_path.display().to_string(),
        pem_path: pem_path.display().to_string(),
        fingerprint: "Generated locally".to_string(),
    };
    snapshot.certificates.retain(|cert| cert.id != domain);
    snapshot.certificates.push(item);
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "SSL",
        format!("Certificate generated for {domain}"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn trust_certificate(certificate_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    snapshot
        .certificates
        .iter()
        .find(|cert| cert.id == certificate_id)
        .ok_or_else(|| "Certificate not found.".to_string())?;
    let (_, _, ca_path, _) = ensure_local_ca(&store)?;
    trust_local_ca_for_current_user(&ca_path)?;
    for cert in &mut snapshot.certificates {
        cert.trusted = true;
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "SSL",
        format!("{LOCAL_CA_NAME} trusted in Windows Certificate Store"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub(crate) fn ensure_host_certificate_trusted(domain: &str) -> AppResult<()> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    let certificate_exists = snapshot
        .certificates
        .iter()
        .any(|cert| cert.domain.eq_ignore_ascii_case(domain));
    if !certificate_exists {
        return Err(format!("Certificate for {domain} was not found."));
    }
    if !local_ca_trusted() {
        let (_, _, ca_path, _) = ensure_local_ca(&store)?;
        trust_local_ca_for_current_user(&ca_path)?;
    }
    let mut changed = false;
    for certificate in &mut snapshot.certificates {
        if !certificate.trusted {
            certificate.trusted = true;
            changed = true;
        }
    }
    if changed {
        store.log(
            &mut snapshot,
            LogLevel::Info,
            "SSL",
            format!("{LOCAL_CA_NAME} trusted in the current Windows user store"),
            None,
        );
        store.save(&snapshot)?;
    }
    Ok(())
}

pub fn revoke_certificate(certificate_id: String) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    snapshot
        .certificates
        .iter()
        .find(|cert| cert.id == certificate_id)
        .ok_or_else(|| "Certificate not found.".to_string())?;
    run_elevated_certutil(&["-delstore", "Root", LOCAL_CA_NAME])?;
    for cert in &mut snapshot.certificates {
        cert.trusted = false;
        cert.auto_renew = false;
    }
    store.log(
        &mut snapshot,
        LogLevel::Warning,
        "SSL",
        format!("{LOCAL_CA_NAME} removed from Windows Certificate Store"),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn save_certificate(certificate: CertificateInfo) -> AppResult<crate::state::AppSnapshot> {
    let store = Store::new()?;
    let mut snapshot = store.load_static()?;
    if let Some(existing) = snapshot
        .certificates
        .iter_mut()
        .find(|cert| cert.id == certificate.id)
    {
        *existing = certificate.clone();
    } else {
        snapshot.certificates.push(certificate.clone());
    }
    store.log(
        &mut snapshot,
        LogLevel::Info,
        "SSL",
        format!("Certificate {} settings saved", certificate.domain),
        None,
    );
    store.save(&snapshot)?;
    Ok(snapshot)
}

pub fn export_certificate(certificate_id: String, folder: String) -> AppResult<String> {
    let store = Store::new()?;
    let snapshot = store.load_static()?;
    let cert = snapshot
        .certificates
        .iter()
        .find(|cert| cert.id == certificate_id)
        .ok_or_else(|| "Certificate not found.".to_string())?;
    fs::create_dir_all(&folder).map_err(|err| format!("Cannot create export folder: {err}"))?;
    for path in [&cert.cert_path, &cert.key_path, &cert.pem_path] {
        let source = std::path::Path::new(path);
        let file_name = source
            .file_name()
            .ok_or_else(|| "Invalid certificate path.".to_string())?;
        fs::copy(source, std::path::Path::new(&folder).join(file_name))
            .map_err(|err| format!("Cannot export certificate file: {err}"))?;
    }
    Ok(folder)
}

pub(crate) fn ensure_host_certificate_files(
    store: &Store,
    domain: &str,
    san_domains: Vec<String>,
) -> AppResult<(PathBuf, PathBuf)> {
    let sans = normalize_san_domains(domain, san_domains);
    let (cert_path, key_path, _) = write_host_certificate(store, domain, &sans, false)?;
    Ok((cert_path, key_path))
}

fn write_host_certificate(
    store: &Store,
    domain: &str,
    san_domains: &[String],
    force: bool,
) -> AppResult<(PathBuf, PathBuf, PathBuf)> {
    fs::create_dir_all(store.dir.join("certs"))
        .map_err(|err| format!("Cannot create certs folder: {err}"))?;
    fs::create_dir_all(store.dir.join("keys"))
        .map_err(|err| format!("Cannot create keys folder: {err}"))?;
    let (ca_cert, ca_key, _, ca_created) = ensure_local_ca(store)?;
    let file_stem = certificate_file_stem(domain);
    let cert_path = store.dir.join("certs").join(format!("{file_stem}.crt"));
    let key_path = store.dir.join("keys").join(format!("{file_stem}.key"));
    let pem_path = store.dir.join("certs").join(format!("{file_stem}.pem"));
    let marker_path = store.dir.join("certs").join(format!("{file_stem}.issuer"));
    let marker = fs::read_to_string(&marker_path).unwrap_or_default();
    let should_write = force
        || ca_created
        || !cert_path.exists()
        || !key_path.exists()
        || marker.trim() != LOCAL_CA_NAME;
    if !should_write {
        return Ok((cert_path, key_path, pem_path));
    }
    let mut params: CertificateParams = Default::default();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, domain.to_string());
    params.is_ca = IsCa::ExplicitNoCa;
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    params.subject_alt_names = san_domains
        .iter()
        .map(|name| {
            name.clone()
                .try_into()
                .map(SanType::DnsName)
                .map_err(|err| format!("Invalid SAN domain {name}: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let key_pair =
        KeyPair::generate().map_err(|err| format!("Cannot create certificate key pair: {err}"))?;
    let cert = params
        .signed_by(&key_pair, &ca_cert, &ca_key)
        .map_err(|err| format!("Cannot create certificate for {domain}: {err}"))?;
    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();
    fs::write(&cert_path, &cert_pem).map_err(|err| format!("Cannot write certificate: {err}"))?;
    fs::write(&key_path, &key_pem).map_err(|err| format!("Cannot write private key: {err}"))?;
    fs::write(&pem_path, format!("{cert_pem}\n{key_pem}"))
        .map_err(|err| format!("Cannot write PEM chain: {err}"))?;
    fs::write(&marker_path, LOCAL_CA_NAME)
        .map_err(|err| format!("Cannot write certificate issuer marker: {err}"))?;
    Ok((cert_path, key_path, pem_path))
}

fn ensure_local_ca(store: &Store) -> AppResult<(Certificate, KeyPair, PathBuf, bool)> {
    fs::create_dir_all(store.dir.join("certs"))
        .map_err(|err| format!("Cannot create certs folder: {err}"))?;
    fs::create_dir_all(store.dir.join("keys"))
        .map_err(|err| format!("Cannot create keys folder: {err}"))?;
    let cert_path = store.dir.join("certs").join(LOCAL_CA_CERT_FILE);
    let key_path = store.dir.join("keys").join(LOCAL_CA_KEY_FILE);
    let mut created = false;
    let key_pair = if key_path.exists() {
        let key_pem = fs::read_to_string(&key_path)
            .map_err(|err| format!("Cannot read local CA key: {err}"))?;
        KeyPair::from_pem(&key_pem).map_err(|err| format!("Cannot parse local CA key: {err}"))?
    } else {
        created = true;
        let key_pair =
            KeyPair::generate().map_err(|err| format!("Cannot create local CA key: {err}"))?;
        fs::write(&key_path, key_pair.serialize_pem())
            .map_err(|err| format!("Cannot write local CA key: {err}"))?;
        key_pair
    };
    let cert = local_ca_params()
        .self_signed(&key_pair)
        .map_err(|err| format!("Cannot create local CA certificate: {err}"))?;
    if created || !cert_path.exists() || !certificate_subject_matches(&cert_path, LOCAL_CA_NAME) {
        fs::write(&cert_path, cert.pem())
            .map_err(|err| format!("Cannot write local CA certificate: {err}"))?;
        created = true;
    }
    Ok((cert, key_pair, cert_path, created))
}

fn local_ca_params() -> CertificateParams {
    let mut params: CertificateParams = Default::default();
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, LOCAL_CA_NAME);
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
}

fn normalize_san_domains(domain: &str, san_domains: Vec<String>) -> Vec<String> {
    let domain = domain.trim().to_lowercase();
    let mut sans = san_domains
        .into_iter()
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    if !sans.iter().any(|name| name == &domain) {
        sans.insert(0, domain);
    }
    sans.sort();
    sans.dedup();
    sans
}

fn certificate_file_stem(domain: &str) -> String {
    domain.replace('*', "wildcard").replace(':', "_")
}

fn certificate_subject_matches(path: &Path, subject: &str) -> bool {
    let path = path.display().to_string();
    let mut command = Command::new("certutil");
    command.args(["-dump", &path]).stdin(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output();
    match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).contains(subject)
        }
        _ => false,
    }
}

fn local_ca_trusted() -> bool {
    let mut command = Command::new("certutil");
    command
        .args(["-user", "-store", "Root", LOCAL_CA_NAME])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    match command.output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).contains(LOCAL_CA_NAME)
        }
        _ => false,
    }
}

fn trust_local_ca_for_current_user(ca_path: &Path) -> AppResult<()> {
    let ca_path = ca_path
        .to_str()
        .ok_or_else(|| "Local CA path is not valid Unicode.".to_string())?;
    let mut command = Command::new("certutil");
    command
        .args(["-user", "-addstore", "-f", "Root", ca_path])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .output()
        .map_err(|err| format!("Cannot trust LocalStack Pro Local CA: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot trust LocalStack Pro Local CA: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn run_elevated_certutil(args: &[&str]) -> AppResult<()> {
    let parameters = args
        .iter()
        .map(|arg| format!("\"{}\"", arg.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ");
    #[cfg(windows)]
    run_elevated_hidden("certutil.exe", &parameters)
        .map_err(|err| format!("Cannot request Windows Certificate Store elevation: {err}"))?;
    Ok(())
}

#[cfg(windows)]
fn run_elevated_hidden(executable: &str, args: &str) -> AppResult<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND};
    use windows_sys::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = wide("runas");
    let file = wide(executable);
    let params = wide(args);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: 0 as HWND,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        lpDirectory: std::ptr::null(),
        nShow: SW_HIDE,
        hInstApp: std::ptr::null_mut(),
        lpIDList: std::ptr::null_mut(),
        lpClass: std::ptr::null(),
        hkeyClass: std::ptr::null_mut(),
        dwHotKey: 0,
        Anonymous: unsafe { std::mem::zeroed() },
        hProcess: 0 as HANDLE,
    };
    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        return Err(format!("Shell elevation failed: {}", unsafe {
            GetLastError()
        }));
    }
    if !info.hProcess.is_null() {
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
