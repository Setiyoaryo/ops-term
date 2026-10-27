//! Explicit display selection without changing environment inherited by shells.
#[derive(Debug, PartialEq, Eq)]
pub enum Backend {
    Auto,
    X11,
    Wayland,
}

pub fn parse(value: Option<&str>) -> Result<Backend, String> {
    match value {
        None | Some("auto") => Ok(Backend::Auto),
        Some("x11") => Ok(Backend::X11),
        Some("wayland") => Ok(Backend::Wayland),
        _ => Err("OPSTERM_DISPLAY_BACKEND must be auto, x11, or wayland".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_default_and_accepts_explicit_backends() {
        assert_eq!(parse(None), Ok(Backend::Auto));
        assert_eq!(parse(Some("auto")), Ok(Backend::Auto));
        assert_eq!(parse(Some("x11")), Ok(Backend::X11));
        assert_eq!(parse(Some("wayland")), Ok(Backend::Wayland));
    }
    #[test]
    fn invalid_backend_is_a_clear_error() {
        assert!(parse(Some("other"))
            .unwrap_err()
            .contains("OPSTERM_DISPLAY_BACKEND"));
    }
}
