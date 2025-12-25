use std::{fs, panic, path::Path, time::Duration};

use anyhow::Result;
use backtrace::Backtrace;
use chrono::prelude::*;
use darksouls3::util::{input::InputBlocker, system::wait_for_system_init};
use fromsoftware_shared::program::Program;
use hudhook::{Hudhook, hooks::dx11::ImguiDx11Hooks};
use log::*;
use simplelog::{ColorChoice, CombinedLogger, SharedLogger, TermLogger, TerminalMode, WriteLogger};
use windows::Win32::{
    Foundation::*, System::SystemServices::*, UI::WindowsAndMessaging::MessageBoxW,
};
use windows::core::*;

mod clipboard_backend;
mod config;
mod core;
mod error_display;
mod item;
mod overlay;
mod save_data;
mod slot_data;
mod utils;

use error_display::ErrorDisplay;
use save_data::SaveData;

/// The entrypoint called when the DLL is first loaded.
///
/// This is where we set up the whole mod and start waiting for the app itself
/// to be initialized enough for us to start doing real things.
#[unsafe(no_mangle)]
extern "C" fn DllMain(hmodule: HINSTANCE, call_reason: u32) -> bool {
    if call_reason != DLL_PROCESS_ATTACH {
        return true;
    }

    handle_panics();

    // If there's an error locating the mod directory, try to log to the current
    // dir instead. Otherwise, ignore the error so we can surface it better
    // throught he UI.
    if let Ok(dir) = utils::mod_directory() {
        let _ = start_logger(dir);
        info!("Logger initialized.");
    }

    // Set up hooks in the main thread to mitigate the risk of the game code
    // executing them while they're being modified.

    // Safety: We only hook these functions here specifically.
    unsafe {
        SaveData::hook();
        item::hook_items();
    }

    let blocker =
        unsafe { InputBlocker::get_instance() }.expect("Failed to initialize input blocker");

    std::thread::spawn(move || {
        info!("Worker thread initialized.");
        wait_for_system_init(&Program::current(), Duration::MAX)
            .expect("Timeout waiting for system init");

        info!("Game system initialized.");

        if let Err(e) = Hudhook::builder()
            .with::<ImguiDx11Hooks>(ErrorDisplay::new(blocker))
            .with_hmodule(hmodule)
            .build()
            .apply()
        {
            panic!("Couldn't apply hooks: {e:?}");
        }
    });

    true
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

        message.push_str(&format!("\n{:?}", Backtrace::new()));

        error!("{}", message);
        message_box(message);
    }));
}

/// Starts the logger which logs to both stdout and a file which users can send
/// to the devs for debugging.
fn start_logger(dir: impl AsRef<Path>) -> Result<()> {
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![TermLogger::new(
        LevelFilter::Warn,
        simplelog::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )];
    if let Ok(logger) = create_write_logger(dir) {
        loggers.push(logger);
    }
    CombinedLogger::init(loggers)?;
    Ok(())
}

/// Creates a write logger that writes to files in [dir].
fn create_write_logger(dir: impl AsRef<Path>) -> Result<Box<WriteLogger<fs::File>>> {
    let dir = dir.as_ref().join("log");
    fs::create_dir_all(&dir)?;
    let filename = dir.join(Local::now().format("archipelago-%Y-%m-%d.log").to_string());
    Ok(WriteLogger::new(
        LevelFilter::Info,
        simplelog::Config::default(),
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(filename)?,
    ))
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
