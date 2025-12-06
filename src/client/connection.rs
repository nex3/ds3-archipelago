use std::fmt;
use std::thread;

use archipelago_rs::client::*;
use archipelago_rs::protocol::*;
use log::*;
use tokio::sync::mpsc::{Receiver, Sender, channel, error::TryRecvError};

use crate::client::ConnectedClient;
use crate::slot_data::SlotData;

/// A class that manages the Archipelago client connection in a pull-based
/// manner. All of the actual communication is done on a separate thread. The
/// state only changes when [ClientConnection.update] is called.
pub struct ClientConnection {
    /// The current state of the client connection.
    state: ClientConnectionState,

    /// The receiver for messages coming from the worker thread.
    rx: Receiver<Result<ServerMessage<SlotData>, ArchipelagoError>>,

    /// The room info for the current connection. This is only set during a
    /// subset of [ClientConnectionState::Connecting], after which point
    /// ownership is passed to [ConnectedClient].
    room_info: Option<RoomInfo>,

    /// The Archipelago data package. This is only set during a subset of
    /// [ClientConnectionState::Connecting], after which point ownership is
    /// passed to [ConnectedClient].
    data_package: Option<DataPackageObject>,

    /// The transmitter for messages going to the worker thread.
    ///
    /// This is only retained until the [ConnectedClient] has been created, at
    /// which point ownership is transferred there.
    tx: Option<Sender<ClientMessage>>,

    /// The handle of the worker thread, used to ensure that it's dropped along
    /// with the client wrapper.
    _handle: thread::JoinHandle<()>,
}

/// The state of the underlying Archipelago client as of the last received message.
#[allow(clippy::large_enum_variant)]
pub enum ClientConnectionState {
    /// The client is in the process of establishing a connection and
    /// downloading the initial data.
    Connecting,

    /// The client has successfully connected. This includes data that's always
    /// available with a successful connection.
    Connected(ConnectedClient),

    /// The client is not connected, either because the initial connection
    /// failed or because an established connection was later closed. This
    /// contains a description of the reason for the closure.
    Disconnected(String),
}

impl fmt::Debug for ClientConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "{}",
            match self {
                ClientConnectionState::Connecting => "Connecting",
                ClientConnectionState::Connected(_) => "Connected",
                ClientConnectionState::Disconnected(_) => "Disconnected",
            }
        )
    }
}

impl ClientConnection {
    /// Creates a new client and begins attempting to establish a connection
    /// with the given credentials.
    pub fn new(
        url: impl AsRef<str>,
        slot: impl AsRef<str>,
        password: Option<impl AsRef<str>>,
        items_handling: ItemsHandlingFlags,
        tags: Vec<String>,
    ) -> ClientConnection {
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
            )) && let Err(send_err) = inner_tx.blocking_send(Err(e))
            {
                warn!(
                    "Failed to send error message from Archipelago client worker thread: \
                        {send_err:?}"
                );
            }
        });

        ClientConnection {
            state: ClientConnectionState::Connecting,
            rx: outer_rx,
            room_info: None,
            data_package: None,
            tx: Some(outer_tx),
            _handle: handle,
        }
    }

    /// The current state of the client.
    pub fn state(&self) -> &ClientConnectionState {
        &self.state
    }

    /// The current state of the client.
    ///
    /// This returns a mutable reference so that the caller can call mutable
    /// methods on the [ConnectedClient], but the caller *must not* change the
    /// state itself.
    pub fn state_mut(&mut self) -> &mut ClientConnectionState {
        &mut self.state
    }

    /// Processes any incoming messages from the worker thread and updates the
    /// client's state accordingly.
    ///
    /// Has no effect if this is already disconnected.
    pub fn update(&mut self) {
        if let ClientConnectionState::Disconnected(_) = self.state {
            return;
        }

        loop {
            match self.rx.try_recv() {
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // We expect the client to sent a disconnect message or an
                    // error if the connection is closed.
                    self.state = ClientConnectionState::Disconnected(
                        "Archipelago client worker thread exited unexpectedly".to_string(),
                    );
                    return;
                }
                Ok(Err(err)) => {
                    warn!("Connection error: {err:?}");
                    self.state = ClientConnectionState::Disconnected(err.to_string());
                    return;
                }
                Ok(Ok(ServerMessage::ConnectionRefused(message))) => {
                    self.state = ClientConnectionState::Disconnected(message.errors.join(", "));
                    return;
                }
                Ok(Ok(ServerMessage::RoomInfo(room_info))) => self.room_info = Some(room_info),
                Ok(Ok(ServerMessage::DataPackage(DataPackage { data }))) => {
                    self.data_package = Some(data);
                }
                Ok(Ok(ServerMessage::Connected(connected))) => {
                    let tx = self.tx.take().unwrap();
                    let game_data = self
                        .data_package
                        .take()
                        .expect("Expected game data to be received before Connected");
                    let room_info = self
                        .room_info
                        .take()
                        .expect("Expected room info to be received before Connected");
                    self.state = ClientConnectionState::Connected(ConnectedClient::new(
                        connected, room_info, game_data, tx,
                    ));
                }
                Ok(Ok(message)) => match &mut self.state {
                    ClientConnectionState::Connected(client) => client.update(message),
                    _ => {
                        self.state = ClientConnectionState::Disconnected(
                            format!("Unexpected message in {:?}: {message:?}", self.state)
                                .to_string(),
                        );
                        return;
                    }
                },
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
    inner_tx: &Sender<Result<ServerMessage<SlotData>, ArchipelagoError>>,
    mut inner_rx: Receiver<ClientMessage>,
) -> Result<(), ArchipelagoError> {
    // Don't use with_data_package because we want to take and transfer
    // ownership of the data package rather than storing it in the wrapped
    // client.
    let mut client = ArchipelagoClient::<SlotData>::new(url).await?;

    let Ok(_) = inner_tx
        .send(Ok(ServerMessage::RoomInfo(client.room_info().clone())))
        .await
    else {
        return Ok(());
    };

    client
        .send(ClientMessage::GetDataPackage(GetDataPackage {
            games: None,
        }))
        .await?;
    let response = client.recv().await?;
    let data_package = match response {
        Some(ServerMessage::DataPackage(pkg)) => pkg,
        Some(received) => {
            return Err(ArchipelagoError::IllegalResponse {
                expected: "Data package",
                received: received.type_name(),
            });
        }
        None => return Err(ArchipelagoError::ConnectionClosed),
    };

    let Ok(_) = inner_tx
        .send(Ok(ServerMessage::DataPackage(data_package)))
        .await
    else {
        return Ok(());
    };

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
