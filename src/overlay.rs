use std::mem;

use archipelago_rs::{self as ap, Print, RichText};
use darksouls3::sprj::{MapItemMan, MenuMan};
use fromsoftware_shared::FromStatic;
use hudhook::RenderContext;
use imgui::*;
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

    /// All logs currently displayed in the log window.
    logs: Vec<Log>,

    /// Whether the log was previously scrolled all the way down.
    log_was_scrolled_down: bool,

    /// The number of frames that have elapsed since new logs were last added.
    /// We use this to determine when to auto-scroll the log window.
    frames_since_new_logs: u64,

    /// The current font scale for the overlay UI.
    font_scale: f32,

    /// Whether the font size has changed since the last frame.
    font_size_changed: bool,

    /// The unfocused window opacity for the overlay UI.
    unfocused_window_opacity: f32,

    /// Whether to wrap text in the log window.
    wrap_text: bool,

    /// Whether the settings window is currently visible.
    settings_window_visible: bool,

    /// Whether the overlay window was focused in the previous frame.
    was_window_focused: bool,

    /// Whether compact mode was enabled in the previous frame.
    was_compact_mode: bool,

    /// The size of the main overlay window in the previous frame. Used to
    /// resize when entering and exiting compact mode.
    previous_size: Option<[f32; 2]>,
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
            logs: Vec::new(),
            log_was_scrolled_down: false,
            frames_since_new_logs: 0,
            font_scale: 1.8,
            font_size_changed: true,
            unfocused_window_opacity: 0.4,
            wrap_text: true,
            settings_window_visible: false,
            was_window_focused: false,
            was_compact_mode: true,
            previous_size: None,
        }
    }

    /// Like [ImguiRenderLoop::render], but takes a reference to [Core] as well.
    ///
    /// We don't store `core` directly in the overlay so that we can ensure that
    /// its mutex is only locked once per render.
    pub fn render(&mut self, ui: &mut Ui, core: &mut Core) {
        self.render_main_window(ui, core);
        self.render_settings_window(ui);
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

        // Set the font scale here because we need the frame height later to
        // calculate the main window size, which depends on it.
        ctx.io_mut().font_global_scale = self.font_scale;
    }

    /// Render the primary overlay window and any popups it opens.
    fn render_main_window(&mut self, ui: &Ui, core: &mut Core) {
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
            .window(format!(
                "Archipelago Client {} [{}]###ap-client-overlay",
                env!("CARGO_PKG_VERSION"),
                match core.connection_state_type() {
                    ap::ConnectionStateType::Connected => "Connected",
                    ap::ConnectionStateType::Connecting => "Connecting...",
                    ap::ConnectionStateType::Disconnected => "Disconnected",
                }
            ))
            .position([viewport_size[0] - 30., 30.], Condition::FirstUseEver)
            .position_pivot([1., 0.])
            .menu_bar(true);

        // When the menu opens or closes, add or remove space from the bottom of
        // the overlay for the message bar and horizontal scrollbar.
        let is_compact_mode = self.is_compact_mode(core);
        builder = match (self.previous_size, is_compact_mode, self.was_compact_mode) {
            (Some(size), true, false) => {
                let style = ui.clone_style();
                let mut remove_bottom_space = ui.frame_height() + style.window_padding[1];
                if !self.wrap_text {
                    remove_bottom_space += style.scrollbar_size;
                }

                builder.size(
                    [size[0], size[1] - remove_bottom_space.ceil()],
                    Condition::Always,
                )
            }
            (Some(size), false, true) => {
                let style = ui.clone_style();
                let mut add_bottom_space = ui.frame_height() + style.window_padding[1];
                if !self.wrap_text {
                    add_bottom_space += style.scrollbar_size;
                }

                builder.size(
                    [size[0], size[1] + add_bottom_space.ceil()],
                    Condition::Always,
                )
            }
            _ => builder.size([viewport_size[0] * 0.4, 300.], Condition::FirstUseEver),
        };

        builder.build(|| {
            self.render_menu_bar(ui);
            ui.separator();
            self.render_log_window(ui, core);
            if !is_compact_mode {
                if core.is_disconnected() {
                    self.render_connection_buttons(ui, core);
                } else {
                    self.render_say_input(ui, core);
                }
            }
            self.render_url_modal_popup(ui, core);

            self.was_window_focused =
                ui.is_window_focused_with_flags(WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS);
            self.was_compact_mode = is_compact_mode;
            self.previous_size = Some(ui.window_size());
        });
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
                    let _item_width = ui.push_item_width(500. * self.font_scale);
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

    /// Renders the menu bar.
    fn render_menu_bar(&mut self, ui: &Ui) {
        ui.menu_bar(|| {
            if ui.menu_item("Settings") {
                log::warn!("Click registered");
                self.settings_window_visible = true;
            }
        });
    }

    /// Renders the settings popup.
    fn render_settings_window(&mut self, ui: &Ui) {
        if !self.settings_window_visible {
            return;
        }

        let settings_bg_color = [0.0, 0.0, 0.0, 1.0];
        let _bg = ui.push_style_color(StyleColor::WindowBg, settings_bg_color);

        ui.window("Archipelago Overlay Settings")
            .position_pivot([0.5, 0.5])
            .collapsible(false)
            .build(|| {
                ui.text("Font Size ");
                ui.same_line();
                if ui.button("-##font-size-decrease-button") {
                    self.font_scale = (self.font_scale - 0.1).max(0.5);
                    self.font_size_changed = true;
                }
                ui.same_line();
                if ui.button("+##font-size-increase-button") {
                    self.font_scale = (self.font_scale + 0.1).min(4.0);
                    self.font_size_changed = true;
                }

                ui.text("Wrap Text ");
                ui.same_line();
                ui.checkbox("##wrap-text-checkbox", &mut self.wrap_text);

                let mut opacity_percent = (self.unfocused_window_opacity * 100.0).round() as i32;
                let _slider_width = ui.push_item_width(150. * self.font_scale);
                ui.text("Unfocused Opacity ");
                ui.same_line();
                ui.slider_config("##unfocused-opacity-slider", 0, 100)
                    .display_format("%d%%")
                    .build(&mut opacity_percent);
                self.unfocused_window_opacity = (opacity_percent as f32) / 100.0;

                if ui.button("Ok") {
                    self.settings_window_visible = false;
                }
            });
    }

    /// Renders the buttons that allow the player to reconnect to Archipelago.
    /// These take the place of the text box when the client is disconnected.
    fn render_connection_buttons(&mut self, ui: &Ui, core: &mut Core) {
        if ui.button("Reconnect") {
            core.reconnect();
        }

        ui.same_line();
        if ui.button("Change URL") {
            ui.open_popup("#url-modal-popup");
            core.config().url().clone_into(&mut self.popup_url);
        }
    }

    /// Renders the log window which displays all the prints sent from the server.
    fn render_log_window(&mut self, ui: &Ui, core: &mut Core) {
        let style = ui.clone_style();

        let scrollbar_bg_opacity = if self.was_window_focused { 1.0 } else { 0.0 };
        let scrollbar_bg_color = [0.0, 0.0, 0.0, scrollbar_bg_opacity];
        let _scrollbar_bg = ui.push_style_color(StyleColor::ScrollbarBg, scrollbar_bg_color);

        let _item_spacing = ui.push_style_var(StyleVar::ItemSpacing([
            style.item_spacing[0],
            style.window_padding[1],
        ]));

        let is_compact_mode = self.is_compact_mode(core);
        let input_height = if !is_compact_mode {
            ui.frame_height_with_spacing()
        } else {
            0.0
        };

        ui.child_window("#log")
            .size([0.0, -input_height.ceil()])
            .draw_background(false)
            .always_vertical_scrollbar(true)
            .always_horizontal_scrollbar(!is_compact_mode && !self.wrap_text)
            .build(|| {
                self.render_logs(ui, core);

                if self.log_was_scrolled_down && self.frames_since_new_logs < 10 {
                    ui.set_scroll_y(ui.scroll_max_y());
                }
                self.log_was_scrolled_down = ui.scroll_y() == ui.scroll_max_y();
            });
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

    /// Renders all the logs in the log window.
    fn render_logs(&mut self, ui: &Ui, core: &mut Core) {
        self.update_logs(ui, core);

        for log in self.logs.iter() {
            let alpha = get_alpha_for_print(&log.print);
            let print_data = log.print.data();
            let _word_spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0, self.font_scale]));

            if self.wrap_text {
                let space_width = ui.calc_text_size(" ")[0];

                // When true, indicates we are in the middle of rendering a
                // "spanning" word that previously didn't fit and must be
                // split character-by-character across lines.
                let mut render_span_char_by_char = false;

                for (i, group) in log.words_groups.iter().enumerate() {
                    let color = get_color_for_richtext(&print_data[i]);
                    let _color = ui
                        .push_style_color(StyleColor::Text, color.with_alpha(alpha).to_rgba_f32s());

                    let mut first_word = true;
                    for word in group {
                        let is_space = word.word.chars().nth(0) == Some(' ');

                        // If a previous word forced char-by-char splitting,
                        // then words without a precomputed width should
                        // continue splitting.
                        let mut need_char_by_char =
                            render_span_char_by_char && word.width.is_none();

                        let mut avail_width = ui.content_region_avail()[0];
                        if let Some(width) = word.width {
                            render_span_char_by_char = false;

                            // Insert an inter-word space when appropriate.
                            if !first_word && !is_space {
                                if space_width <= avail_width {
                                    ui.same_line_with_spacing(0.0, space_width);
                                } else {
                                    ui.new_line();
                                }
                                avail_width = ui.content_region_avail()[0];
                            }

                            // If the word fits in the current line, render it
                            // normally; otherwise try moving to a new line once
                            // and re-check. If it still doesn't fit, we'll
                            // fall back to char-by-char splitting.
                            if width <= avail_width {
                                ui.text(&word.word);
                                ui.same_line();
                            } else {
                                if ui.cursor_pos()[0] != 0.0 && !is_space {
                                    ui.new_line();
                                    avail_width = ui.content_region_avail()[0];
                                }
                                if width <= avail_width {
                                    ui.text(&word.word);
                                    ui.same_line();
                                } else {
                                    need_char_by_char = true;
                                    render_span_char_by_char = true;
                                }
                            }
                        }

                        if need_char_by_char {
                            // Character-by-character rendering path.
                            // Very long words are split into substrings
                            // that fit the current available width.

                            let word_chars: Vec<char> = word.word.chars().collect();
                            let mut start_idx = 0;
                            while start_idx < word_chars.len() {
                                let mut end_idx = start_idx;
                                while end_idx < word_chars.len() {
                                    let char_str: String =
                                        word_chars[start_idx..=end_idx].iter().collect();
                                    let line_width = ui.calc_text_size(&char_str)[0];
                                    if line_width > avail_width {
                                        break;
                                    }
                                    end_idx += 1;
                                }

                                // If no characters fit and the cursor is not at the start of a line,
                                // retry by inserting a newline; otherwise, force at least one
                                // character to avoid infinite loops.
                                if end_idx == start_idx {
                                    if ui.cursor_pos()[0] != 0.0 {
                                        ui.new_line();
                                        avail_width = ui.content_region_avail()[0];
                                        continue;
                                    }
                                    end_idx += 1;
                                }

                                // Avoid placing a trailing space at the end of a
                                // wrapped line: drop the last space char if the
                                // substring ends before the full word.
                                let line_str: String = if is_space && end_idx < word_chars.len() {
                                    word_chars[start_idx..end_idx - 1].iter().collect()
                                } else {
                                    word_chars[start_idx..end_idx].iter().collect()
                                };

                                ui.text(&line_str);

                                start_idx = end_idx;
                                if start_idx < word_chars.len() {
                                    // More of this word remains; refresh the
                                    // available width for the next line fragment.
                                    avail_width = ui.content_region_avail()[0];
                                } else {
                                    // Finished this word fragment; continue on
                                    // the same line for following words.
                                    ui.same_line();
                                }
                            }
                        } else if word.width.is_none() {
                            ui.text(&word.word);
                            ui.same_line();
                        }

                        if !is_space {
                            first_word = false;
                        }
                    }
                }
            } else {
                // Non-wrapping mode: simply render each group in a single line.
                for richtext in print_data.iter().take(log.words_groups.len()) {
                    let color = get_color_for_richtext(richtext);
                    let _color = ui
                        .push_style_color(StyleColor::Text, color.with_alpha(alpha).to_rgba_f32s());
                    ui.text(richtext.to_string());
                    ui.same_line();
                }
            }
            drop(_word_spacing);

            // Adds item spacing between logs.
            let _log_spacing =
                ui.push_style_var(StyleVar::ItemSpacing([0.0, 5.0 * self.font_scale]));
            ui.new_line();
        }
    }

    /// Gets new logs from [core] and update word widths if necessary.
    fn update_logs(&mut self, ui: &Ui, core: &mut Core) {
        let mut new_log_count = 0;
        for print in core.consume_logs() {
            let new_log = print.into();
            self.logs.push(new_log);
            new_log_count += 1;
        }

        if new_log_count == 0 && !self.font_size_changed {
            return;
        } else if new_log_count > 0 {
            self.frames_since_new_logs = 0;
        }

        let log_count = self.logs.len();
        let start_idx = if self.font_size_changed {
            // Recalculate widths for all logs if the font size changed.
            self.font_size_changed = false;
            0
        } else {
            // Only calculate widths for new logs.
            log_count - new_log_count
        };

        for log_idx in start_idx..log_count {
            let log = &mut self.logs[log_idx];
            let mut spanning_word_start_idx: Option<usize> = None;
            for group_idx in 0..log.words_groups.len() {
                let (prev_groups, curr_groups) = log.words_groups.split_at_mut(group_idx);
                let group = &mut curr_groups[0];
                for (word_idx, log_word) in group.iter_mut().enumerate() {
                    let log_word_width = ui.calc_text_size(&log_word.word)[0];

                    // Always calculate width for non-first words in a group.
                    if word_idx > 0 {
                        log_word.width = Some(log_word_width);
                        spanning_word_start_idx = None;
                        continue;
                    }

                    let is_space = log_word.word.chars().nth(0) == Some(' ');
                    if !is_space && let Some(start_idx) = spanning_word_start_idx {
                        // We're already in a spanning word,
                        // so add this word's width to its width.
                        let start_word = prev_groups[start_idx].last_mut().unwrap();
                        start_word.width = Some(start_word.width.unwrap() + log_word_width);
                    } else {
                        // Spaces and non-continuations always get their width calculated.
                        log_word.width = Some(log_word_width);
                        spanning_word_start_idx = None;
                    }
                }

                // After processing the group, check if it ended with a
                // non-space word to potentially start a spanning word.
                if spanning_word_start_idx.is_none()
                    && let Some(last_word) = group.last_mut()
                {
                    let last_word_is_space = last_word.word.chars().nth(0) == Some(' ');
                    if !last_word_is_space {
                        spanning_word_start_idx = Some(group_idx);
                    }
                }
            }
        }
    }

    /// Returns whether the overlay is currently in "compact mode", where the
    /// bottommost widgets are not rendered.
    fn is_compact_mode(&self, core: &Core) -> bool {
        if core.is_disconnected() {
            // When the connection is inactive, always show the buttons to
            // reconnect.
            false
        } else if let Ok(menu_man) = unsafe { MenuMan::instance() } {
            // If MapItemMan isn't available, that usually means we're on the
            // main menu. There's probably a better way to detect that but we
            // don't know it yet.
            !menu_man.is_menu_mode() && unsafe { MapItemMan::instance() }.is_ok()
        } else {
            true
        }
    }
}

/// A single log for the overlay.
struct Log {
    /// The original [Print] for this log.
    print: Print,

    /// The groups of words that make up `print`'s string.
    /// The length corresponds to the number of [RichText] items in `print.data()`.
    ///
    /// This is used to optimize rendering by pre-splitting the log into words
    /// and pre-calculating their widths.
    words_groups: Vec<Vec<LogWord>>,
}

/// A single word for a [Log].
struct LogWord {
    /// The text of the word.
    word: String,

    /// The width of the word when rendered.
    width: Option<f32>,
}

impl From<Print> for Log {
    /// Creates a [Log] from an Archipelago [Print].
    ///
    /// Converts each [RichText] element in `print.data()` into a separate
    /// group of words ([LogWord]s), enabling per-group styling during rendering.
    ///
    /// # Behavior Details
    /// - **Tokenization**: Words are extracted by splitting on ASCII space characters (`' '`).
    /// - **Leading/trailing spaces**: Merged into a single `LogWord` with all space characters preserved.
    /// - **Consecutive spaces between words**: Merged into a single `LogWord` with all but the
    ///   first space character preserved.
    ///
    /// ## Widths
    /// The `width` field is left as `None`.
    ///
    /// ## Grouping
    /// Each group's order matches `print.data()`, enabling renderers to apply
    /// matching richtext styling to the corresponding group.
    ///
    /// ### Examples:
    /// ```
    /// "hello world" → ["hello", "world"]
    /// " hello   world  " → [" ", "hello", "  ", "world", "  "]
    /// ```
    /// Empty strings result in empty word groups:
    /// ```
    /// "" → []
    /// ```
    fn from(print: Print) -> Self {
        let words_groups = print
            .data()
            .iter()
            .map(|richtext| {
                let mut words = Vec::new();
                let text = richtext.to_string();
                if text.is_empty() {
                    return words;
                }
                let split_words: Vec<String> = text.split(' ').map(|s| s.to_string()).collect();

                let mut space_counter = 0;
                for word in split_words {
                    if word.is_empty() {
                        space_counter += 1;
                    } else {
                        if space_counter > 0 {
                            let spaces = " ".repeat(space_counter);
                            words.push(LogWord {
                                word: spaces,
                                width: None,
                            });
                            space_counter = 0;
                        }
                        words.push(LogWord { word, width: None });
                    }
                }

                if space_counter > 0 {
                    let spaces = " ".repeat(space_counter);
                    words.push(LogWord {
                        word: spaces,
                        width: None,
                    });
                }

                words
            })
            .collect();

        Log {
            print,
            words_groups,
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

fn get_alpha_for_print(print: &Print) -> u8 {
    use Print::*;
    match print {
        Chat { .. }
        | ServerChat { .. }
        | Tutorial { .. }
        | CommandResult { .. }
        | AdminCommandResult { .. }
        | Unknown { .. } => 0xff,
        ItemSend { item, .. } | ItemCheat { item, .. } | Hint { item, .. }
            if item.receiver().name() == item.sender().name() =>
        {
            0xFF
        }
        _ => 0xAA,
    }
}

fn get_color_for_richtext(richtext: &RichText) -> ImColor32 {
    // TODO: Load in fonts to support bold, maybe write a line manually for
    // underline? I'm not sure there's a reasonable way to support
    // background colors.
    use RichText::*;
    use ap::TextColor::*;
    match richtext {
        Player { .. } | PlayerName { .. } | Color { color: Blue, .. } => BLUE,
        Item { .. } | Color { color: Magenta, .. } => MAGENTA,
        Location { .. } | EntranceName { .. } | Color { color: Cyan, .. } => CYAN,
        Color { color: Black, .. } => BLACK,
        Color { color: Red, .. } => RED,
        Color { color: Green, .. } => GREEN,
        Color { color: Yellow, .. } => YELLOW,
        _ => WHITE,
    }
}
