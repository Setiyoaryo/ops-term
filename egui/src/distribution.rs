//! Resolve the writable distribution artifact without trusting inherited AppImage variables.
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

static LAUNCH_DISTRIBUTION: OnceLock<Result<DistributionSnapshot, String>> = OnceLock::new();

/// Capture launch context before another instance can replace this executable.
/// An invalid context disables updating without preventing normal GUI startup.
pub fn initialize() {
    let _ = current_distribution();
}

pub fn executable_path() -> Result<PathBuf, String> {
    current_distribution().map(|distribution| distribution.path.clone())
}

pub fn asset_key() -> Result<String, String> {
    current_distribution().map(|distribution| key_for(distribution.appimage))
}

/// Refuse writes if the artifact captured at launch has disappeared or changed.
pub fn validate_install_target(path: &Path) -> Result<(), String> {
    current_distribution()?.validate_install_target(path)
}

fn key_for(appimage: bool) -> String {
    format!(
        "{}-{}{}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if appimage { "-appimage" } else { "" }
    )
}

fn current_distribution() -> Result<&'static DistributionSnapshot, String> {
    LAUNCH_DISTRIBUTION
        .get_or_init(|| {
            let executable = std::env::current_exe()
                .map_err(|error| format!("cannot locate the running executable: {error}"))?;
            let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
            let appdir = std::env::var_os("APPDIR").map(PathBuf::from);
            DistributionSnapshot::capture(&executable, appimage.as_deref(), appdir.as_deref())
        })
        .as_ref()
        .map_err(Clone::clone)
}

struct DistributionSnapshot {
    path: PathBuf,
    appimage: bool,
    identity: FileIdentity,
}

impl DistributionSnapshot {
    fn capture(
        executable: &Path,
        appimage: Option<&Path>,
        appdir: Option<&Path>,
    ) -> Result<Self, String> {
        let (path, appimage) = resolve(executable, appimage, appdir)?;
        let identity = FileIdentity::at(&path)?;
        Ok(Self {
            path,
            appimage,
            identity,
        })
    }

    fn validate_install_target(&self, path: &Path) -> Result<(), String> {
        if path != self.path {
            return Err("install target differs from the distribution captured at launch".into());
        }
        if FileIdentity::at(path)? != self.identity {
            return Err(
                "Installed distribution changed since launch. Reopen OpsTerm and check for updates again.".into(),
            );
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
struct FileIdentity {
    size: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed: (i64, i64),
}

impl FileIdentity {
    fn at(path: &Path) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("cannot inspect distribution {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("distribution must be a regular file, not a symlink".into());
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            size: metadata.len(),
            modified: metadata.modified().map_err(|error| {
                format!("cannot inspect distribution modification time: {error}")
            })?,
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
            #[cfg(unix)]
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

// All context is supplied explicitly so tests never race on process environment.
fn resolve(
    executable: &Path,
    appimage: Option<&Path>,
    appdir: Option<&Path>,
) -> Result<(PathBuf, bool), String> {
    if !executable.is_absolute() {
        return Err("the running executable path must be absolute".into());
    }
    let (image, directory) = match (appimage, appdir) {
        (None, None) => return Ok((executable.to_path_buf(), false)),
        (Some(image), Some(directory)) => (image, directory),
        _ => {
            return Err("incomplete AppImage environment: APPIMAGE and APPDIR are required".into())
        }
    };
    if !image.is_absolute() || !directory.is_absolute() {
        return Err("APPIMAGE and APPDIR must be absolute paths".into());
    }
    let directory =
        fs::canonicalize(directory).map_err(|error| format!("cannot resolve APPDIR: {error}"))?;
    if !directory.is_dir() {
        return Err("APPDIR is not a directory".into());
    }
    let executable = fs::canonicalize(executable)
        .map_err(|error| format!("cannot resolve the running executable: {error}"))?;
    if !executable.starts_with(&directory) || executable == directory {
        return Err(
            "the running executable is outside APPDIR; refusing stale AppImage environment".into(),
        );
    }
    if !is_appimage_file(image)? {
        return Err("APPIMAGE is not a type-2 AppImage".into());
    }
    // Canonicalize only after rejecting a symlink at the supplied image path.
    let image =
        fs::canonicalize(image).map_err(|error| format!("cannot resolve APPIMAGE: {error}"))?;
    if image.starts_with(&directory) {
        return Err("APPIMAGE must be outside APPDIR".into());
    }
    Ok((image, true))
}

pub(crate) fn is_appimage_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x7fELF") && bytes.get(8..11) == Some(b"AI\x02")
}

pub(crate) fn is_appimage_file(path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("APPIMAGE must be a regular file, not a symlink".into());
    }
    let mut header = [0u8; 11];
    let mut file =
        File::open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    match file.read_exact(&mut header) {
        Ok(()) => Ok(is_appimage_bytes(&header)),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => Err(format!("cannot read AppImage header: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    #[cfg(target_os = "linux")]
    fn launch_path_survives_atomic_executable_replacement() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        const CHILD_ROOT: &str = "OPSTERM_DISTRIBUTION_REPLACEMENT_TEST";
        fn wait_for(path: &Path) -> bool {
            let deadline = Instant::now() + Duration::from_secs(15);
            while Instant::now() < deadline {
                if path.exists() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            false
        }

        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let root = PathBuf::from(root);
            let original = executable_path().unwrap();
            assert_eq!(original, root.join("running-test"));
            fs::write(root.join("ready"), b"ready").unwrap();
            assert!(
                wait_for(&root.join("replaced")),
                "parent did not replace executable"
            );
            assert_eq!(executable_path().unwrap(), original);
            assert!(validate_install_target(&original).is_err());
            return;
        }

        let fixture = Fixture::new();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o700)).unwrap();
        let running = fixture.0.join("running-test");
        fs::copy(std::env::current_exe().unwrap(), &running).unwrap();
        let mut child = Command::new(&running)
            .args([
                "--exact",
                "distribution::tests::launch_path_survives_atomic_executable_replacement",
                "--nocapture",
            ])
            .env(CHILD_ROOT, &fixture.0)
            .env_remove("APPIMAGE")
            .env_remove("APPDIR")
            .spawn()
            .unwrap();
        if !wait_for(&fixture.0.join("ready")) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("private test process did not capture its launch path");
        }
        let replacement = fixture.0.join("replacement");
        fs::write(&replacement, b"replacement executable").unwrap();
        fs::rename(&replacement, &running).unwrap();
        let proc_exe = fs::read_link(format!("/proc/{}/exe", child.id())).unwrap();
        assert!(proc_exe
            .as_os_str()
            .as_encoded_bytes()
            .ends_with(b" (deleted)"));
        fs::write(fixture.0.join("replaced"), b"replaced").unwrap();
        assert!(
            child.wait().unwrap().success(),
            "replaced executable lost its launch path"
        );
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "opsterm-distribution-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(path.join("mount/usr/bin")).unwrap();
            fs::write(path.join("mount/usr/bin/opsterm"), b"binary").unwrap();
            fs::write(
                path.join("OpsTerm.AppImage"),
                b"\x7fELF\x02\x01\x01\x00AI\x02payload",
            )
            .unwrap();
            Self(path)
        }
        fn executable(&self) -> PathBuf {
            self.0.join("mount/usr/bin/opsterm")
        }
        fn image(&self) -> PathBuf {
            self.0.join("OpsTerm.AppImage")
        }
        fn mount(&self) -> PathBuf {
            self.0.join("mount")
        }
        fn resolve(&self) -> Result<(PathBuf, bool), String> {
            resolve(&self.executable(), Some(&self.image()), Some(&self.mount()))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn raw_executable_and_manifest_key_are_unchanged() {
        let f = Fixture::new();
        assert_eq!(
            resolve(&f.executable(), None, None).unwrap(),
            (f.executable(), false)
        );
        assert_eq!(
            key_for(false),
            format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
        );
        assert_eq!(key_for(true), format!("{}-appimage", key_for(false)));
    }

    #[test]
    fn captured_native_identity_accepts_only_unchanged_target() {
        let f = Fixture::new();
        let snapshot = DistributionSnapshot::capture(&f.executable(), None, None).unwrap();
        assert_eq!(snapshot.path, f.executable());
        assert!(!snapshot.appimage);
        assert!(snapshot.validate_install_target(&f.executable()).is_ok());
        assert!(snapshot.validate_install_target(&f.image()).is_err());

        fs::write(f.executable(), b"modified executable").unwrap();
        assert!(snapshot.validate_install_target(&f.executable()).is_err());
        fs::remove_file(f.executable()).unwrap();
        assert!(snapshot.validate_install_target(&f.executable()).is_err());
        assert!(DistributionSnapshot::capture(&f.executable(), None, None).is_err());
    }

    #[test]
    fn captured_identity_refuses_same_size_atomic_replacement() {
        let f = Fixture::new();
        let snapshot = DistributionSnapshot::capture(&f.executable(), None, None).unwrap();
        let replacement = f.0.join("replacement");
        fs::copy(f.executable(), &replacement).unwrap();
        fs::rename(replacement, f.executable()).unwrap();
        assert!(snapshot.validate_install_target(&f.executable()).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn captured_identity_refuses_symlink_swap_and_initial_symlink() {
        let f = Fixture::new();
        let snapshot = DistributionSnapshot::capture(&f.executable(), None, None).unwrap();
        let moved = f.0.join("original-executable");
        fs::rename(f.executable(), &moved).unwrap();
        std::os::unix::fs::symlink(moved, f.executable()).unwrap();
        assert!(snapshot.validate_install_target(&f.executable()).is_err());
        assert!(DistributionSnapshot::capture(&f.executable(), None, None).is_err());
    }

    #[test]
    fn captured_appimage_context_survives_unmounted_appdir() {
        let f = Fixture::new();
        let snapshot =
            DistributionSnapshot::capture(&f.executable(), Some(&f.image()), Some(&f.mount()))
                .unwrap();
        fs::remove_dir_all(f.mount()).unwrap();
        assert_eq!(snapshot.path, fs::canonicalize(f.image()).unwrap());
        assert!(snapshot.appimage);
        assert_eq!(
            key_for(snapshot.appimage),
            format!("{}-appimage", key_for(false))
        );
        assert!(snapshot.validate_install_target(&f.image()).is_ok());
        fs::remove_file(f.image()).unwrap();
        assert!(snapshot.validate_install_target(&f.image()).is_err());
    }

    #[test]
    fn appimage_targets_outer_file() {
        let f = Fixture::new();
        assert_eq!(
            f.resolve().unwrap(),
            (fs::canonicalize(f.image()).unwrap(), true)
        );
    }

    #[test]
    fn rejects_partial_relative_missing_and_stale_environment() {
        let f = Fixture::new();
        assert!(resolve(&f.executable(), Some(&f.image()), None).is_err());
        assert!(resolve(&f.executable(), None, Some(&f.mount())).is_err());
        assert!(resolve(&f.executable(), Some(Path::new("image")), Some(&f.mount())).is_err());
        assert!(resolve(&f.executable(), Some(&f.image()), Some(Path::new("mount"))).is_err());
        assert!(resolve(Path::new("binary"), None, None).is_err());
        let outside = f.0.join("raw");
        fs::write(&outside, b"binary").unwrap();
        assert!(resolve(&outside, Some(&f.image()), Some(&f.mount())).is_err());
        fs::remove_file(f.image()).unwrap();
        assert!(f.resolve().is_err());
        fs::remove_dir_all(f.mount()).unwrap();
        assert!(f.resolve().is_err());
    }

    #[test]
    fn rejects_wrong_format_directories_and_image_symlinks() {
        let f = Fixture::new();
        for bytes in [
            b"\x7fELFraw".as_slice(),
            b"\x7fELF\0\0\0\0AI\x01",
            b"not ELF!AI\x02",
        ] {
            fs::write(f.image(), bytes).unwrap();
            assert!(f.resolve().is_err());
        }
        fs::remove_file(f.image()).unwrap();
        fs::create_dir(f.image()).unwrap();
        assert!(f.resolve().is_err());
        fs::remove_dir(f.image()).unwrap();
        #[cfg(unix)]
        {
            let target = f.0.join("real-image");
            fs::write(&target, b"\x7fELF\0\0\0\0AI\x02").unwrap();
            std::os::unix::fs::symlink(target, f.image()).unwrap();
            assert!(f.resolve().is_err());
        }
    }

    #[test]
    #[cfg(unix)]
    fn executable_symlink_cannot_escape_appdir() {
        let f = Fixture::new();
        let outside = f.0.join("raw");
        fs::write(&outside, b"binary").unwrap();
        fs::remove_file(f.executable()).unwrap();
        std::os::unix::fs::symlink(outside, f.executable()).unwrap();
        assert!(f.resolve().is_err());
    }
}
