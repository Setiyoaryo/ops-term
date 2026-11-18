//! Serial consoles are a Unix feature; Windows reports a clear error.

use super::ConnectionProvider;
use crate::domain::{SerialSpec, TerminalDimensions};
use crate::error::{AppError, AppResult};

pub fn list_devices() -> Vec<String> {
    Vec::new()
}

pub fn spawn_serial(
    _spec: &SerialSpec,
    _dims: TerminalDimensions,
) -> AppResult<Box<dyn ConnectionProvider>> {
    Err(AppError::Validation(
        "serial consoles are not supported on this platform".into(),
    ))
}
