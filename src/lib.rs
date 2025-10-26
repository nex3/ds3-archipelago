use std::fs;
use std::panic;
use std::path::Path;
use std::time::Duration;

use chrono::prelude::*;
use darksouls3_util::system::wait_for_system_init;
use fromsoftware_shared::program::Program;
use hudhook::{Hudhook, hooks::dx11::ImguiDx11Hooks};
use log::*;
use simplelog::{ColorChoice, CombinedLogger, TermLogger, TerminalMode, WriteLogger};
use windows::Win32::{
    Foundation::*, System::SystemServices::*, UI::WindowsAndMessaging::MessageBoxW,
};
use windows::core::*;

mod archipelago_mod;
mod client;
mod clipboard_backend;
mod config;
mod paths;
mod preload;
mod save_data;
mod slot_data;

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

    start_logger(&*paths::MOD_DIRECTORY);

    info!("Logger initialized.");

    preload::preload();

    // Set up hooks in the main thread to mitigate the risk of the game code
    // isn't executing them while they're being modified.

    // Safety: We only hook these functions here specifically.
    unsafe { SaveData::hook() };

    std::thread::spawn(move || {
        info!("Worker thread initialized.");
        wait_for_system_init(&Program::current(), Duration::MAX)
            .expect("Timeout waiting for system init");

        info!("Game system initialized.");

        if let Err(e) = Hudhook::builder()
            .with::<ImguiDx11Hooks>(archipelago_mod::ArchipelagoMod::new())
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
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Warn,
            simplelog::Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            simplelog::Config::default(),
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(filename)
                .unwrap(),
        ),
    ])
    .unwrap();
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
