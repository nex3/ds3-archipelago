use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use archipelago_rs::client::ArchipelagoError;
use archipelago_rs::protocol::{ItemsHandlingFlags, RichPrint};
use darksouls3::cs::*;
use darksouls3::sprj::*;
use fromsoftware_shared::{FromStatic, InstanceResult};
use log::*;

use crate::client::{ClientConnectionState::*, *};
use crate::config::Config;
use crate::item::{CategorizedItemIDExt, EquipParamExt};
use crate::save_data::*;

/// The core of the Archipelago mod. This is responsible for running the
/// non-UI-related game logic and interacting with the Archieplago client.
pub struct Core {
    /// The configuration for the current Archipelago connection. This is not
    /// guaranteed to be complete *or* accurate; it's the mod's responsibility
    /// to ensure it makes sense before actually interacting with an individual
    /// game.
    config: Config,

    /// The log of prints displayed in the overlay.
    log_buffer: Vec<RichPrint>,

    /// The Archipelago client connection.
    connection: ClientConnection,

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

    /// The last time the player either sent or received a death link (or
    /// started a session).
    last_death_link: Instant,

    /// Whether the player has achieved their goal and sent that information to
    /// the Archipelago server. This is stored here rather than in the save data
    /// so that it's resent every time the player starts the game, just in case
    /// it got lost in transit.
    sent_goal: bool,
}

/// The grace period between MapItemMan starting to exist and the mod beginning
/// to take actions.
const GRACE_PERIOD: Duration = Duration::from_secs(10);

/// The grace period after either sending or receiving a death link during which
/// no further death links will be sent or received.
const DEATH_LINK_GRACE_PERIOD: Duration = Duration::from_secs(30);

impl Core {
    /// Creates a new instance of the mod.
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let connection = Self::new_connection(&config);
        Ok(Self {
            config,
            connection,
            log_buffer: vec![],
            last_item_time: Instant::now(),
            load_time: None,
            locations_sent: 0,
            last_death_link: Instant::now(),
            sent_goal: false,
        })
    }

    /// Creates a new [ClientConnection] based on the connection information in [config].
    fn new_connection(config: &Config) -> ClientConnection {
        ClientConnection::new(
            config.url(),
            config.slot(),
            config.password().as_ref(),
            ItemsHandlingFlags::OTHER_WORLDS & ItemsHandlingFlags::STARTING_INVENTORY,
            vec!["DeathLink".to_string()],
        )
    }

    /// Returns the simplified connection state for [client].
    pub fn simple_connection_state(&self) -> SimpleConnectionState {
        match self.connection.state() {
            Disconnected(_) => SimpleConnectionState::Disconnected,
            Connecting => SimpleConnectionState::Connecting,
            Connected(_) => SimpleConnectionState::Connected,
        }
    }

    /// Returns the current user config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Updates the URL to use to connect to Archipelago and reconnects the
    /// Archipelago session.
    pub fn update_url(&mut self, url: impl AsRef<str>) -> Result<()> {
        self.config.set_url(url);
        self.config.save()?;
        self.connection = Self::new_connection(&self.config);
        Ok(())
    }

    /// Returns a reference to the Archipelago client, if it's connected.
    pub fn client(&self) -> Option<&ConnectedClient> {
        self.connection.client()
    }

    /// Returns the list of all logs that have been emitted in the current
    /// session.
    pub fn logs(&self) -> &[RichPrint] {
        self.log_buffer.as_slice()
    }

    /// A function that's run just before rendering the overlay UI in every
    /// frame render. This is where the core logic of the mod takes place.
    ///
    /// Thistakes an [error] parameter which indicates that a fatal error is
    /// currently being displayed to the user and the mod shouldn't process any
    /// more game logic. It only returns an error if the mod has hit a fatal
    /// failure and can't continue any longer.
    pub fn tick(&mut self, error: bool) -> Result<()> {
        let old_state = self.simple_connection_state();
        self.connection.update();

        if let Disconnected(err) = self.connection.state() {
            match old_state {
                SimpleConnectionState::Connecting => {
                    self.log(
                        if let Some(ArchipelagoError::IllegalResponse {
                            received: "ConnectionRefused",
                            ..
                        }) = err.downcast_ref::<ArchipelagoError>()
                        {
                            "Connection refused. Make sure the server session is running and the \
                             URL is up-to-date."
                                .to_string()
                        } else {
                            format!("Connection failed: {}", err)
                        },
                    );
                }
                SimpleConnectionState::Connected => self.log(format!("Disconnected: {}", err)),
                _ => {}
            }
        }

        self.process_new_prints();

        if error {
            return Ok(());
        }

        // Safety: It should be safe to access the item man during a frame draw,
        // since we're on the main thread.
        let item_man = unsafe { MapItemMan::instance() };
        if item_man.is_err() {
            self.load_time = None;
        } else if self.load_time.is_none() {
            self.load_time = Some(Instant::now());
        }

        if let Some(time) = self.load_time
            && time.elapsed() < GRACE_PERIOD
        {
            return Ok(());
        }

        self.check_version_conflict()?;

        self.check_seed_conflict()?;
        if let Some(save_data) = SaveData::instance_mut().as_mut()
            && save_data.seed.is_none()
        {
            save_data.seed = Some(self.config.seed().to_string());
        };

        self.check_dlc_error()?;
        self.process_incoming_items(item_man);
        self.process_inventory_items();
        self.handle_death_link();
        self.handle_goal();

        Ok(())
    }

    /// Handle new prints that come from the Archipelago server.
    fn process_new_prints(&mut self) {
        let new_prints = self
            .connection
            .client_mut()
            .map(|c| c.prints())
            .unwrap_or_default();
        for message in &new_prints {
            info!("[APS] {message}");
        }
        self.log_buffer.extend(new_prints);
    }

    /// Returns an error if the user's static randomizer version doesn't match
    /// this mod's version.
    fn check_version_conflict(&self) -> Result<()> {
        if let Some(client_version) = self.config().client_version()
            && client_version != env!("CARGO_PKG_VERSION")
        {
            bail!(
                "Your apconfig.json was generated using static randomizer v{}, but this client is \
                 v{}. Re-run the static randomizer with the current version.",
                client_version,
                env!("CARGO_PKG_VERSION"),
            );
        } else {
            Ok(())
        }
    }

    /// Returns an error if there's a conflict between the notion of the current
    /// seed in the server, the save, and/or the config. Also updates the save
    /// data's notion based on whatever is available if it doesn't exist yet.
    fn check_seed_conflict(&mut self) -> Result<()> {
        let client_seed = self
            .connection
            .client()
            .map(|c| c.room_info().seed_name.as_str());
        let save = SaveData::instance();
        let save_seed = save.as_ref().and_then(|s| s.seed.as_ref());

        match (client_seed, save_seed) {
            (Some(client_seed), _) if client_seed != self.config.seed() => bail!(
                "You've connected to a different Archipelago multiworld than the one that \
                 DS3Randomizer.exe used!\n\
                 \n\
		 Connected room seed: {}\n\
                 DS3Randomizer.exe seed: {}",
                client_seed,
                self.config.seed()
            ),
            (Some(client_seed), Some(save_seed)) if client_seed != save_seed => bail!(
                "You've connected to a different Archipelago multiworld than the one that \
                 you used before with this save!\n\
                 \n\
		 Connected room seed: {}\n\
		 Save file seed: {}",
                client_seed,
                save_seed
            ),
            (_, Some(save_seed)) if self.config.seed() != save_seed => bail!(
                "Your most recent DS3Randomizer.exe invocation connected to a different \
                 Archipealgo multiworld than the one that you used before with this save!\n\
                 \n\
                 DS3Randomizer.exe seed: {}\n\
                 Save file seed: {}",
                self.config.seed(),
                save_seed
            ),
            _ => Ok(()),
        }
    }

    /// Returns an error if [config] expects DLC to be installed and it is not.
    fn check_dlc_error(&self) -> Result<()> {
        if let Connected(client) = self.connection.state() &&
            let Ok(dlc) = (unsafe { CSDlc::instance() }) &&
            // The DLC always registers as not installed until the player clicks
            // through the initial opening screen and loads their global save
            // data. Ideally we should find a better way of detecting when that
            // happens, but for now we just wait to indicate an error until
            // they're actually in a game.
            (unsafe { MapItemMan::instance() }).is_ok() &&
            client.connected().slot_data.options.enable_dlc
            && (!dlc.dlc1_installed || !dlc.dlc2_installed)
        {
            bail!(
                "DLC is enabled for this seed but your game is missing {}.",
                if dlc.dlc1_installed {
                    "the Ringed City DLC"
                } else if dlc.dlc2_installed {
                    "the Ashes of Ariandel DLC"
                } else {
                    "both DLCs"
                }
            );
        } else {
            Ok(())
        }
    }

    /// Handle new items, distributing them to the player when appropriate. This
    /// also initializes the [SaveData] for a new file.
    fn process_incoming_items(&mut self, item_man: InstanceResult<&mut MapItemMan>) {
        let Connected(client) = self.connection.state_mut() else {
            return;
        };
        let Ok(item_man) = item_man else {
            return;
        };
        let Ok(player_game_data) = (unsafe { PlayerGameData::instance() }) else {
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
            .find(|item| item.index() >= save_data.items_granted)
        {
            info!(
                "Granting {} (AP ID {}, DS3 ID {:?}{})",
                item.ap_name(),
                item.ap_id(),
                item.ds3_id(),
                if let Some(location) = item.location_name() {
                    format!("from {}", location)
                } else {
                    "".to_string()
                }
            );

            // Grant Path of the Dragon as a gesture rather than an item.
            if item.ds3_id().category() == ItemCategory::Goods
                && item.ds3_id().uncategorized().value() == 9030
            {
                player_game_data.grant_gesture(29, item.ds3_id());
            } else {
                item_man.grant_item(ItemBufferEntry {
                    id: item.ds3_id(),
                    quantity: item.quantity(),
                    durability: -1,
                });
            }

            save_data.items_granted += 1;
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

        // We have to make a separate vector here so we aren't borrowing while
        // we make mutations.
        let ids = game_data_man
            .main_player_game_data
            .equipment
            .equip_inventory_data
            .items_data
            .items()
            .map(|e| e.item_id)
            .collect::<Vec<_>>();
        for id in ids {
            if !id.is_archipelago() {
                continue;
            }

            info!("Inventory contains Archipelago item {:?}", id);
            let row = regulation_manager
                .get_equip_param(id)
                .unwrap_or_else(|| panic!("no row defined for Archipelago ID {:?}", id));

            info!("  Archipelago location: {}", row.archipelago_location_id());
            save_data.locations.insert(row.archipelago_location_id());

            if let Some(good) = row.as_goods() && good.icon_id() == 7039
            {
                info!("  Item is Path of the Dragon, granting gesture");
                // If the player gets the synthetic Path of the Dragon item,
                // give them the gesture itself instead. Don't display an
                // item pop-up, because they already saw one when they got
                // the item.
                game_data_man
                    .main_player_game_data
                    .gesture_data
                    .set_gesture_acquired(29, true);
                info!("  Removing from inventory");
                game_data_man.remove_item(id, 1);
            } else if let Some((real_id, quantity)) = row.archipelago_item() {
                info!("  Converting to {}x {:?}", quantity, real_id);
                game_data_man.give_item_directly(real_id, quantity.try_into().unwrap());
                info!("  Removing from inventory");
                game_data_man.remove_item(id, 1);
            } else {
                info!(
                    "  Item has no Archipelago metadata. Basic price: {}, sell value: {}{}",
                    row.basic_price(),
                    row.sell_value(),
                    if let Some(good) = row.as_goods() {
                        format!(", icon id: {}", good.icon_id())
                    } else { 
                        "".into()
                    }
                );
            }
        }

        if let Connected(client) = self.connection.state_mut()
            && save_data.locations.len() > self.locations_sent
        {
            client.location_checks(save_data.locations.iter().copied());
            self.locations_sent = save_data.locations.len();
        }
    }

    /// Kills the player after a death link is received and sends a death link
    /// when the player dies.
    pub fn handle_death_link(&mut self) {
        if self.last_death_link.elapsed() < DEATH_LINK_GRACE_PERIOD {
            return;
        }
        let Connected(client) = self.connection.state_mut() else {
            return;
        };
        let Ok(player) = (unsafe { PlayerIns::instance() }) else {
            return;
        };
        if !client.connected().slot_data.options.death_link {
            return;
        }

        if client.death_link().is_some() {
            player.kill();
            self.last_death_link = Instant::now();
        } else if player.super_chr_ins.modules.data.hp == 0 {
            client.send_death_link();
            self.last_death_link = Instant::now();
        }
    }

    /// Detects when the player has won the game and notifies the server.
    pub fn handle_goal(&mut self) {
        if let Ok(event_man) = (unsafe { SprjEventFlagMan::instance() })
            && let Connected(client) = self.connection.state()
            && !self.sent_goal
            && event_man.get_flag(14100800.try_into().unwrap())
        {
            client.send_goal();
            self.sent_goal = true;
        }
    }

    /// The player's slot ID, if it's known.
    pub fn slot(&self) -> Option<i64> {
        self.connection.client().map(|c| c.connected().slot)
    }

    /// Writes a message to the log buffer that we display to the user in the
    /// overlay, as well as to the internal logger.
    fn log(&mut self, message: impl AsRef<str>) {
        let message_ref = message.as_ref();
        info!("[APC] {message_ref}");
        // Consider making this a circular buffer if it ends up eating too much
        // memory over time.
        self.log_buffer
            .push(RichPrint::message(message_ref.to_string()));
    }
}

/// A simplification of [ClientConnectionState] that doesn't contain any
/// actual data and thus doesn't need to worry about lifetimes.
pub enum SimpleConnectionState {
    Disconnected,
    Connecting,
    Connected,
}
