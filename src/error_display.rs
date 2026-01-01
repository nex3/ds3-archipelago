use std::sync::{Arc, Mutex};

use hudhook::{ImguiRenderLoop, RenderContext};
use imgui::*;

use anyhow::{Error, Result};
use darksouls3::util::input::{InputBlocker, InputFlags};

use crate::{
    Core, clipboard_backend::WindowsClipboardBackend, overlay::Overlay, utils::PopupModalExt,
};

/// A wrapper around the rest of the mod's UI that doesn't expect any state to
/// exist. This allows the full [Overlay] to assume that its [Core] exists while
/// still using Hudhook and ImGui to surface fatal errors that may occur during
/// initialization.
pub struct ErrorDisplay {
    /// The struct that's used to block and unblock input going to DS3.
    input_blocker: &'static InputBlocker,

    /// The main overlay if it managed to initialize correctly, or [None]
    /// otherwise.
    overlay: Option<Overlay>,

    /// The core game logic. Used to extract fatal errors to display to the
    /// user.
    core: Option<Arc<Mutex<Core>>>,

    /// A fatal error to display. Once set, this can't be changed, even if other
    /// fatal errors are detected later.
    error: Option<Error>,

    /// Whether to display the full error information or just the summary.
    show_full_error: bool,
}

impl ErrorDisplay {
    /// Creates a new [ErrorDisplay] that will only ever be run
    pub fn new(core: Result<Arc<Mutex<Core>>>, input_blocker: &'static InputBlocker) -> Self {
        match core {
            Ok(core) => Self {
                input_blocker,
                overlay: Some(Overlay::new(core.clone())),
                core: Some(core),
                error: None,
                show_full_error: false,
            },
            Err(error) => Self {
                input_blocker,
                overlay: None,
                core: None,
                error: Some(error),
                show_full_error: false,
            },
        }
    }
}

impl ImguiRenderLoop for ErrorDisplay {
    fn render(&mut self, ui: &mut Ui) {
        let io = ui.io();
        let mut flag = InputFlags::empty();
        if io.want_capture_mouse {
            flag |= InputFlags::Mouse;
        }
        if io.want_capture_keyboard {
            flag |= InputFlags::Keyboard;
        }
        if io.want_capture_mouse && io.want_capture_keyboard {
            // Only block pad input if both the mouse and keyboard are blocked
            // (for example if a modal dialog is up).
            flag |= InputFlags::GamePad;
        }
        self.input_blocker.block_only(flag);

        if let Some(overlay) = &mut self.overlay {
            overlay.render(ui);
        }

        if self.error.is_none()
            && let Some(core) = &self.core
        {
            self.error = core.lock().unwrap().take_error();
        }

        let Some(error) = &self.error else { return };

        unsafe {
            imgui_sys::igSetNextWindowSize(
                [800., if self.show_full_error { 500. } else { 0. }].into(),
                Condition::Always as i32,
            );
        }

        ui.open_popup("#fatal-error");
        ui.modal_popup_config("#fatal-error")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .size(
                [800., if self.show_full_error { 500. } else { 0. }],
                Condition::Always,
            )
            .build(|| {
                ui.set_window_font_scale(1.8);

                ui.checkbox("Show full error", &mut self.show_full_error);
                ui.text_wrapped(if self.show_full_error {
                    format!("{:?}", error)
                } else {
                    error.to_string()
                });

                ui.separator();
                if ui.button("Exit") {
                    std::process::exit(1);
                }
            });
    }

    fn initialize<'a>(&'a mut self, ctx: &mut Context, _render_context: &'a mut dyn RenderContext) {
        ctx.set_clipboard_backend(WindowsClipboardBackend {});
    }

    fn before_render<'a>(
        &'a mut self,
        ctx: &mut Context,
        render_context: &'a mut dyn RenderContext,
    ) {
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.before_render(ctx, render_context);
        }
    }
}
