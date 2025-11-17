use std::time::Instant;

use archipelago_rs::protocol::{ItemsHandlingFlags, JSONColor, JSONMessagePart, PrintJSON};
use darksouls3::sprj::*;
use darksouls3::util::input::*;
use fromsoftware_shared::{FromStatic, InstanceResult};
use hudhook::{ImguiRenderLoop, RenderContext};
use imgui::*;
use log::*;

use crate::client::{ClientConnectionState::*, *};
use crate::clipboard_backend::WindowsClipboardBackend;
use crate::config::Config;
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

/// The fully-initialized Archipelago mod at the whole-game level.
pub struct ArchipelagoMod {
    /// The configuration for the current Archipelago connection. This is not
    /// guaranteed to be complete *or* accurate; it's the mod's responsibility
    /// to ensure it makes sense before actually interacting with an individual
    /// game.
    config: Config,

    /// The Archipelago client connection, if it's connected.
    connection: Option<ClientConnection>,

    /// The log of prints displayed in the overlay.
    log_buffer: Vec<PrintJSON>,

    /// The time we last granted an item to the player. Used to ensure we don't
    /// give more than one item per second.
    last_item_time: Instant,

    /// The time at which we noticed the game loading (as indicated by
    /// MapItemMan coming into existence). Used to compute the grace period
    /// before we start doing stuff in game. None if the game is not currently
    /// loaded.
    load_time: Option<Instant>,

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

    /// The number of frames that have elapsed since new logs were last added.
    /// We use this to determine when to auto-scroll the log window.
    frames_since_new_logs: i64,
}

// Safety: The sole ArchipelagoMod instance is owned by Hudhook, which only ever
// interacts with it during frame rendering. We know DS3 frame rendering always
// happens on the main thread, and never in parallel, so synchronization is not
// a real concern.
unsafe impl Sync for ArchipelagoMod {}

impl ArchipelagoMod {
    /// Creates a new instance of the mod.
    pub fn new(input_blocker: &'static InputBlocker) -> ArchipelagoMod {
        let config = match Config::load_or_default() {
            Ok(config) => config,
            Err(e) => panic!("Failed to load config: {e:?}"),
        };

        let mut ap_mod = ArchipelagoMod {
            config,
            connection: None,
            log_buffer: vec![],
            last_item_time: Instant::now(),
            load_time: None,
            input_blocker,
            viewport_size: None,
            popup_url: String::new(),
            popup_slot: String::new(),
            popup_password: String::new(),
            say_input: String::new(),
            log_was_scrolled_down: false,
            frames_since_new_logs: 0,
        };

        if ap_mod.config.url().is_some() && ap_mod.config.slot().is_some() {
            ap_mod.connect();
        }

        return ap_mod;
    }

    /// Returns the simplified connection state for [client].
    fn simple_connection_state(&self) -> SimpleConnectionState {
        if let Some(connection) = self.connection.as_ref() {
            match connection.state() {
                Disconnected(_) => SimpleConnectionState::Disconnected,
                Connecting => SimpleConnectionState::Connecting,
                Connected(_) => SimpleConnectionState::Connected,
            }
        } else {
            SimpleConnectionState::Disconnected
        }
    }

    /// Returns a reference to the Archipelago client, if it's connected.
    fn client(&self) -> Option<&ConnectedClient> {
        if let Some(connection) = self.connection.as_ref() {
            match connection.state() {
                Connected(client) => Some(client),
                _ => None,
            }
        } else {
            None
        }
    }

    /// A function that's run just before rendering the overlay UI in every
    /// frame render. This is where the core logic of the mod takes place.
    fn tick(&mut self) {
        // We can't pattern match here because we need to use self.connection
        // mutably while also calling `self.log()` which is a different mutable
        // self borrow.
        if self.connection.is_some() {
            let old_state = self.simple_connection_state();

            {
                let connection = self.connection.as_mut().unwrap();
                connection.update();
            }

            if let Disconnected(err) = self.connection.as_ref().unwrap().state() {
                match old_state {
                    SimpleConnectionState::Connecting => {
                        self.log(format!("Connection failed: {}", err));
                    }
                    SimpleConnectionState::Connected => self.log(format!("Disconnected: {}", err)),
                    _ => {}
                }
            }
        }

        // Safety: It should be safe to access the item man during a frame draw,
        // since we're on the main thread.
        let item_man = unsafe { MapItemMan::instance() };
        if item_man.is_err() {
            self.load_time = None;
        } else if self.load_time.is_none() {
            self.load_time = Some(Instant::now());
        }

        self.process_new_prints();

        // If there's an unresolved conflict between the saved and connected
        // seeds, don't make any changes until it's resolved.
        if let Some(connection) = self.connection.as_ref()
            && let Connected(client) = connection.state()
            && let Some(save_data) = SaveData::instance().as_ref()
            && save_data.seed != client.room_info().seed_name
        {
            return;
        }

        self.process_items(item_man);
    }

    /// Handle new prints that come from the Archipelago server.
    fn process_new_prints(&mut self) {
        let Some(connection) = self.connection.as_mut() else {
            return;
        };
        let Connected(client) = connection.state_mut() else {
            return;
        };

        let new_prints = client.prints();
        if new_prints.len() > 0 {
            self.frames_since_new_logs = 0;
        }
        for message in &new_prints {
            info!("[APS] {message}");
        }
        self.log_buffer.extend(new_prints);
    }

    /// Handle new items, distributing them to the player when appropriate. This
    /// also initializes the [SaveData] for a new file.
    fn process_items(&mut self, item_man: InstanceResult<&mut MapItemMan>) {
        let Some(connection) = self.connection.as_mut() else {
            return;
        };
        let Connected(client) = connection.state_mut() else {
            return;
        };
        let Ok(item_man) = item_man else {
            return;
        };

        // Only set save data once [MapItemMan] is loaded, because that
        // means we're actually in a game.
        let mut save_data = SaveData::instance_mut();
        if save_data.is_none() {
            *save_data = Some(SaveData {
                items_granted: 0,
                seed: client.room_info().seed_name.clone(),
            });
        }
        let save_data = save_data.as_mut().unwrap();

        // Wait a second between each item grant, and 10 seconds after we load
        // in before we start granting items at all.
        if self.last_item_time.elapsed().as_secs() < 1
            || self.load_time.is_none_or(|i| i.elapsed().as_secs() < 10)
        {
            return;
        }

        if client.items().len() > save_data.items_granted {
            let item = &client.items()[save_data.items_granted];
            item_man.grant_item(ItemBufferEntry {
                id: item.ds3_id(),
                quantity: item.quantity(),
                durability: -1,
            });
            save_data.items_granted += 1;
            self.last_item_time = Instant::now();
        }
        // Make sure that there are items queued up to add before we
        // invalidate the previous list of items. This avoids a race
        // condition where we might think [SaveData.items_granted]
        // was too large before we received the initial list of
        // items in the first place.
        else if client.items().len() > 0 && client.items().len() < save_data.items_granted {
            let message = format!(
                "The number of items your save has recorded ({}) is greater \
                 than the number of items Archipelago thinks you've received \
                 ({}). This probably means that your local Archipelago save \
                 has been corrupted. The client will fix this automatically, \
                 but you'll end up receiving all your items again.",
                save_data.items_granted,
                client.items().len(),
            );
            warn!("{}", message);
            self.log_buffer.push(PrintJSON::Unknown {
                data: vec![JSONMessagePart::Color {
                    text: "Warning:".to_string(),
                    color: JSONColor::Red,
                }],
            });
            save_data.items_granted = 0;
        }
    }

    /// The player's slot ID, if it's known.
    fn slot(&self) -> Option<i32> {
        Some(self.client()?.connected().slot)
    }

    /// Initiates a new connection to the Archipelago server based on the data
    /// in the [config]. As a precondition, this requires the config's URL and
    /// slot to be set.
    fn connect(&mut self) {
        self.connection = Some(ClientConnection::new(
            self.config.url().unwrap(),
            self.config.slot().unwrap(),
            self.config.password(),
            ItemsHandlingFlags::OTHER_WORLDS & ItemsHandlingFlags::STARTING_INVENTORY,
            vec![],
        ));
    }

    /// Writes a message to the log buffer that we display to the user in the
    /// overlay, as well as to the internal logger.
    fn log(&mut self, message: impl AsRef<str>) {
        let message_ref = message.as_ref();
        info!("[APC] {message_ref}");
        // Consider making this a circular buffer if it ends up eating too much
        // memory over time.
        self.log_buffer
            .push(PrintJSON::message(message_ref.to_string()));
        self.frames_since_new_logs = 0;
    }

    // Rendering

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
                            self.config.set_url(&self.popup_url);
                            self.config.set_slot(&self.popup_slot);
                            self.config.set_password(if self.popup_password.len() == 0 {
                                None
                            } else {
                                Some(&self.popup_password)
                            });

                            if let Err(e) = self.config.save() {
                                error!("Failed to save config: {e}");
                            }
                            self.connect();
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
        match self.simple_connection_state() {
            SimpleConnectionState::Connected => ui.text_colored(RED.to_rgba_f32s(), "Connected"),
            SimpleConnectionState::Connecting => ui.text("Connecting..."),
            SimpleConnectionState::Disconnected => {
                ui.text_colored(RED.to_rgba_f32s(), "Disconnected");
                ui.same_line();
                if ui.button("Connect") {
                    ui.open_popup("#connect");
                    copy_from_or_clear(&mut self.popup_url, self.config.url());
                    copy_from_or_clear(&mut self.popup_slot, self.config.slot());
                    copy_from_or_clear(&mut self.popup_password, self.config.password());
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
                for message in &self.log_buffer {
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
                                if self.slot().is_some_and(|s| *receiving == s) =>
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
        ui.disabled(self.client().is_none(), || {
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

            if send
                && let Some(connection) = self.connection.as_mut()
                && let Connected(client) = connection.state_mut()
            {
                client.say(&self.say_input);
                self.say_input.clear();
            }
        });
    }

    /// Renders the popup window alerting the user that their connected seed
    /// doesn't match their saved seed.
    fn render_seed_conflict_popup(&mut self, ui: &Ui) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let Connected(client) = connection.state() else {
            return;
        };
        let mut save_data = SaveData::instance_mut();
        let Some(save_data) = save_data.as_mut() else {
            return;
        };
        if save_data.seed == client.room_info().seed_name {
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
                    save_data.seed,
                    client.room_info().seed_name,
                ));

                if ui.button("Overwrite") {
                    save_data.seed = client.room_info().seed_name.clone();
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

impl ImguiRenderLoop for ArchipelagoMod {
    fn render(&mut self, ui: &mut Ui) {
        self.tick();

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
                self.render_seed_conflict_popup(ui);
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

/// A simplification of [ClientConnectionState] that doesn't contain any
/// actual data and thus doesn't need to worry about lifetimes.
enum SimpleConnectionState {
    Disconnected,
    Connecting,
    Connected,
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
