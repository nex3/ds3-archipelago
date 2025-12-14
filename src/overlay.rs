use archipelago_rs::protocol::{RichMessageColor, RichMessagePart, RichPrint};
use hudhook::RenderContext;
use imgui::*;
use log::*;

use anyhow::Result;

use crate::core::{Core, SimpleConnectionState};
use crate::utils::PopupModalExt;

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
    core: Core,

    /// The last-known size of the viewport. This is only set once hudhook has
    /// been initialized and the viewport has a non-zero size.
    viewport_size: Option<[f32; 2]>,

    /// The URL field in the modal connection popup.
    popup_url: String,

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

    /// The current font scale for the overlay UI.
    font_scale: f32,

    /// Whether to show the chat input line.
    show_input: bool,

    /// Whether to show the horizontal scrollbar in the log window when
    /// content overflows.
    show_horizontal_scrollbar: bool,
}

// Safety: The sole Overlay instance is owned by Hudhook, which only ever
// interacts with it during frame rendering. We know DS3 frame rendering always
// happens on the main thread, and never in parallel, so synchronization is not
// a real concern.
unsafe impl Sync for Overlay {}

impl Overlay {
    /// Creates a new instance of the overlay and the core mod logic.
    pub fn new() -> Result<Self> {
        Ok(Self {
            core: Core::new()?,
            viewport_size: None,
            popup_url: String::new(),
            say_input: String::new(),
            log_was_scrolled_down: false,
            logs_emitted: 0,
            frames_since_new_logs: 0,
            font_scale: 1.8,
            show_input: true,
            show_horizontal_scrollbar: true,
        })
    }

    /// Like [ImguiRenderLoop.render], except that this can return a [Result] to
    /// signal a fatal error.
    ///
    /// The [error] flag indicates whether this has experienced a fatal error
    /// that it's in the process of displaying to the user.
    pub fn render(&mut self, ui: &mut Ui, error: bool) -> Result<()> {
        self.core.tick(error)?;

        let Some(viewport_size) = self.viewport_size else {
            return Ok(());
        };

        ui.window(format!("Archipelago Client {}", env!("CARGO_PKG_VERSION")))
            .position([viewport_size[0] - 30., 30.], Condition::FirstUseEver)
            .position_pivot([1., 0.])
            .size([viewport_size[0] * 0.4, 300.], Condition::FirstUseEver)
            .build(|| {
                ui.set_window_font_scale(self.font_scale);

                self.render_settings_button(ui);
                ui.same_line();
                self.render_connection_widget(ui);
                ui.separator();
                self.render_log_window(ui);
                if self.show_input {
                    self.render_say_input(ui);
                }
                self.render_url_popup(ui);
                self.render_settings_popup(ui);
            });

        Ok(())
    }

    /// Like [ImguiRenderLoop.before_render].
    pub fn before_render<'a>(
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

    /// Renders the modal popup which queries the player for connection
    /// information.
    fn render_url_popup(&mut self, ui: &Ui) {
        ui.modal_popup_config("#url-popup")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .size([600., 0.], Condition::FirstUseEver)
            .build(|| {
                {
                    let _ = ui.push_item_width(600.);
                    ui.input_text("Room URL", &mut self.popup_url)
                        .hint("archipelago.gg:12345")
                        .chars_noblank(true)
                        .build();
                }

                ui.disabled(self.popup_url.is_empty(), || {
                    if ui.button("Connect") {
                        ui.close_current_popup();
                        if let Err(e) = self.core.update_url(&self.popup_url) {
                            error!("Failed to save config: {e}");
                        }
                    }
                });
            });
    }

    /// Renders the modal popup which allows the user to change the overlay
    /// settings.
    fn render_settings_popup(&mut self, ui: &Ui) {
        ui.popup("#settings-popup", || {
            ui.text("Font Size");
            ui.same_line();
            if ui.button("-##font-size-decrease-button") {
                self.font_scale = (self.font_scale - 0.1).max(0.5);
            }
            ui.same_line();
            if ui.button("+##font-size-increase-button") {
                self.font_scale = (self.font_scale + 0.1).min(4.0);
            }

            ui.text("Show Chat Input");
            ui.same_line();
            ui.checkbox("##show-input-checkbox", &mut self.show_input);

            ui.text("Show Horizontal Scrollbar");
            ui.same_line();
            ui.checkbox("##show-horizontal-scrollbar-checkbox", &mut self.show_horizontal_scrollbar);
        });
    }

    /// Renders the settings button that opens the settings popup.
    fn render_settings_button(&mut self, ui: &Ui) {
        if ui.button("UI") {
            ui.open_popup("#settings-popup");
        }
    }

    /// Renders the widget that displays the current connection status and
    /// allows the player to reconnect to Archipelago.
    fn render_connection_widget(&mut self, ui: &Ui) {
        ui.text("Connection status:");
        ui.same_line();
        match self.core.simple_connection_state() {
            SimpleConnectionState::Connected => ui.text_colored(GREEN.to_rgba_f32s(), "Connected"),
            SimpleConnectionState::Connecting => ui.text("Connecting..."),
            SimpleConnectionState::Disconnected => {
                ui.text_colored(RED.to_rgba_f32s(), "Disconnected");
                ui.same_line();
                if ui.button("Change URL") {
                    ui.open_popup("#url-popup");
                    self.core.config().url().clone_into(&mut self.popup_url);
                }
            }
        }
    }

    /// Renders the log window which displays all the prints sent from the server.
    fn render_log_window(&mut self, ui: &Ui) {
        let input_height = if self.show_input {
            ui.frame_height_with_spacing().ceil()
        } else {
            0.0
        };

        ui.child_window("#log")
            .size([0.0, -input_height])
            .draw_background(false)
            .always_vertical_scrollbar(true)
            .horizontal_scrollbar(self.show_horizontal_scrollbar)
            .build(|| {
                let logs = self.core.logs();
                if logs.len() != self.logs_emitted {
                    self.frames_since_new_logs = 0;
                    self.logs_emitted = logs.len();
                }

                for message in logs {
                    use RichPrint::*;
                    write_message_data(
                        ui,
                        message.data(),
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
            let arrow_button_width = ui.frame_height(); // Arrow buttons are square buttons.
            let style = ui.clone_style();
            let spacing = style.item_spacing[0] * self.font_scale * 0.7;
            
            let input_width = ui.push_item_width(-(arrow_button_width + spacing));
            let mut send = ui
                .input_text("##say-input", &mut self.say_input)
                .enter_returns_true(true)
                .build();
            drop(input_width);

            ui.same_line_with_spacing(0.0, spacing);
            send = ui.arrow_button("##say-button", Direction::Right) || send;

            if send && let Some(client) = self.core.client() {
                client.say(&self.say_input);
                self.say_input.clear();
            }
        });
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

/// Writes the text in [parts] to [ui] in a single line.
fn write_message_data(ui: &Ui, parts: &[RichMessagePart], alpha: u8) {
    let mut first = true;
    for part in parts {
        if !first {
            ui.same_line();
        }
        first = false;

        // TODO: Load in fonts to support bold, maybe write a line manually for
        // underline? I'm not sure there's a reasonable way to support
        // background colors.
        use RichMessageColor::*;
        use RichMessagePart::*;
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
        ui.text_colored(color.with_alpha(alpha).to_rgba_f32s(), part.to_string());
    }
}
