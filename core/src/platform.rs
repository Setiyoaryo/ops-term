//! Small cross-platform helpers for locations and trusted program lookup.
//!
//! Linux keeps the existing XDG behaviour. macOS uses `Application Support`
//! and Windows uses roaming/local application data, so state never lands in a
//! stray home directory that the platform tools do not expect.

use std::path::{Path, PathBuf};

fn non_empty(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|value| !value.is_empty()).map(PathBuf::from)
}

/// The user's home directory, if the platform exposes one.
pub fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        non_empty(std::env::var_os("USERPROFILE")).or_else(|| non_empty(std::env::var_os("HOME")))
    } else {
        non_empty(std::env::var_os("HOME")).or_else(|| non_empty(std::env::var_os("USERPROFILE")))
    }
}

/// Durable application data directory (database, vault, diagnostics).
pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = non_empty(std::env::var_os("LOCALAPPDATA")) {
            return base;
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            return home.join("Library/Application Support");
        }
    }
    if let Some(base) = non_empty(std::env::var_os("XDG_DATA_HOME")) {
        return base;
    }
    home_dir().unwrap_or_default().join(".local/share")
}

/// User configuration directory (settings that are not large state).
pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = non_empty(std::env::var_os("APPDATA")) {
            return base;
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = home_dir() {
            return home.join("Library/Application Support");
        }
    }
    if let Some(base) = non_empty(std::env::var_os("XDG_CONFIG_HOME")) {
        return base;
    }
    home_dir().unwrap_or_default().join(".config")
}

/// Resolve a trusted program from explicit absolute candidates, then `PATH`.
///
/// Absolute candidates are tried first so packaged builds never depend on an
/// inherited `PATH`. `PATH` is only a fallback and every entry is made absolute
/// before use, so callers still receive a real path instead of a bare name.
pub fn find_program(names: &[&str]) -> Option<PathBuf> {
    for name in names {
        let candidate = Path::new(name);
        if candidate.is_absolute() && candidate.is_file() {
            return Some(candidate.to_path_buf());
        }
    }
    let path = std::env::var_os("PATH")?;
    for name in names {
        if name.contains(std::path::MAIN_SEPARATOR) {
            continue;
        }
        for directory in std::env::split_paths(&path) {
            if directory.as_os_str().is_empty() {
                continue;
            }
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(target_os = "windows")]
            {
                let executable = directory.join(format!("{name}.exe"));
                if executable.is_file() {
                    return Some(executable);
                }
            }
        }
    }
    None
}

/// Candidate directories for user-installed fonts, most specific first.
pub fn font_dirs() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(base) = non_empty(std::env::var_os("XDG_DATA_HOME")) {
        directories.push(base.join("fonts"));
    }
    if let Some(home) = home_dir() {
        directories.push(home.join(".local/share/fonts"));
        directories.push(home.join(".fonts"));
        directories.push(home.join("Library/Fonts"));
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(base) = non_empty(std::env::var_os("LOCALAPPDATA")) {
            directories.push(base.join("Microsoft/Windows/Fonts"));
        }
    }
    directories
}
