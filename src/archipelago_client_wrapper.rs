use std::thread;

use archipelago_rs::client::*;
use archipelago_rs::protocol::*;
use log::*;
use tokio::sync::mpsc::{Receiver, Sender, channel, error::TryRecvError};

/// A pull-based wrapper around the Archipelago client connection. All of the
/// actual communication is done on a separate thread. The state only changes
/// when [ArchipelagoClient.update] is called.
pub struct ArchipelagoClientWrapper {
    /// The current state of the client connection.
    state: ArchipelagoClientState,

    /// The receiver for messages coming from the worker thread.
    rx: Receiver<Result<ServerMessage, ArchipelagoError>>,

    /// The transmitter for messages going to the worker thread.
    tx: Sender<ClientMessage>,

    /// Buffered messages waiting to be consumed by the caller.
    messages: Vec<PrintJSON>,

    /// The handle of the worker thread, used to ensure that it's dropped along
    /// with the client wrapper.
    _handle: thread::JoinHandle<()>,
}

/// The state of the underlying Archipelago client as of the last received message.
pub enum ArchipelagoClientState {
    /// The client is in the process of establishing a connection and
    /// downloading the initial data.
    Connecting,

    /// The client has successfully connected. This includes data that's always
    /// available with a successful connection.
    Connected(archipelago_rs::protocol::Connected),

    /// The client is not connected, either because the initial connection
    /// failed or because an established connection was later closed. This
    /// contains a description of the reason for the closure.
    Disconnected(String),
}

impl ArchipelagoClientWrapper {
    /// Creates a new client and begins attempting to establish a connection
    /// with the given credentials.
    pub fn new(
        url: impl AsRef<str>,
        slot: impl AsRef<str>,
        password: Option<impl AsRef<str>>,
        items_handling: ItemsHandlingFlags,
        tags: Vec<String>,
    ) -> ArchipelagoClientWrapper {
        let (inner_tx, outer_rx) = channel(1024);
        let (outer_tx, inner_rx) = channel(1024);

        let url_copy = url.as_ref().to_string();
        let slot_copy = slot.as_ref().to_string();
        let password_copy = password.map(|s| s.as_ref().to_string());
        let tags_copy = tags.clone();
        let handle = std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => panic!("Couldn't load Tokio runtime: {e:?}"),
            };

            if let Err(e) = runtime.block_on(run_worker(
                url_copy.as_str(),
                slot_copy.as_str(),
                password_copy.as_deref(),
                items_handling,
                tags_copy,
                &inner_tx,
                inner_rx,
            )) {
                if let Err(send_err) = inner_tx.blocking_send(Err(e)) {
                    warn!(
                        "Failed to send error message from Archipelago client worker thread: \
                        {send_err:?}"
                    );
                }
            }
        });

        ArchipelagoClientWrapper {
            state: ArchipelagoClientState::Connecting,
            rx: outer_rx,
            tx: outer_tx,
            messages: vec![],
            _handle: handle,
        }
    }

    /// The current state of the client.
    pub fn state(&self) -> &ArchipelagoClientState {
        &self.state
    }

    /// Returns all Archipelago messages that have been received since the last
    /// time this message was called.
    ///
    /// These are only refreshed when [update] is called.
    pub fn messages(&mut self) -> Vec<PrintJSON> {
        std::mem::take(&mut self.messages)
    }

    /// Processes any incoming messages from the worker thread and updates the
    /// client's state accordingly.
    pub fn update(&mut self) {
        loop {
            match self.rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // We expect the client to sent a disconnect message or an
                    // error if the connection is closed.
                    self.state = ArchipelagoClientState::Disconnected(
                        "Archipelago client worker thread exited unexpectedly".to_string(),
                    );
                    return;
                }
                Ok(Err(err)) => {
                    warn!("Connection error: {err:?}");
                    self.state = ArchipelagoClientState::Disconnected(err.to_string());
                    return;
                }
                Ok(Ok(ServerMessage::ConnectionRefused(message))) => {
                    self.state = ArchipelagoClientState::Disconnected(message.errors.join(", "));
                    return;
                }
                Ok(Ok(ServerMessage::Connected(connected))) => {
                    self.state = ArchipelagoClientState::Connected(connected);
                }
                Ok(Ok(ServerMessage::Print(Print { text }))) => {
                    self.messages.push(PrintJSON::message(text))
                }
                Ok(Ok(ServerMessage::PrintJSON(message))) => self.messages.push(message),
                _ => (), // Ignore messages we don't care about
            };
        }
    }
}

/// Creates and runs the Archipelago client in a worker thread.
async fn run_worker(
    url: &str,
    slot: &str,
    password: Option<&str>,
    items_handling: ItemsHandlingFlags,
    tags: Vec<String>,
    inner_tx: &Sender<Result<ServerMessage, ArchipelagoError>>,
    mut inner_rx: Receiver<ClientMessage>,
) -> Result<(), ArchipelagoError> {
    let mut client = ArchipelagoClient::new(url).await?;
    let connected = client
        .connect("Dark Souls III", slot, password, items_handling, tags)
        .await?;

    let Ok(_) = inner_tx.send(Ok(ServerMessage::Connected(connected))).await else {
        return Ok(());
    };

    loop {
        tokio::select! {
            Some(message) = inner_rx.recv() => client.send(message).await?,
            result = client.recv() => {
                let Some(message) = result? else { return Ok(()) };
                let Ok(_) = inner_tx.send(Ok(message)).await else { return Ok(()) };
            },
            else => { return Ok(()) },
        }
    }
}
