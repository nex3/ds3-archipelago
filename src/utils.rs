use std::{env, path::PathBuf};

use anyhow::Result;
use imgui::*;
use json;
use mint::Vector2;

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

pub trait PopupModalExt {
    /// Sets the size of the modal dialog.
    fn size(self, size: impl Into<Vector2<f32>>, condition: Condition) -> Self;
}

impl<Label> PopupModalExt for PopupModal<'_, '_, Label> {
    fn size(self, size: impl Into<Vector2<f32>>, condition: Condition) -> Self {
        unsafe { imgui_sys::igSetNextWindowSize(size.into().into(), condition as i32) };
        self
    }
}
