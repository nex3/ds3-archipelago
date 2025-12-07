use core::result::Result;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::LazyLock;

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleFileNameW, GetModuleHandleExW,
};
use windows::core::*;

/// The directory in which the mod lives.
///
/// This is where we store the user's config, log files, and so on. In
/// development, it's the root directory of the repo itself.
pub static MOD_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    let dll = current_dll_path().unwrap();
    let parent = dll.parent().unwrap();
    if parent.ends_with("target/debug") {
        parent.parent().unwrap().parent().unwrap().join("log")
    } else {
        parent.to_path_buf()
    }
});

/// Returns the path to `archipelago.dll`.
fn current_dll_path() -> Result<PathBuf, String> {
    let mut module = HMODULE::default();
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
            PCWSTR(current_dll_path as *mut u16),
            &mut module,
        )
    }
    .map_err(|e| e.to_string())?;

    // `GetModuleFileNameW` doesn't have any way to indicate how much room is
    // necessary for the file, so we have to progressively increase our
    // allocation until we hit the appropriate size.
    let mut size: usize = usize::try_from(MAX_PATH).map_err(|e| e.to_string())?;
    let mut filename: Vec<u16>;
    const GROWTH_FACTOR: f64 = 1.5;
    loop {
        filename = vec![0; size];
        let n = unsafe { GetModuleFileNameW(module, &mut filename) } as usize;
        if n == 0 {
            return Err(format!(
                "GetModuleFileNameW failed: {}",
                io::Error::last_os_error()
            ));
        } else if n == filename.capacity()
            && io::Error::last_os_error()
                .raw_os_error()
                .is_some_and(|c| i32::try_from(ERROR_INSUFFICIENT_BUFFER.0).is_ok_and(|e| c == e))
        {
            size = (size as f64 * GROWTH_FACTOR) as usize;
        } else {
            break;
        }
    }

    Ok(PathBuf::from(OsString::from_wide(&filename)))
}
