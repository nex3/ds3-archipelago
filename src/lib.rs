use core::result::Result;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::panic;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::prelude::*;
use darksouls3::sprj::{SprjTaskGroupIndex, SprjTaskImp};
use eldenring_util::system::wait_for_system_init;
use fromsoftware_shared::{program::Program, singleton::get_instance, task::*};
use log::*;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use windows::Win32::{
    Foundation::*,
    System::LibraryLoader::{
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GetModuleFileNameW, GetModuleHandleExW,
    },
    System::SystemServices::*,
    UI::WindowsAndMessaging::MessageBoxW,
};
use windows::core::*;

/// The entrypoint called when the DLL is first loaded.
///
/// This is where we set up the whole mod and start waiting for the app itself
/// to be initialized enough for us to start doing real things.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn DllMain(_: HINSTANCE, call_reason: u32) -> bool {
    if call_reason != DLL_PROCESS_ATTACH {
        return true;
    }

    handle_panics();

    let dll = current_dll_path().unwrap();
    let parent = dll.parent().unwrap();
    if parent.ends_with("target/debug") {
        start_logger(&parent.parent().unwrap().parent().unwrap().join("log"));
    } else {
        start_logger(&parent.join("log"));
    }

    trace!("Logger initialized.");

    std::thread::spawn(move || {
        trace!("Worker thread initialized.");
        wait_for_system_init(&Program::current(), Duration::MAX)
            .expect("Timeout waiting for system init");

        trace!("Game system initialized.");
        on_load();
    });

    true
}

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

    return Ok(PathBuf::from(OsString::from_wide(&filename)));
}

/// Handle panics by both logging and popping up a message box, which is the
/// most reliable way to make something visible to the end user.
fn handle_panics() {
    panic::set_hook(Box::new(|panic_info| {
        let mut message = String::new();
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            message.push_str(&format!("Rust panic: {s}"));
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            message.push_str(&format!("Rust panic: {s}"));
        } else {
            message.push_str(&format!("Rust panic: {:?}", panic_info.payload()));
        }

        if let Some(l) = panic_info.location() {
            message.push_str(&format!("\n{l}"));
        }

        error!("{}", message);
        message_box(message);
    }));
}

/// Starts the logger which logs to both stdout and a file which users can send
/// to the devs for debugging.
fn start_logger(dir: &impl AsRef<Path>) {
    let dir_ref = dir.as_ref();
    fs::create_dir_all(dir_ref).unwrap();
    let filename = dir_ref.join(Local::now().format("archipelago-%Y-%m-%d.log").to_string());
    message_box(filename.display().to_string());
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Warn,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Trace,
            Config::default(),
            fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(true)
                .open(filename)
                .unwrap(),
        ),
    ])
    .unwrap();
}

/// A function that runs once the basic game systems are set up.
///
/// This doesn't guarantee that any particular singleton is available beyond the
/// core task system.
pub fn on_load() {
    let Some(task_imp) = (unsafe { get_instance::<SprjTaskImp>() }) else {
        panic!("Couldn't load SprjTaskImp");
        return;
    };

    let mut first = true;
    task_imp.run_recurring(
        move |_: &usize| {
            if first {
                message_box(format!("In task!"));
                first = false;
            }
        },
        SprjTaskGroupIndex::DbgDispStep,
    );
    trace!("Scheduled initial task.");
}

/// Displays a message box with the given message.
fn message_box(message: impl Into<String>) {
    unsafe {
        MessageBoxW(
            HWND(0),
            &HSTRING::from(message.into()),
            w!("DS3 Archipelago Client"),
            Default::default(),
        );
    }
}
