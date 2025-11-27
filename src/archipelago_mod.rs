use std::any::Any;
use std::time::Instant;

use archipelago_rs::protocol::{ItemsHandlingFlags, PrintJSON};
use darksouls3::cs::*;
use darksouls3::param::EQUIP_PARAM_GOODS_ST;
use darksouls3::sprj::*;
use fromsoftware_shared::{FromStatic, InstanceResult};
use log::*;

use crate::client::{ClientConnectionState::*, *};
use crate::config::Config;
use crate::item::{CategorizedItemIDExt, EquipParamExt};
use crate::save_data::*;

/// The fully-initialized Archipelago mod at the whole-game level.
pub struct ArchipelagoMod {
    /// The configuration for the current Archipelago connection. This is not
    /// guaranteed to be complete *or* accurate; it's the mod's responsibility
    /// to ensure it makes sense before actually interacting with an individual
    /// game.
    config: Config,

    /// The log of prints displayed in the overlay.
    log_buffer: Vec<PrintJSON>,

    /// The Archipelago client connection, if it's connected.
    connection: Option<ClientConnection>,

    /// The time we last granted an item to the player. Used to ensure we don't
    /// give more than one item per second.
    last_item_time: Instant,

    /// The time at which we noticed the game loading (as indicated by
    /// MapItemMan coming into existence). Used to compute the grace period
    /// before we start doing stuff in game. None if the game is not currently
    /// loaded.
    load_time: Option<Instant>,

    /// The number of locations sent to the server in this session. This always
    /// starts at 0 when the player boots the game again to ensure that they
    /// resend any locations that may have been missed.
    locations_sent: usize,
}

impl ArchipelagoMod {
    /// Creates a new instance of the mod.
    pub fn new() -> ArchipelagoMod {
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
            locations_sent: 0,
        };

        if ap_mod.config.url().is_some() && ap_mod.config.slot().is_some() {
            ap_mod.connect();
        }

        ap_mod
    }

    /// Returns the simplified connection state for [client].
    pub fn simple_connection_state(&self) -> SimpleConnectionState {
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

    /// Returns the current user config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Update the user config with the given fields.
    pub fn update_config(
        &mut self,
        url: impl AsRef<str>,
        slot: impl AsRef<str>,
        password: Option<impl AsRef<str>>,
    ) -> Result<(), String> {
        self.config.set_url(url);
        self.config.set_slot(slot);
        self.config.set_password(password);
        self.config.save()
    }

    /// Returns a reference to the Archipelago client, if it's connected.
    pub fn client(&self) -> Option<&ConnectedClient> {
        if let Some(connection) = self.connection.as_ref() {
            match connection.state() {
                Connected(client) => Some(client),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Returns the list of all logs that have been emitted in the current
    /// session.
    pub fn logs(&self) -> &[PrintJSON] {
        self.log_buffer.as_slice()
    }

    /// A function that's run just before rendering the overlay UI in every
    /// frame render. This is where the core logic of the mod takes place.
    pub fn tick(&mut self) {
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

        if let Some(connection) = self.connection.as_ref()
            && let Connected(client) = connection.state()
            && let Some(save_data) = SaveData::instance_mut().as_mut()
        {
            if save_data.seed.is_none() {
                // If the connection seed exists and the saved seed doesn't,
                // we're presumably on a new file. Write the connection seed to
                // the save data so we can surface conflicts in the future.
                save_data.seed = Some(client.room_info().seed_name.clone());
            } else if !save_data.seed_matches(&client.room_info().seed_name) {
                // If there's an unresolved conflict between the saved and
                // connected seeds, don't make any changes until it's resolved.
                return;
            }
        }

        self.process_incoming_items(item_man);
        self.process_inventory_items();
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
        for message in &new_prints {
            info!("[APS] {message}");
        }
        self.log_buffer.extend(new_prints);
    }

    /// Handle new items, distributing them to the player when appropriate. This
    /// also initializes the [SaveData] for a new file.
    fn process_incoming_items(&mut self, item_man: InstanceResult<&mut MapItemMan>) {
        let Some(connection) = self.connection.as_mut() else {
            return;
        };
        let Connected(client) = connection.state_mut() else {
            return;
        };
        let Ok(item_man) = item_man else {
            return;
        };
        let mut save_data = SaveData::instance_mut();
        let Some(save_data) = save_data.as_mut() else {
            return;
        };

        // Wait a second between each item grant, and 10 seconds after we load
        // in before we start granting items at all.
        if self.last_item_time.elapsed().as_secs() < 1
            || self.load_time.is_none_or(|i| i.elapsed().as_secs() < 10)
        {
            return;
        }

        if let Some(item) = client
            .items()
            .iter()
            .find(|item| save_data.items_granted.insert(item.ap_id()))
        {
            item_man.grant_item(ItemBufferEntry {
                id: item.ds3_id(),
                quantity: item.quantity(),
                durability: -1,
            });
            self.last_item_time = Instant::now();
        }
    }

    /// Removes any placeholder items from the player's inventory and notifies
    /// the server that they've been accessed.
    fn process_inventory_items(&mut self) {
        let Some(ref mut save_data) = SaveData::instance_mut() else {
            return;
        };
        let Ok(game_data_man) = (unsafe { GameDataMan::instance() }) else {
            return;
        };
        let Ok(regulation_manager) = (unsafe { CSRegulationManager::instance() }) else {
            return;
        };

        let archipelago_item_ids = game_data_man
            .main_player_game_data
            .equipment
            .equip_inventory_data
            .items_data
            .items()
            .map(|entry| entry.item_id)
            .filter(|id| id.is_archipelago())
            .collect::<Vec<_>>();

        if archipelago_item_ids.len() != 0 {
            for id in archipelago_item_ids {
                let row = regulation_manager
                    .get_equip_param(id)
                    .expect("no row defined for Archipelago ID");

                save_data.locations.insert(row.archipelago_location_id());
                if let Some(good) = (&row as &dyn Any).downcast_ref::<EQUIP_PARAM_GOODS_ST>()
                    && good.icon_id() == 7039
                {
                    todo!();
                } else if let Some((real_id, quantity)) = row.archipelago_item() {
                    game_data_man.add_or_remove_item(real_id, quantity.try_into().unwrap());
                }
                game_data_man.add_or_remove_item(id, -1);
            }
        }

        if let Some(connection) = self.connection.as_mut()
            && let Connected(client) = connection.state_mut()
            && save_data.locations.len() > self.locations_sent
        {
            client.location_checks(save_data.locations.iter().copied());
            self.locations_sent = save_data.locations.len();
        }
    }

    /// The player's slot ID, if it's known.
    pub fn slot(&self) -> Option<i64> {
        Some(self.client()?.connected().slot)
    }

    /// Initiates a new connection to the Archipelago server based on the data
    /// in the [config]. As a precondition, this requires the config's URL and
    /// slot to be set.
    pub fn connect(&mut self) {
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
    }
}

/// A simplification of [ClientConnectionState] that doesn't contain any
/// actual data and thus doesn't need to worry about lifetimes.
pub enum SimpleConnectionState {
    Disconnected,
    Connecting,
    Connected,
}
