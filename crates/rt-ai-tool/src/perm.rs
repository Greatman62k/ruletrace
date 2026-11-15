//! The configuration permission Warning (Req 28.10).

use rt_foundation::{Code, Diagnostic, InputId, InputLocation, Location};
use std::path::Path;

const GROUP_OR_OTHER_READ: u32 = 0o044;

/// `RT-AI-0009` when `mode` has a read bit set for group or others. `None`
/// mode (the platform reports no permission bits) gives no Warning.
#[must_use]
pub fn permission_warning(input: &InputId, mode: Option<u32>) -> Option<Diagnostic> {
    let mode = mode?;
    if mode & GROUP_OR_OTHER_READ == 0 {
        return None;
    }
    Some(Diagnostic::new(
        Code::AiConfigPermissions,
        "the configuration file is readable by group or other users".to_owned(),
        Location::Input(InputLocation {
            input: input.clone(),
            line: None,
            column: None,
            field: None,
        }),
    ))
}

/// The permission bits of `path`, where the platform reports them.
#[must_use]
pub fn file_mode(path: &Path) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).ok().map(|m| m.permissions().mode())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}
