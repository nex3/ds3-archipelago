use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;

use archipelago_rs::client::{ArchipelagoClient, ArchipelagoError};

use crate::archipelago_client_wrapper::OutboundMessage::*;

/// A pull-based wrapper around the Archipelago client connection. All of the
/// actual communication is done on a separate thread. The state only changes
/// when [ArchipelagoClient.update] is called.
pub struct ArchipelagoClientWrapper {
    /// The current state of the client connection.
    state: ArchipelagoClientState,

    /// The receiver for messages coming from the worker thread.
    rx: Receiver<OutboundMessage>,

    /// The transmitter for messages going to the worker thread.
    tx: Sender<InboundMessage>,

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
        items_handling: Option<i32>,
        tags: Vec<String>,
    ) -> ArchipelagoClientWrapper {
        let (inner_tx, outer_rx) = channel();
        let (outer_tx, inner_rx) = channel();

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

            let worker = match runtime.block_on(Worker::new(
                url_copy.as_str(),
                slot_copy.as_str(),
                password_copy.as_deref(),
                items_handling,
                tags_copy,
            )) {
                Ok(worker) => worker,
                Err(e) => {
                    let _ = inner_tx.send(Disconnected(e));
                    return;
                }
            };

            if let Err(_) = inner_tx.send(Connected(worker.connected)) {
                return;
            }

            for _ in inner_rx {
                // ...
            }
        });

        ArchipelagoClientWrapper {
            state: ArchipelagoClientState::Connecting,
            rx: outer_rx,
            tx: outer_tx,
            _handle: handle,
        }
    }

    /// The current state of the client.
    pub fn state(&self) -> &ArchipelagoClientState {
        &self.state
    }

    /// Processes any incoming messages from the worker thread and updates the
    /// client's state accordingly.
    pub fn update(&mut self) {
        loop {
            match self.rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.state = ArchipelagoClientState::Disconnected(
                        "ArchpelagoClientWrapper outgoing socket disconnected".to_string(),
                    );
                    return;
                }
                Ok(Disconnected(err)) => {
                    self.state = ArchipelagoClientState::Disconnected(err.to_string());
                    return;
                }
                Ok(Connected(connected)) => {
                    self.state = ArchipelagoClientState::Connected(connected);
                }
            };
        }
    }
}

/// The data in the worker thread that handles the underlying client.
struct Worker {
    /// The underlying Archipelago client.
    client: ArchipelagoClient,

    /// The data returned from the initial connection.
    connected: archipelago_rs::protocol::Connected,
}

impl Worker {
    /// Constructs the worker and asynchronously begins the initial connection.
    async fn new(
        url: &str,
        slot: &str,
        password: Option<&str>,
        items_handling: Option<i32>,
        tags: Vec<String>,
    ) -> Result<Worker, ArchipelagoError> {
        let mut client = ArchipelagoClient::new(url).await?;
        let connected = client
            .connect("Dark Souls III", slot, password, items_handling, tags)
            .await?;
        Ok(Worker { client, connected })
    }
}

/// Messages sent from the [ArchipelagoClientWrapper] to the worker thread.
enum InboundMessage {}

/// Messages sent from the worker thread to the [ArchipelagoClientWrapper].
enum OutboundMessage {
    /// A message indicating that a connection has been successfully
    /// established.
    Connected(archipelago_rs::protocol::Connected),

    /// A message indicating that there is no connection, either because
    /// establishing it failed in the first place or because it was closed after
    /// being successfully established.
    Disconnected(ArchipelagoError),
}
