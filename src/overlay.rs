use archipelago_rs::protocol::{JSONColor, JSONMessagePart, PrintJSON};
use darksouls3::util::input::*;
use hudhook::{ImguiRenderLoop, RenderContext};
use imgui::*;
use log::*;

use crate::archipelago_mod::{ArchipelagoMod, SimpleConnectionState};
use crate::clipboard_backend::WindowsClipboardBackend;
use crate::save_data::*;

const GREEN: ImColor32 = ImColor32::from_rgb(0x8A, 0xE2, 0x43);
const RED: ImColor32 = ImColor32::from_rgb(0xFF, 0x44, 0x44);
const WHITE: ImColor32 = ImColor32::from_rgb(0xFF, 0xFF, 0xFF);
// This is the darkest gray that still meets WCAG guidelines for contrast with
// the black background of the overlay.
const BLACK: ImColor32 = ImColor32::from_rgb(0x9C, 0x9C, 0x9C);
const YELLOW: ImColor32 = ImColor32::from_rgb(0xFC, 0xE9, 0x4F);
const BLUE: ImColor32 = ImColor32::from_rgb(0x82, 0xA9, 0xD4);
const MAGENTA: ImColor32 = ImColor32::from_rgb(0xBF, 0x9B, 0xBC);
const CYAN: ImColor32 = ImColor32::from_rgb(0x34, 0xE2, 0xE2);

/// The visual overlay that appears on top of the game.
///
/// Because this is driver of the event loop and of user interaction, it owns
/// the mod itself.
pub struct Overlay {
    /// The struct that manages the core mod logic that's not strictly
    /// UI-related.
    core: ArchipelagoMod,

    /// The struct that's used to block and unblock input going to DS3.
    input_blocker: &'static InputBlocker,

    /// The last-known size of the viewport. This is only set once hudhook has
    /// been initialized and the viewport has a non-zero size.
    viewport_size: Option<[f32; 2]>,

    /// The URL field in the modal connection popup.
    popup_url: String,

    /// The slot name field in the modal connection popup.
    popup_slot: String,

    /// The password field in the modal connection popup.
    popup_password: String,

    /// The text the user typed in the say input.
    say_input: String,

    /// Whether the log was previously scrolled all the way down.
    log_was_scrolled_down: bool,

    /// The number of logs that were most recently emitted. This is used to
    /// determine when new logs are emitted for [frames_since_new_logs].
    logs_emitted: usize,

    /// The number of frames that have elapsed since new logs were last added.
    /// We use this to determine when to auto-scroll the log window.
    frames_since_new_logs: u64,
}

// Safety: The sole ArchipelagoMod instance is owned by Hudhook, which only ever
// interacts with it during frame rendering. We know DS3 frame rendering always
// happens on the main thread, and never in parallel, so synchronization is not
// a real concern.
unsafe impl Sync for Overlay {}

impl Overlay {
    /// Creates a new instance of the overlay and the core mod logic.
    pub fn new(input_blocker: &'static InputBlocker) -> Overlay {
        Overlay {
            core: ArchipelagoMod::new(),
            input_blocker,
            viewport_size: None,
            popup_url: String::new(),
            popup_slot: String::new(),
            popup_password: String::new(),
            say_input: String::new(),
            log_was_scrolled_down: false,
            logs_emitted: 0,
            frames_since_new_logs: 0,
        }
    }

    /// Renders the modal popup which queries the player for connection
    /// information.
    fn render_connection_popup(&mut self, ui: &Ui) {
        ui.modal_popup_config("#connect")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .build(|| {
                let t = ui.push_item_width(400.);
                ui.input_text("Room URL", &mut self.popup_url)
                    .hint("archipelago.gg:12345")
                    .chars_noblank(true)
                    .build();
                ui.input_text("Player Name", &mut self.popup_slot).build();
                ui.input_text("Password", &mut self.popup_password)
                    .password(true)
                    .build();
                drop(t);

                ui.disabled(
                    self.popup_url.len() == 0 || self.popup_slot.len() == 0,
                    || {
                        if ui.button("Connect") {
                            ui.close_current_popup();
                            if let Err(e) = self.core.update_config(
                                &self.popup_url,
                                &self.popup_slot,
                                if self.popup_password.len() == 0 {
                                    None
                                } else {
                                    Some(&self.popup_password)
                                },
                            ) {
                                error!("Failed to save config: {e}");
                            }
                            self.core.connect();
                        }
                    },
                );
            });
    }

    /// Renders the widget that displays the current connection status and
    /// allows the player to reconnect to Archipelago.
    fn render_connection_widget(&mut self, ui: &Ui) {
        ui.text("Connection status:");
        ui.same_line();
        match self.core.simple_connection_state() {
            SimpleConnectionState::Connected => ui.text_colored(RED.to_rgba_f32s(), "Connected"),
            SimpleConnectionState::Connecting => ui.text("Connecting..."),
            SimpleConnectionState::Disconnected => {
                ui.text_colored(RED.to_rgba_f32s(), "Disconnected");
                ui.same_line();
                if ui.button("Connect") {
                    ui.open_popup("#connect");
                    let config = self.core.config();
                    copy_from_or_clear(&mut self.popup_url, config.url());
                    copy_from_or_clear(&mut self.popup_slot, config.slot());
                    copy_from_or_clear(&mut self.popup_password, config.password());
                }
            }
        }
    }

    /// Renders the log window which displays all the prints sent from the server.
    fn render_log_window(&mut self, ui: &Ui) {
        ui.child_window("#log")
            .size([0.0, -33.])
            .draw_background(false)
            .always_vertical_scrollbar(true)
            .horizontal_scrollbar(true)
            .build(|| {
                let logs = self.core.logs();
                if logs.len() != self.logs_emitted {
                    self.frames_since_new_logs = 0;
                    self.logs_emitted = logs.len();
                }

                for message in logs {
                    use PrintJSON::*;
                    write_message_data(
                        ui,
                        &message.data(),
                        // De-emphasize miscellaneous server prints.
                        match message {
                            Chat { .. }
                            | ServerChat { .. }
                            | Tutorial { .. }
                            | CommandResult { .. }
                            | AdminCommandResult { .. }
                            | Unknown { .. } => 0xff,
                            ItemSend { receiving, .. }
                            | ItemCheat { receiving, .. }
                            | Hint { receiving, .. }
                                if self.core.slot().is_some_and(|s| *receiving == s) =>
                            {
                                0xFF
                            }
                            _ => 0xAA,
                        },
                    );
                }
                if self.log_was_scrolled_down && self.frames_since_new_logs < 10 {
                    ui.set_scroll_y(ui.scroll_max_y());
                }
                self.log_was_scrolled_down = ui.scroll_y() == ui.scroll_max_y();
            });
    }

    /// Renders the text box in which users can write chats to the server.
    fn render_say_input(&mut self, ui: &Ui) {
        ui.disabled(self.core.client().is_none(), || {
            let width = ui.push_item_width(-40.);
            let mut send = ui
                .input_text("##say-input", &mut self.say_input)
                .enter_returns_true(true)
                .build();
            drop(width);

            ui.same_line();
            let width = ui.push_item_width(30.);
            send = ui.arrow_button("##say-button", Direction::Right) || send;
            drop(width);

            if send && let Some(client) = self.core.client() {
                client.say(&self.say_input);
                self.say_input.clear();
            }
        });
    }

    /// Renders the popup window alerting the user that their connected seed
    /// doesn't match their saved seed. Returns whether the popup was rendered.
    fn render_version_conflict_popup(&mut self, ui: &Ui) -> bool {
        let Some(version) = self.core.config().version() else {
            return false;
        };
        if version == env!("CARGO_PKG_VERSION") {
            return false;
        }

        ui.open_popup("#version-conflict");
        ui.modal_popup_config("#version-conflict")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .build(|| {
                // Without a wrapper window, the size of the popup ends up
                // narrow and tall. There seems to be no way to control this
                // directly with the Rust UI.
                let Some(_tok) = ui
                    .child_window("#version-conflict-window")
                    .size([600., 130.])
                    .begin()
                else {
                    return;
                };
                ui.set_window_font_scale(1.8);

                ui.text_wrapped(format!(
                    "This save was generated using static randomizer v{}, but \
                     this client is v{}. Re-run the static randomizer with the \
                     current version.",
                    version,
                    env!("CARGO_PKG_VERSION"),
                ));

                ui.separator();
                if ui.button("Exit") {
                    std::process::exit(1);
                }
            });
        true
    }

    /// Renders the popup window alerting the user that their connected seed
    /// doesn't match their saved seed.
    fn render_seed_conflict_popup(&mut self, ui: &Ui) {
        let Some(client) = self.core.client() else {
            return;
        };
        let mut save_data = SaveData::instance_mut();
        let Some(save_data) = save_data.as_mut() else {
            return;
        };
        if save_data.seed_matches(&client.room_info().seed_name) {
            return;
        }

        ui.open_popup("#seed-conflict");
        ui.modal_popup_config("#seed-conflict")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .build(|| {
                // Without a wrapper window, the size of the popup ends up
                // narrow and tall. There seems to be no way to control this
                // directly with the Rust UI.
                let Some(_tok) = ui
                    .child_window("#seed-conflict-window")
                    .size([600., 250.])
                    .begin()
                else {
                    return;
                };
                ui.set_window_font_scale(1.8);

                ui.text_wrapped(format!(
                    "You've connected to a different Archipelago multiworld \
                     than the one that you used before with this save!\n\
                     \n\
		     Save file seed: {}\n\
		     Connected room seed: {}\n\
		     \n\
		     Continue connecting and overwrite the save file seed?",
                    save_data.seed.as_ref().unwrap(),
                    client.room_info().seed_name,
                ));

                ui.separator();
                if ui.button("Overwrite") {
                    save_data.seed = Some(client.room_info().seed_name.clone());
                    return;
                }

                ui.same_line_with_spacing(0., 20.);
                if ui.button("Exit") {
                    // TODO: It would be cool if we could quit out of the save
                    // file to the main menu rather than outright killing the
                    // program. There may be something in GameMan for that...
                    std::process::exit(1);
                }
            });
    }
}

impl ImguiRenderLoop for Overlay {
    fn render(&mut self, ui: &mut Ui) {
        self.core.tick();

        let io = ui.io();
        let mut flag = InputFlags::empty();
        if io.want_capture_mouse {
            flag = flag | InputFlags::Mouse;
        }
        if io.want_capture_keyboard {
            flag = flag | InputFlags::Keyboard;
        }
        if io.want_capture_mouse && io.want_capture_keyboard {
            // Only block pad input if both the mouse and keyboard are blocked
            // (for example if a modal dialog is up).
            flag = flag | InputFlags::GamePad;
        }
        self.input_blocker.block_only(flag);

        let Some(viewport_size) = self.viewport_size else {
            // Work around veeenu/hudhook#235
            ui.window("tmp")
                .size([100., 100.], Condition::Always)
                .position([-200., -200.], Condition::Always)
                .build(|| {});
            return;
        };

        ui.window(format!("Archipelago Client {}", env!("CARGO_PKG_VERSION")))
            .position([viewport_size[0] - 30., 30.], Condition::FirstUseEver)
            .position_pivot([1., 0.])
            .size([viewport_size[0] * 0.4, 300.], Condition::FirstUseEver)
            .build(|| {
                ui.set_window_font_scale(1.8);

                self.render_connection_widget(ui);
                ui.separator();
                self.render_log_window(ui);
                self.render_connection_popup(ui);
                self.render_say_input(ui);
                if !self.render_version_conflict_popup(ui) {
                    self.render_seed_conflict_popup(ui);
                }
            });
    }

    fn initialize<'a>(&'a mut self, ctx: &mut Context, _render_context: &'a mut dyn RenderContext) {
        ctx.set_clipboard_backend(WindowsClipboardBackend {});
    }

    fn before_render<'a>(
        &'a mut self,
        ctx: &mut Context,
        _render_context: &'a mut dyn RenderContext,
    ) {
        self.frames_since_new_logs += 1;
        self.viewport_size = match ctx.main_viewport().size {
            [0., 0.] => None,
            size => Some(size),
        };
    }
}

trait ImColor32Ext {
    /// Returns a copy of [self] with its opacity overridden by [alpha].
    fn with_alpha(&self, alpha: u8) -> ImColor32;
}

impl ImColor32Ext for ImColor32 {
    fn with_alpha(&self, alpha: u8) -> ImColor32 {
        ImColor32::from_bits((self.to_bits() & 0x00ffffff) | ((alpha as u32) << 24))
    }
}

/// If [source] is set, copies its contents into [target]. Otherwise, sets
/// [target] to the empty string.
fn copy_from_or_clear(target: &mut String, source: Option<&String>) {
    if let Some(value) = source {
        target.clone_from(value);
    } else {
        target.clear();
    }
}

/// Writes the text in [parts] to [ui] in a single line.
fn write_message_data(ui: &Ui, parts: &Vec<JSONMessagePart>, alpha: u8) {
    let mut first = true;
    for part in parts {
        if !first {
            ui.same_line();
        }
        first = false;

        // TODO: Load in fonts to support bold, maybe write a line manually for
        // underline? I'm not sure there's a reasonable way to support
        // background colors.
        use JSONColor::*;
        use JSONMessagePart::*;
        let color = match part {
            PlayerId { .. } | PlayerName { .. } | Color { color: Blue, .. } => BLUE,
            ItemId { .. } | ItemName { .. } | Color { color: Magenta, .. } => MAGENTA,
            LocationId { .. }
            | LocationName { .. }
            | EntranceName { .. }
            | Color { color: Cyan, .. } => CYAN,
            Color { color: Black, .. } => BLACK,
            Color { color: Red, .. } => RED,
            Color { color: Green, .. } => GREEN,
            Color { color: Yellow, .. } => YELLOW,
            _ => WHITE,
        };
        ui.text_colored(color.with_alpha(alpha).to_rgba_f32s(), &part.text());
    }
}
