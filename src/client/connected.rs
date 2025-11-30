use std::time::SystemTime;

use archipelago_rs::protocol::*;
use tokio::sync::mpsc::Sender;

use crate::client::Item;
use crate::slot_data::{I64Key, SlotData};

/// The Archipelago name for this game. THis must match Archipelago's world
/// name exactly.
const GAME_NAME: &str = "Dark Souls III";

/// A pull-based client representing an established, active Archipelago
/// connection. All of the actual communication is done on a separate thread.
/// This is owned by a [ClientConnection] and its state only changes when
/// [ClientConnection.update] is called.
pub struct ConnectedClient {
    /// The information provided upon the initial connection.
    connected: archipelago_rs::protocol::Connected<SlotData>,

    /// The room info for the current connection.
    room_info: RoomInfo,

    /// The Archipelago data package.
    data_package: DataPackageObject,

    /// The transmitter for messages going to the worker thread.
    tx: Sender<ClientMessage>,

    /// Buffered received items waiting to be consumed by the caller.
    items: Vec<Item>,

    /// Buffered prints waiting to be consumed by the caller.
    prints: Vec<RichPrint>,

    /// The most recent death link that hasn't yet been consumed by the caller.
    death_link: Option<DeathLink>,
}

impl ConnectedClient {
    /// Creates a new client and begins attempting to establish a connection
    /// with the given credentials.
    pub(super) fn new(
        connected: archipelago_rs::protocol::Connected<SlotData>,
        room_info: RoomInfo,
        data_package: DataPackageObject,
        tx: Sender<ClientMessage>,
    ) -> ConnectedClient {
        ConnectedClient {
            connected,
            room_info,
            data_package,
            tx,
            items: vec![],
            prints: vec![],
            death_link: None,
        }
    }

    /// The information provided upon the initial connection.
    pub fn connected(&self) -> &archipelago_rs::protocol::Connected<SlotData> {
        &self.connected
    }

    /// The room information for the current session.
    pub fn room_info(&self) -> &RoomInfo {
        &self.room_info
    }

    /// Returns all Archipelago items that have been received by the player
    /// during their entire run of the game.
    pub fn items(&self) -> &[Item] {
        self.items.as_ref()
    }

    /// Consumes and returns all Archipelago prints that have been received
    /// since the last time this method was called.
    ///
    /// These are only refreshed when [update] is called.
    pub fn prints(&mut self) -> Vec<RichPrint> {
        std::mem::take(&mut self.prints)
    }

    /// Consumes and returns a death link that has been received since the last
    /// time this method was called.
    ///
    /// This is only refreshed when [update] is called. If multiple death links
    /// have been received, this will return the most recent one.
    pub fn death_link(&mut self) -> Option<DeathLink> {
        std::mem::take(&mut self.death_link)
    }

    /// Sends a message to the server and other clients.
    pub fn say(&self, text: impl AsRef<str>) {
        self.tx
            .blocking_send(ClientMessage::Say(Say {
                text: text.as_ref().to_string(),
            }))
            .unwrap();
    }

    /// Notifies the server that the given [locations] have been accessed.
    pub fn location_checks(&self, locations: impl IntoIterator<Item = i64>) {
        self.tx
            .blocking_send(ClientMessage::LocationChecks(LocationChecks {
                locations: locations.into_iter().collect(),
            }))
            .unwrap();
    }

    pub fn send_death_link(&self) {
        self.tx
            .blocking_send(ClientMessage::Bounce(Bounce {
                games: None,
                slots: None,
                tags: vec![],
                data: BounceData::DeathLink(DeathLink {
                    time: SystemTime::now(),
                    cause: None,
                    source: self.room_info.seed_name.clone(),
                }),
            }))
            .unwrap();
    }

    /// Processes any incoming messages from the worker thread and updates the
    /// client's state accordingly.
    pub(super) fn update(&mut self, message: ServerMessage<SlotData>) {
        match message {
            ServerMessage::ReceivedItems(message) => {
                self.items.extend(message.items.into_iter().map(|ap| {
                    let name = self
                        .data_package
                        .games
                        .get(GAME_NAME)
                        .expect(&format!("Expected game data for {}", GAME_NAME))
                        .item_id_to_name()
                        .get(&ap.item)
                        .expect("Expected item ID to have a name")
                        .clone();
                    let location = self
                        .data_package
                        .games
                        .get(GAME_NAME)
                        .and_then(|g| g.location_id_to_name().get(&ap.location))
                        .map(|n| n.clone());
                    let id_key = I64Key(ap.item);
                    let ds3_id = self
                        .connected
                        .slot_data
                        .ap_ids_to_item_ids
                        .get(&id_key)
                        .expect("Archipelago ID should have a DS3 ID defined in slot data");
                    let quantity = self
                        .connected
                        .slot_data
                        .item_counts
                        .get(&id_key)
                        .map(|n| *n)
                        .unwrap_or(1);
                    Item::new(ap, name, location, ds3_id.0, quantity)
                }))
            }
            ServerMessage::Print(Print { text }) => self.prints.push(RichPrint::message(text)),
            ServerMessage::RichPrint(mut message) => {
                message.add_names(&self.connected, &self.data_package);
                self.prints.push(message);
            },
            ServerMessage::Bounced(Bounced {
                data: BounceData::DeathLink(death_link),
                ..
            }) => self.death_link = Some(death_link),
            _ => (), // Ignore messages we don't care about
        };
    }
}
