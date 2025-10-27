use archipelago_rs::protocol::*;
use tokio::sync::mpsc::Sender;

use crate::client::{GameDataWrapper, Item};
use crate::slot_data::{I64Key, SlotData};

/// A pull-based client representing an established, active Archipelago
/// connection. All of the actual communication is done on a separate thread.
/// This is owned by a [ClientConnection] and its state only changes when
/// [ClientConnection.update] is called.
pub struct ConnectedClient {
    /// The information provided upon the initial connection.
    connected: archipelago_rs::protocol::Connected<SlotData>,

    /// The room info for the current connection.
    room_info: RoomInfo,

    /// The game data for Dark Souls III.
    game_data: GameDataWrapper,

    /// The transmitter for messages going to the worker thread.
    tx: Sender<ClientMessage>,

    /// Buffered received items waiting to be consumed by the caller.
    items: Vec<Item>,

    /// Buffered prints waiting to be consumed by the caller.
    prints: Vec<PrintJSON>,
}

impl ConnectedClient {
    /// Creates a new client and begins attempting to establish a connection
    /// with the given credentials.
    pub(super) fn new(
        connected: archipelago_rs::protocol::Connected<SlotData>,
        room_info: RoomInfo,
        game_data: GameDataWrapper,
        tx: Sender<ClientMessage>,
    ) -> ConnectedClient {
        ConnectedClient {
            connected,
            room_info,
            game_data,
            tx,
            items: vec![],
            prints: vec![],
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

    /// Returns all Archipelago prints that have been received since the last
    /// time this message was called.
    ///
    /// These are only refreshed when [update] is called.
    pub fn prints(&mut self) -> Vec<PrintJSON> {
        std::mem::take(&mut self.prints)
    }

    /// Sends a message to the server and other clients.
    pub fn say(&mut self, text: impl AsRef<str>) {
        self.tx
            .blocking_send(ClientMessage::Say(Say {
                text: text.as_ref().to_string(),
            }))
            .unwrap();
    }

    /// Processes any incoming messages from the worker thread and updates the
    /// client's state accordingly.
    pub(super) fn update(&mut self, message: ServerMessage<SlotData>) {
        match message {
            ServerMessage::ReceivedItems(message) => {
                self.items.extend(message.items.into_iter().map(|ap| {
                    let name = self.game_data.item_name(ap.item);
                    let location = self.game_data.location_name(ap.location);
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
            ServerMessage::Print(Print { text }) => self.prints.push(PrintJSON::message(text)),
            ServerMessage::PrintJSON(message) => self.prints.push(message),
            _ => (), // Ignore messages we don't care about
        };
    }
}
