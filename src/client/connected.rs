use archipelago_rs::protocol::*;
use tokio::sync::mpsc::Sender;

use crate::slot_data::SlotData;

/// A pull-based client representing an established, active Archipelago
/// connection. All of the actual communication is done on a separate thread.
/// This is owned by a [ClientConnection] and its state only changes when
/// [ClientConnection.update] is called.
pub struct ConnectedClient {
    /// The information provided upon the initial connection.
    connected: archipelago_rs::protocol::Connected<SlotData>,

    /// The transmitter for messages going to the worker thread.
    tx: Sender<ClientMessage>,

    /// Buffered prints waiting to be consumed by the caller.
    prints: Vec<PrintJSON>,
}

impl ConnectedClient {
    /// Creates a new client and begins attempting to establish a connection
    /// with the given credentials.
    pub(super) fn new(
        connected: archipelago_rs::protocol::Connected<SlotData>,
        tx: Sender<ClientMessage>,
    ) -> ConnectedClient {
        ConnectedClient {
            connected,
            tx,
            prints: vec![],
        }
    }

    /// The information provided upon the initial connection.
    pub fn connected(&self) -> &archipelago_rs::protocol::Connected<SlotData> {
        &self.connected
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
            ServerMessage::Print(Print { text }) => self.prints.push(PrintJSON::message(text)),
            ServerMessage::PrintJSON(message) => self.prints.push(message),
            _ => (), // Ignore messages we don't care about
        };
    }
}
