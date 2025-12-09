use std::env;
use std::path::PathBuf;

use anyhow::Result;
use json;

/// The logic underlying [mod_directory].
pub fn mod_directory() -> Result<PathBuf> {
    let var = env::var("ME3_LAUNCHER_HOST_DLL")?;
    let mut path = PathBuf::from(if var.starts_with('"') {
        // Work around garyttierney/me3#607 while it exists.
        json::from_str::<String>(var.as_str())?
    } else {
        var
    });
    path.pop();
    path.pop();
    Ok(path)
}
