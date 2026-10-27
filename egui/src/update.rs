use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const PUBLIC_KEY: &str = include_str!("../update-pubkey.txt");
const MANIFEST_PREFIX: &str = "opsterm-update-v1";
const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/Setiyoaryo/ops-term/releases/latest/download/manifest.json";
const MAX_ASSET_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub assets: HashMap<String, Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
pub struct Available {
    pub version: Version,
    pub notes: String,
    pub asset_key: String,
    pub asset: Asset,
    pub rebuilt: bool,
}

#[derive(Debug, Clone)]
enum Source {
    Remote(String),
    Local(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct UpdateConfig {
    manifest_url: String,
    auto_check: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            manifest_url: DEFAULT_MANIFEST_URL.into(),
            auto_check: true,
        }
    }
}

pub struct Updater {
    pub manifest_url: String,
    pub auto_check: bool,
    pub show: bool,
    pub message: String,
    pub error: Option<String>,
    pub available: Option<Available>,
    pub installed: Option<PathBuf>,
    pub checking: bool,
    pub installing: bool,
    pub last_check: Option<Instant>,
    started: Instant,
    checked_once: bool,
    interactive: bool,
    check_rx: Option<Receiver<Result<Option<Available>, String>>>,
    install_rx: Option<Receiver<Result<PathBuf, String>>>,
}

impl Updater {
    pub fn new() -> Self {
        let config = load_config();
        let override_url = std::env::var("OPSTERM_UPDATE_URL").ok();
        let manifest_url = resolve_update_url(&config.manifest_url, override_url.as_deref());
        Self {
            manifest_url,
            auto_check: config.auto_check,
            show: false,
            message: String::new(),
            error: None,
            available: None,
            installed: None,
            checking: false,
            installing: false,
            last_check: None,
            started: Instant::now(),
            checked_once: false,
            interactive: false,
            check_rx: None,
            install_rx: None,
        }
    }

    pub fn configured(&self) -> bool {
        !self.manifest_url.trim().is_empty() && !public_key_text().is_empty()
    }

    pub fn busy(&self) -> bool {
        self.checking || self.installing
    }

    pub fn maybe_auto_check(&mut self) {
        if self.checked_once || self.started.elapsed() < Duration::from_secs(3) {
            return;
        }
        self.checked_once = true;
        if self.auto_check && !self.manifest_url.trim().is_empty() {
            self.start_check(false);
        }
    }

    pub fn start_check(&mut self, interactive: bool) {
        if self.busy() {
            return;
        }
        self.show |= interactive;
        self.interactive = interactive;
        self.error = None;
        self.installed = None;
        self.available = None;
        let url = self.manifest_url.trim().to_string();
        if url.is_empty() {
            self.error = Some("Update URL is not configured.".into());
            self.show |= interactive;
            return;
        }
        if public_key_text().is_empty() {
            self.error = Some("This build has no update public key.".into());
            self.show |= interactive;
            return;
        }
        if let Err(error) = manifest_source(&url) {
            self.error = Some(error);
            self.show |= interactive;
            return;
        }
        self.checking = true;
        self.message = "Checking for updates…".into();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(check_remote(&url));
        });
        self.check_rx = Some(rx);
    }

    pub fn start_install(&mut self) {
        if self.installing {
            return;
        }
        let Some(available) = self.available.clone() else {
            return;
        };
        let executable = match install_target(&available) {
            Ok(path) => path,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        self.installing = true;
        self.error = None;
        self.installed = None;
        self.message = format!("Downloading and verifying v{}…", available.version);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = download_and_verify(&available)
                .and_then(|bytes| install_bytes(&executable, &bytes));
            let _ = tx.send(result);
        });
        self.install_rx = Some(rx);
    }

    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = self.check_rx.take() {
            match rx.try_recv() {
                Ok(Ok(Some(available))) => {
                    self.checking = false;
                    self.last_check = Some(Instant::now());
                    self.message = format!("Update v{} is available.", available.version);
                    self.available = Some(available);
                    self.error = None;
                    self.show = true;
                    changed = true;
                }
                Ok(Ok(None)) => {
                    self.checking = false;
                    self.last_check = Some(Instant::now());
                    self.message = format!("OpsTerm v{CURRENT_VERSION} is up to date.");
                    self.available = None;
                    self.error = None;
                    self.show |= self.interactive;
                    changed = true;
                }
                Ok(Err(error)) => {
                    self.checking = false;
                    self.last_check = Some(Instant::now());
                    self.message.clear();
                    self.error = Some(error);
                    self.show |= self.interactive;
                    changed = true;
                }
                Err(TryRecvError::Empty) => {
                    self.check_rx = Some(rx);
                }
                Err(TryRecvError::Disconnected) => {
                    self.checking = false;
                    self.error = Some("Update check stopped unexpectedly.".into());
                    self.show |= self.interactive;
                    changed = true;
                }
            }
        }
        if let Some(rx) = self.install_rx.take() {
            match rx.try_recv() {
                Ok(Ok(path)) => {
                    self.installing = false;
                    self.message = "Update installed. Restart OpsTerm to use it.".into();
                    self.installed = Some(path);
                    self.error = None;
                    self.show = true;
                    changed = true;
                }
                Ok(Err(error)) => {
                    self.installing = false;
                    self.message.clear();
                    self.error = Some(error);
                    self.show = true;
                    changed = true;
                }
                Err(TryRecvError::Empty) => {
                    self.install_rx = Some(rx);
                }
                Err(TryRecvError::Disconnected) => {
                    self.installing = false;
                    self.error = Some("Update installer stopped unexpectedly.".into());
                    self.show = true;
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn save_config(&mut self) {
        let config = UpdateConfig {
            manifest_url: self.manifest_url.trim().to_string(),
            auto_check: self.auto_check,
        };
        if let Ok(text) = serde_json::to_string_pretty(&config) {
            if let Some(parent) = config_path().parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(config_path(), text);
        }
    }
}

pub fn restart(executable: &Path) -> Result<(), String> {
    let appimage = crate::distribution::is_appimage_file(executable)?;
    if restart_command(executable, true, appimage).spawn().is_ok() {
        return Ok(());
    }
    restart_command(executable, false, appimage)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not restart OpsTerm: {error}"))
}

fn restart_command(executable: &Path, detached: bool, appimage: bool) -> Command {
    let mut command = if detached {
        let mut command = Command::new("setsid");
        command.arg(executable);
        command
    } else {
        Command::new(executable)
    };
    if appimage {
        // Restart must also work when the original launch used extraction mode
        // explicitly on a machine with no usable FUSE runtime.
        command.arg("--appimage-extract-and-run");
        // The previous mount/extraction can disappear after replacement. Let the
        // new image reconstruct its own runtime context, preserving display,
        // agent sockets, and every unrelated environment variable.
        for name in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
            command.env_remove(name);
        }
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn public_key_text() -> &'static str {
    PUBLIC_KEY.trim()
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("org.opsterm.app").join("update.json")
}

fn load_config() -> UpdateConfig {
    fs::read(config_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn allow_http() -> bool {
    matches!(
        std::env::var("OPSTERM_UPDATE_ALLOW_HTTP").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

fn parse_source(value: &str) -> Result<Source, String> {
    parse_source_with(value, allow_http())
}

fn parse_source_with(value: &str, allow_plain_http: bool) -> Result<Source, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("update location is empty".into());
    }
    if value.starts_with('/') {
        return Ok(Source::Local(PathBuf::from(value)));
    }
    let url = Url::parse(value).map_err(|error| format!("invalid update location: {error}"))?;
    match url.scheme() {
        "https" | "http" if url.scheme() == "https" || allow_plain_http => {
            if url.host_str().is_none() {
                return Err("update URL has no host".into());
            }
            if !url.username().is_empty() || url.password().is_some() {
                return Err("update URL must not contain credentials".into());
            }
            Ok(Source::Remote(value.to_string()))
        }
        "file" => {
            let path = url
                .to_file_path()
                .map_err(|_| "invalid local update path".to_string())?;
            Ok(Source::Local(path))
        }
        _ => Err("update locations must use HTTPS or file://".into()),
    }
}

fn manifest_source(value: &str) -> Result<Source, String> {
    let source = parse_source(value)?;
    match source {
        Source::Local(path) if path.is_dir() => Ok(Source::Local(path.join("manifest.json"))),
        source => Ok(source),
    }
}

fn curl_protocol_args() -> Vec<&'static str> {
    if allow_http() {
        vec!["--proto", "=http,https", "--proto-redir", "=http,https"]
    } else {
        vec!["--proto", "=https", "--proto-redir", "=https"]
    }
}

fn curl(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let user_agent = format!("OpsTerm/{CURRENT_VERSION}");
    let limit = max_bytes.to_string();
    let mut command = Command::new("curl");
    command.args([
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--retry",
        "2",
        "--retry-delay",
        "1",
        "--max-time",
        "120",
        "--max-filesize",
        &limit,
        "--user-agent",
        &user_agent,
    ]);
    command.args(curl_protocol_args());
    command.arg("--output").arg("-").arg(url);
    let output = command.output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "curl is required for updates but was not found".to_string()
        } else {
            format!("could not run curl: {error}")
        }
    })?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        let message = message.trim();
        let message = message.chars().take(500).collect::<String>();
        return Err(format!("download failed: {message}"));
    }
    Ok(output.stdout)
}

fn fetch_manifest(source: &Source) -> Result<Manifest, String> {
    let bytes = match source {
        Source::Remote(url) => curl(url, 1024 * 1024)?,
        Source::Local(path) => {
            fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?
        }
    };
    if bytes.len() > 1024 * 1024 {
        return Err("update manifest is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("invalid update manifest: {error}"))
}

fn parse_public_key(value: &str) -> Result<VerifyingKey, String> {
    let bytes = BASE64
        .decode(value.trim())
        .map_err(|error| format!("invalid update public key: {error}"))?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "invalid update public key length".to_string())?;
    VerifyingKey::from_bytes(&bytes).map_err(|error| format!("invalid update public key: {error}"))
}

fn canonical(version: &str, key: &str, asset: &Asset) -> String {
    format!(
        "{MANIFEST_PREFIX}\n{version}\n{key}\n{}\n{}\n{}",
        asset.size,
        asset.sha256.to_ascii_lowercase(),
        asset.url
    )
}

fn verify_signature(
    version: &str,
    key: &str,
    asset: &Asset,
    verifying_key: &VerifyingKey,
) -> Result<(), String> {
    let signature = BASE64
        .decode(asset.signature.trim())
        .map_err(|error| format!("invalid update signature: {error}"))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|error| format!("invalid update signature: {error}"))?;
    verifying_key
        .verify(canonical(version, key, asset).as_bytes(), &signature)
        .map_err(|_| "update signature verification failed".to_string())
}

fn resolve_update_url(saved: &str, override_url: Option<&str>) -> String {
    override_url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .or_else(|| (!saved.trim().is_empty()).then_some(saved.trim()))
        .unwrap_or(DEFAULT_MANIFEST_URL)
        .to_string()
}

fn should_offer_version(current: &Version, remote: &Version, local_development: bool) -> bool {
    remote > current || (local_development && remote == current)
}

fn check_remote(url: &str) -> Result<Option<Available>, String> {
    let source = manifest_source(url)?;
    let manifest = fetch_manifest(&source)?;
    let key = crate::distribution::asset_key()?;
    let asset = manifest
        .assets
        .get(&key)
        .cloned()
        .ok_or_else(|| format!("update manifest has no asset for {key}"))?;
    validate_asset(&asset)?;
    let verifying_key = parse_public_key(public_key_text())?;
    verify_signature(&manifest.version, &key, &asset, &verifying_key)?;
    let current = Version::parse(CURRENT_VERSION)
        .map_err(|error| format!("invalid current version: {error}"))?;
    let remote = Version::parse(&manifest.version)
        .map_err(|error| format!("invalid manifest version: {error}"))?;
    if !should_offer_version(&current, &remote, matches!(source, Source::Local(_))) {
        return Ok(None);
    }
    let rebuilt = remote == current;
    if rebuilt && installed_matches(&asset) {
        return Ok(None);
    }
    Ok(Some(Available {
        version: remote,
        notes: manifest.notes,
        asset_key: key,
        asset,
        rebuilt,
    }))
}

fn installed_matches(asset: &Asset) -> bool {
    let Ok(executable) = crate::distribution::executable_path() else {
        return false;
    };
    installed_path_matches(&executable, asset)
}

fn installed_path_matches(executable: &Path, asset: &Asset) -> bool {
    let Ok(metadata) = fs::metadata(executable) else {
        return false;
    };
    if metadata.len() != asset.size {
        return false;
    }
    let Ok(bytes) = fs::read(executable) else {
        return false;
    };
    hex::encode(Sha256::digest(bytes)).eq_ignore_ascii_case(&asset.sha256)
}

fn validate_asset(asset: &Asset) -> Result<(), String> {
    parse_source(&asset.url)?;
    if asset.size == 0 || asset.size > MAX_ASSET_BYTES {
        return Err("update asset has an invalid size".into());
    }
    if asset.sha256.len() != 64 || !asset.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("update asset has an invalid SHA-256".into());
    }
    let signature = BASE64
        .decode(asset.signature.trim())
        .map_err(|error| format!("invalid update signature: {error}"))?;
    if signature.len() != 64 {
        return Err("update asset has an invalid signature length".into());
    }
    Ok(())
}

fn download_and_verify(available: &Available) -> Result<Vec<u8>, String> {
    validate_asset(&available.asset)?;
    let verifying_key = parse_public_key(public_key_text())?;
    verify_signature(
        &available.version.to_string(),
        &available.asset_key,
        &available.asset,
        &verifying_key,
    )?;
    let source = parse_source(&available.asset.url)?;
    let bytes = match source {
        Source::Remote(url) => curl(&url, available.asset.size + 1)?,
        Source::Local(path) => {
            let metadata = fs::metadata(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            if metadata.len() != available.asset.size {
                return Err("local update has the wrong size".into());
            }
            fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?
        }
    };
    if bytes.len() as u64 != available.asset.size {
        return Err("downloaded update has the wrong size".into());
    }
    let digest = hex::encode(Sha256::digest(&bytes));
    if !digest.eq_ignore_ascii_case(&available.asset.sha256) {
        return Err("downloaded update failed its SHA-256 check".into());
    }
    validate_executable_format(&bytes, &available.asset_key)?;
    Ok(bytes)
}

fn validate_executable_format(bytes: &[u8], asset_key: &str) -> Result<(), String> {
    if !bytes.starts_with(b"\x7fELF") {
        return Err("downloaded update is not a Linux executable".into());
    }
    let appimage = crate::distribution::is_appimage_bytes(bytes);
    if appimage != asset_key.ends_with("-appimage") {
        return Err("downloaded update format does not match its signed asset key".into());
    }
    Ok(())
}

fn install_target(available: &Available) -> Result<PathBuf, String> {
    if available.asset_key != crate::distribution::asset_key()? {
        return Err("update asset does not match the running distribution".into());
    }
    crate::distribution::executable_path()
}

pub fn cli(args: &[String]) -> Option<i32> {
    let install = match args.first().map(String::as_str) {
        Some("--check-update") => false,
        Some("--install-update") => true,
        _ => return None,
    };
    let updater = Updater::new();
    let url = updater.manifest_url.trim();
    if url.is_empty() {
        eprintln!("update: set OPSTERM_UPDATE_URL or configure Help > Check for updates");
        return Some(2);
    }
    match check_remote(url) {
        Ok(None) => {
            println!("OpsTerm v{CURRENT_VERSION} is up to date");
            Some(0)
        }
        Ok(Some(available)) => {
            println!(
                "{} {} is available ({:.1} MiB)",
                if available.rebuilt {
                    "new build of"
                } else {
                    "update"
                },
                available.version,
                available.asset.size as f64 / 1_048_576.0
            );
            if !available.notes.trim().is_empty() {
                println!("{}", available.notes);
            }
            if !install {
                return Some(0);
            }
            let executable = match install_target(&available) {
                Ok(path) => path,
                Err(error) => {
                    eprintln!("update: {error}");
                    return Some(2);
                }
            };
            match download_and_verify(&available)
                .and_then(|bytes| install_bytes(&executable, &bytes))
            {
                Ok(path) => {
                    println!("installed {}", path.display());
                    Some(0)
                }
                Err(error) => {
                    eprintln!("update: {error}");
                    Some(2)
                }
            }
        }
        Err(error) => {
            eprintln!("update: {error}");
            Some(2)
        }
    }
}

fn install_bytes(executable: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(executable)
        .map_err(|error| format!("cannot inspect the running executable: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("the running executable is not a regular file".into());
    }
    let parent = executable
        .parent()
        .ok_or_else(|| "the running executable has no parent directory".to_string())?;
    let name = executable
        .file_name()
        .ok_or_else(|| "the running executable has no file name".to_string())?
        .to_string_lossy()
        .into_owned();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let temporary = parent.join(format!(".{name}.new.{}.{stamp}", std::process::id()));
    let backup = parent.join(format!(".{name}.old"));

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o755);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| format!("cannot write the update next to the executable: {error}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot write the update: {error}"));
    }
    drop(file);

    let _ = fs::remove_file(&backup);
    if let Err(error) = fs::hard_link(executable, &backup) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot create a rollback copy: {error}"));
    }
    if let Err(error) = fs::rename(&temporary, executable) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("cannot replace the executable: {error}"));
    }
    Ok(executable.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_asset(version: &str, key: &str, bytes: &[u8]) -> (Asset, VerifyingKey) {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let hash = hex::encode(Sha256::digest(bytes));
        let mut asset = Asset {
            url: "https://example.invalid/opsterm".into(),
            size: bytes.len() as u64,
            sha256: hash,
            signature: String::new(),
        };
        let signature = signing.sign(canonical(version, key, &asset).as_bytes());
        asset.signature = BASE64.encode(signature.to_bytes());
        (asset, signing.verifying_key())
    }

    #[test]
    fn public_feed_only_offers_newer_versions_but_file_development_can_rebuild() {
        let current = Version::parse("0.1.1").unwrap();
        assert!(!should_offer_version(&current, &current, false));
        assert!(should_offer_version(&current, &current, true));
        assert!(should_offer_version(
            &current,
            &Version::parse("0.1.2").unwrap(),
            false
        ));
        assert!(!should_offer_version(
            &current,
            &Version::parse("0.1.0").unwrap(),
            true
        ));
    }

    #[test]
    fn feed_override_and_custom_local_feed_are_preserved() {
        assert_eq!(
            resolve_update_url("file:///tmp/manifest.json", None),
            "file:///tmp/manifest.json"
        );
        assert_eq!(
            resolve_update_url(
                "https://example.invalid/feed",
                Some("file:///tmp/test.json")
            ),
            "file:///tmp/test.json"
        );
        assert_eq!(
            resolve_update_url("", Some("  ")),
            UpdateConfig::default().manifest_url
        );
    }

    #[test]
    fn public_release_feed_is_configured_by_default() {
        assert_eq!(
            UpdateConfig::default().manifest_url,
            "https://github.com/Setiyoaryo/ops-term/releases/latest/download/manifest.json"
        );
        assert!(UpdateConfig::default().auto_check);
    }

    #[test]
    fn partial_update_config_keeps_the_disabled_auto_check_choice() {
        let config: UpdateConfig = serde_json::from_str(r#"{"auto_check":false}"#).unwrap();
        assert!(!config.auto_check);
        assert_eq!(config.manifest_url, UpdateConfig::default().manifest_url);
    }

    #[test]
    fn verifies_a_signed_manifest() {
        let (asset, key) = signed_asset("1.2.3", "linux-x86_64", b"binary");
        assert!(verify_signature("1.2.3", "linux-x86_64", &asset, &key).is_ok());
        assert!(verify_signature("1.2.4", "linux-x86_64", &asset, &key).is_err());
    }

    #[test]
    fn appimage_asset_key_is_part_of_the_signature() {
        let bytes = b"\x7fELF\0\0\0\0AI\x02payload";
        let (asset, key) = signed_asset("1.2.3", "linux-x86_64-appimage", bytes);
        assert!(verify_signature("1.2.3", "linux-x86_64-appimage", &asset, &key).is_ok());
        assert!(verify_signature("1.2.3", "linux-x86_64", &asset, &key).is_err());
    }

    #[test]
    fn signed_asset_format_cannot_cross_distribution_types() {
        let image = b"\x7fELF\0\0\0\0AI\x02payload";
        let raw = b"\x7fELFbinary";
        assert!(validate_executable_format(image, "linux-x86_64-appimage").is_ok());
        assert!(validate_executable_format(raw, "linux-x86_64").is_ok());
        assert!(validate_executable_format(raw, "linux-x86_64-appimage").is_err());
        assert!(validate_executable_format(image, "linux-x86_64").is_err());
        assert!(validate_executable_format(b"not ELF!AI\x02", "linux-x86_64-appimage").is_err());
    }

    #[test]
    fn appimage_restart_clears_only_old_runtime_context_for_both_launchers() {
        for detached in [false, true] {
            let command = restart_command(Path::new("/tmp/OpsTerm.AppImage"), detached, true);
            let expected_args: Vec<&std::ffi::OsStr> = if detached {
                vec![
                    "/tmp/OpsTerm.AppImage".as_ref(),
                    "--appimage-extract-and-run".as_ref(),
                ]
            } else {
                vec!["--appimage-extract-and-run".as_ref()]
            };
            assert_eq!(command.get_args().collect::<Vec<_>>(), expected_args);
            let changes: Vec<_> = command.get_envs().collect();
            assert_eq!(changes.len(), 4);
            for name in ["APPIMAGE", "APPDIR", "ARGV0", "OWD"] {
                assert!(changes
                    .iter()
                    .any(|(key, value)| *key == name && value.is_none()));
            }
            let raw = restart_command(Path::new("/tmp/opsterm"), detached, false);
            assert_eq!(raw.get_envs().count(), 0);
            let expected_raw_args: Vec<&std::ffi::OsStr> = if detached {
                vec!["/tmp/opsterm".as_ref()]
            } else {
                vec![]
            };
            assert_eq!(raw.get_args().collect::<Vec<_>>(), expected_raw_args);
        }
    }

    #[test]
    fn appimage_install_replaces_outer_image_and_hashes_it_for_rebuild_checks() {
        let directory = std::env::temp_dir().join(format!(
            "opsterm-image-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(directory.join("mount/usr/bin")).unwrap();
        let mounted = directory.join("mount/usr/bin/opsterm");
        fs::write(&mounted, b"mounted ELF executable").unwrap();
        let outer = directory.join("OpsTerm.AppImage");
        let old = b"\x7fELF\0\0\0\0AI\x02old image";
        let new = b"\x7fELF\0\0\0\0AI\x02new image";
        fs::write(&outer, old).unwrap();
        let (asset, _) = signed_asset("1.2.3", "linux-x86_64-appimage", new);
        assert!(!installed_path_matches(&outer, &asset));
        assert_eq!(install_bytes(&outer, new).unwrap(), outer);
        assert!(installed_path_matches(&outer, &asset));
        assert!(!installed_path_matches(&mounted, &asset));
        assert_eq!(fs::read(&mounted).unwrap(), b"mounted ELF executable");
        assert_eq!(
            fs::read(directory.join(".OpsTerm.AppImage.old")).unwrap(),
            old
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn accepts_https_and_local_paths_but_not_plain_http() {
        assert!(parse_source_with("https://example.com/a", false).is_ok());
        assert!(parse_source_with("http://example.com/a", false).is_err());
        assert!(parse_source_with("http://127.0.0.1/a", true).is_ok());
        assert!(parse_source_with("file:///tmp/a", false).is_ok());
        assert!(parse_source_with("/tmp/a", false).is_ok());
        assert!(parse_source_with("ftp://example.com/a", true).is_err());
    }

    #[test]
    fn installs_and_keeps_a_rollback_copy() {
        let directory = std::env::temp_dir().join(format!(
            "opsterm-update-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let executable = directory.join("opsterm-egui");
        fs::write(&executable, b"old").unwrap();
        let new_bytes = b"\x7fELFnew";
        let path = install_bytes(&executable, new_bytes).unwrap();
        assert_eq!(path, executable);
        assert_eq!(fs::read(&executable).unwrap(), new_bytes);
        assert_eq!(
            fs::read(directory.join(".opsterm-egui.old")).unwrap(),
            b"old"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn refuses_to_replace_a_symlink() {
        let directory = std::env::temp_dir().join(format!(
            "opsterm-update-link-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let target = directory.join("target");
        let link = directory.join("link");
        fs::write(&target, b"old").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(install_bytes(&link, b"\x7fELFnew").is_err());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn rejects_bad_public_keys() {
        assert!(parse_public_key("not base64").is_err());
        assert!(parse_public_key("AAAA").is_err());
    }
}
