//! Resolve the writable distribution artifact without trusting inherited AppImage variables.
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

pub fn executable_path() -> Result<PathBuf, String> {
    current_distribution().map(|(path, _)| path)
}

pub fn asset_key() -> Result<String, String> {
    current_distribution().map(|(_, appimage)| key_for(appimage))
}

fn key_for(appimage: bool) -> String {
    format!(
        "{}-{}{}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        if appimage { "-appimage" } else { "" }
    )
}

fn current_distribution() -> Result<(PathBuf, bool), String> {
    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate the running executable: {error}"))?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    let appdir = std::env::var_os("APPDIR").map(PathBuf::from);
    resolve(&executable, appimage.as_deref(), appdir.as_deref())
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
