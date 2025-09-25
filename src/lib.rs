use std::fs;
use std::panic;
use std::path::Path;
use std::time::Duration;

use chrono::prelude::*;
use darksouls3::sprj::{SprjTaskGroupIndex, SprjTaskImp};
use eldenring_util::system::wait_for_system_init;
use fromsoftware_shared::{program::Program, singleton::get_instance, task::*};
use log::*;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode, WriteLogger};
use windows::Win32::{
    Foundation::*, System::SystemServices::*, UI::WindowsAndMessaging::MessageBoxW,
};
use windows::core::*;

mod paths;

/// The entrypoint called when the DLL is first loaded.
///
/// This is where we set up the whole mod and start waiting for the app itself
/// to be initialized enough for us to start doing real things.
#[unsafe(no_mangle)]
extern "C" fn DllMain(_: HINSTANCE, call_reason: u32) -> bool {
    if call_reason != DLL_PROCESS_ATTACH {
        return true;
    }

    handle_panics();

    start_logger(&*paths::MOD_DIRECTORY);

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
    };

    let mut first = true;
    task_imp.run_recurring(
        move |_: &usize| {
            if first {
                message_box("In task!".to_string());
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
