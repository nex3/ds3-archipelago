use std::mem;

use archipelago_rs::{self as ap, RichText, TextColor};
use darksouls3::sprj::{MapItemMan, MenuMan};
use fromsoftware_shared::FromStatic;
use hudhook::RenderContext;
use imgui::*;
use imgui_sys::ImVec2;
use log::*;

use crate::core::Core;

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
pub struct Overlay {
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

    /// The unfocused window opacity for the overlay UI.
    unfocused_window_opacity: f32,

    /// Whether the overlay window was focused in the previous frame.
    was_window_focused: bool,

    was_menu_open: bool,

    previous_size: Option<[f32; 2]>,

    /// Whether the unfocused window opacity button in the view menu was
    /// clicked. Necessary to close the menu, so the opacity can be
    /// adjusted without the menu overlapping the window.
    was_unfocused_window_opacity_button_clicked: bool,
}

// Safety: The sole Overlay instance is owned by Hudhook, which only ever
// interacts with it during frame rendering. We know DS3 frame rendering always
// happens on the main thread, and never in parallel, so synchronization is not
// a real concern.
unsafe impl Sync for Overlay {}

impl Overlay {
    /// Creates a new instance of the overlay and the core mod logic.
    pub fn new() -> Self {
        Self {
            viewport_size: None,
            popup_url: String::new(),
            say_input: String::new(),
            log_was_scrolled_down: false,
            logs_emitted: 0,
            frames_since_new_logs: 0,
            font_scale: 1.8,
            unfocused_window_opacity: 0.4,
            was_window_focused: false,
            was_menu_open: false,
            previous_size: None,
            was_unfocused_window_opacity_button_clicked: false,
        }
    }

    /// Like [ImguiRenderLoop::render], but takes a reference to [Core] as well.
    ///
    /// We don't store `core` directly in the overlay so that we can ensure that
    /// its mutex is only locked once per render.
    pub fn render(&mut self, ui: &mut Ui, core: &mut Core) {
        let Some(viewport_size) = self.viewport_size else {
            return;
        };

        let window_opacity = if self.was_window_focused {
            1.0
        } else {
            self.unfocused_window_opacity
        };
        let mut bg_color = [0.0, 0.0, 0.0, window_opacity];
        let _bg = ui.push_style_color(StyleColor::WindowBg, bg_color);
        let _menu_bg = ui.push_style_color(StyleColor::MenuBarBg, bg_color);
        bg_color[3] = 1.0; // Popup backgrounds should always be fully opaque.
        let _popup_bg = ui.push_style_color(StyleColor::PopupBg, bg_color);

        let mut builder = ui
            .window(format!("Archipelago Client {}", env!("CARGO_PKG_VERSION")))
            .position([viewport_size[0] - 30., 30.], Condition::FirstUseEver)
            .position_pivot([1., 0.])
            .menu_bar(true);

        // When the menu opens or closes, add or remove space from the bottom of
        // the overlay for the message bar and horizontal scrollbar.
        builder = match (self.previous_size, self.is_menu_open(), self.was_menu_open) {
            (Some(size), true, false) => builder.size([size[0], size[1] + 50.], Condition::Always),
            (Some(size), false, true) => builder.size([size[0], size[1] - 50.], Condition::Always),
            _ => builder.size([viewport_size[0] * 0.4, 300.], Condition::FirstUseEver),
        };

        builder.build(|| {
            ui.set_window_font_scale(self.font_scale);

            self.render_unfocused_window_opacity_popup(ui);
            self.render_menu_bar(ui);
            self.render_connection_widget(ui, core);
            ui.separator();
            self.render_log_window(ui, core);
            if self.is_menu_open() {
                self.render_say_input(ui, core);
            }
            self.render_url_modal_popup(ui, core);

            self.was_window_focused =
                ui.is_window_focused_with_flags(WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS);
            self.was_menu_open = self.is_menu_open();
            self.previous_size = Some(ui.window_size());
        });

        drop(_bg);
        drop(_menu_bg);
    }

    /// See [ImguiRenderLoop::before_render], but takes a reference to [Core] as
    /// well.
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
    fn render_url_modal_popup(&mut self, ui: &Ui, core: &mut Core) {
        ui.modal_popup_config("#url-modal-popup")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .always_auto_resize(true)
            .build(|| {
                {
                    let _ = ui.push_item_width(500. * self.font_scale);
                    ui.input_text("Room URL", &mut self.popup_url)
                        .hint("archipelago.gg:12345")
                        .chars_noblank(true)
                        .build();
                }

                ui.disabled(self.popup_url.is_empty(), || {
                    if ui.button("Connect") {
                        ui.close_current_popup();
                        if let Err(e) = core.update_url(&self.popup_url) {
                            error!("Failed to save config: {e}");
                        }
                    }
                });
            });
    }

    /// Renders the popup which allows the user to set the unfocused window opacity.
    fn render_unfocused_window_opacity_popup(&mut self, ui: &Ui) {
        if self.was_unfocused_window_opacity_button_clicked {
            ui.open_popup("#unfocused-window-opacity-popup");
            self.was_unfocused_window_opacity_button_clicked = false;

            if let Some(viewport_size) = self.viewport_size {
                let center_x = viewport_size[0] / 2.0;
                let center_y = viewport_size[1] / 2.0;
                unsafe {
                    sys::igSetNextWindowPos(
                        ImVec2::new(center_x, center_y),
                        Condition::Appearing as i32,
                        ImVec2::new(0.5, 0.5),
                    );
                }
            }
        }

        ui.popup("#unfocused-window-opacity-popup", || {
            // Force unfocused state while setting opacity to give immediate feedback.
            self.was_window_focused = false;

            let mut opacity_percent = (self.unfocused_window_opacity * 100.0).round() as i32;
            let _slider_width = ui.push_item_width(150. * self.font_scale);
            ui.slider_config("Window Opacity (Unfocused)", 0, 100)
                .display_format("%d%%")
                .build(&mut opacity_percent);
            drop(_slider_width);
            self.unfocused_window_opacity = (opacity_percent as f32) / 100.0;

            if ui.button("Close") {
                ui.close_current_popup();
            }
        });
    }

    /// Renders the menu bar.
    fn render_menu_bar(&mut self, ui: &Ui) {
        ui.menu_bar(|| {
            ui.menu("View", || {
                self.render_view_menu(ui);
            });
        });
    }

    /// Renders the view menu.
    fn render_view_menu(&mut self, ui: &Ui) {
        ui.text("Font Size");
        ui.same_line();
        if ui.button("-##font-size-decrease-button") {
            self.font_scale = (self.font_scale - 0.1).max(0.5);
        }
        ui.same_line();
        if ui.button("+##font-size-increase-button") {
            self.font_scale = (self.font_scale + 0.1).min(4.0);
        }

        ui.text("Window Opacity (Unfocused)");
        ui.same_line();
        if ui.button("Change##unfocused-window-opacity-button") {
            ui.close_current_popup();
            self.was_unfocused_window_opacity_button_clicked = true;
        }
    }

    /// Renders the widget that displays the current connection status and
    /// allows the player to reconnect to Archipelago.
    fn render_connection_widget(&mut self, ui: &Ui, core: &Core) {
        ui.text("Connection status:");
        ui.same_line();
        match core.connection_state_type() {
            ap::ConnectionStateType::Connected => {
                ui.text_colored(GREEN.to_rgba_f32s(), "Connected");
            }
            ap::ConnectionStateType::Connecting => {
                ui.text_colored(BLUE.to_rgba_f32s(), "Connecting...");
            }
            ap::ConnectionStateType::Disconnected => {
                bold_text_colored(ui, "Disconnected", RED.to_rgba_f32s());
                ui.same_line();
                if ui.button("Change URL") {
                    ui.open_popup("#url-modal-popup");
                    core.config().url().clone_into(&mut self.popup_url);
                }
            }
        }
    }

    /// Renders the log window which displays all the prints sent from the server.
    fn render_log_window(&mut self, ui: &Ui, core: &Core) {
        let scrollbar_bg_opacity = if self.was_window_focused { 1.0 } else { 0.0 };
        let scrollbar_bg_color = [0.0, 0.0, 0.0, scrollbar_bg_opacity];
        let _scrollbar_bg = ui.push_style_color(StyleColor::ScrollbarBg, scrollbar_bg_color);

        let input_height = if self.is_menu_open() {
            ui.frame_height_with_spacing().ceil()
        } else {
            0.0
        };

        ui.child_window("#log")
            .size([0.0, -input_height])
            .draw_background(false)
            .always_vertical_scrollbar(true)
            .horizontal_scrollbar(self.is_menu_open())
            .build(|| {
                let logs = core.logs();
                if logs.len() != self.logs_emitted {
                    self.frames_since_new_logs = 0;
                    self.logs_emitted = logs.len();
                }

                for message in logs {
                    use ap::Print::*;
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
                            ItemSend { item, .. } | ItemCheat { item, .. } | Hint { item, .. }
                                if core.config().slot() == item.receiver().name()
                                    || core.config().slot() == item.sender().name() =>
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

        drop(_scrollbar_bg);
    }

    /// Renders the text box in which users can write chats to the server.
    fn render_say_input(&mut self, ui: &Ui, core: &mut Core) {
        ui.disabled(core.client().is_none(), || {
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

            if send && let Some(client) = core.client_mut() {
                // We don't have a great way to surface these errors, and
                // they're non-fatal, so just ignore them.
                let _ = client.say(mem::take(&mut self.say_input));
            }
        });
    }

    /// Returns whether the player has the menu open and so can interact with
    /// the overlay.
    fn is_menu_open(&self) -> bool {
        if let Ok(menu_man) = unsafe { MenuMan::instance() } {
            // If MapItemMan isn't available, that usually means we're on the
            // main menu. There's probably a better way to detect that but we
            // don't know it yet.
            menu_man.is_menu_mode() || unsafe { MapItemMan::instance() }.is_err()
        } else {
            false
        }
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
fn write_message_data(ui: &Ui, parts: &[RichText], alpha: u8) {
    let mut first = true;
    for part in parts {
        if !first {
            ui.same_line();
        }
        first = false;

        // TODO: Load in fonts to support bold, maybe write a line manually for
        // underline? I'm not sure there's a reasonable way to support
        // background colors.
        use RichText::*;
        use TextColor::*;
        let color = match part {
            Player { .. } | PlayerName { .. } | Color { color: Blue, .. } => BLUE,
            Item { .. } | Color { color: Magenta, .. } => MAGENTA,
            Location { .. } | EntranceName { .. } | Color { color: Cyan, .. } => CYAN,
            Color { color: Black, .. } => BLACK,
            Color { color: Red, .. } => RED,
            Color { color: Green, .. } => GREEN,
            Color { color: Yellow, .. } => YELLOW,
            _ => WHITE,
        };
        ui.text_colored(color.with_alpha(alpha).to_rgba_f32s(), part.to_string());
    }
}

/// Renders bold colored text by drawing it multiple times with 1px offset.
/// TODO: Use ImGui font loading to support proper bold rendering.
fn bold_text_colored(ui: &Ui, text: &str, color: [f32; 4]) {
    let pos = ui.cursor_pos();
    for i in 0..3 {
        ui.set_cursor_pos([pos[0] + i as f32, pos[1]]);
        ui.text_colored(color, text);
    }
}
