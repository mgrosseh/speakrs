use crate::{
    common::{database::open_client_db, rpc::RpcServiceClient},
    schema::{
        ClientDataStore, SessionToken,
        channel::{Channel, ChannelId},
        client::client_session::ClientSession,
        message::MessageId,
        user::{User, UserId},
    },
};
use eyre::{Context, Result};
use speakrs_storage::pagination::{Edge, Pagination};
use std::{
    fmt::Debug,
    sync::{OnceLock, RwLock},
    usize,
};
use tarpc::tokio_serde::formats::Json;
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info_span};

#[derive(Debug, Clone)]
pub enum Connection {
    /// An empty connection, i.e. not connected to any server.
    Empty,
    /// A connection to a server, that is unregistered.
    /// This means no user data has been associated with this server (the server needs to be registered with).
    Unregistered(UnregisteredConnection),
    /// An active connection to a server.
    /// Usually this means full communication with the server is possible, though reauthentication may be
    /// necessary, if not yet logged in or the session expired.
    Active(ActiveConnection),
}
#[derive(Debug, Clone)]
pub struct UnregisteredConnection {
    service_client: RpcServiceClient,
    db: ClientDataStore,
}
#[derive(Debug, Clone)]
pub struct ActiveConnection {
    service_client: RpcServiceClient,
    db: ClientDataStore,
    client_session: ClientSession,
}
impl Connection {
    async fn create_service_client(addr: impl ToSocketAddrs) -> Result<RpcServiceClient> {
        let mut transport = tarpc::serde_transport::tcp::connect(addr, Json::default);
        transport.config_mut().max_frame_length(usize::MAX);
        Ok(RpcServiceClient::new(tarpc::client::Config::default(), transport.await?).spawn())
    }
    /// Return true if `self` is active (registered), false if [`Connection::Unregistered`] or [`Connection::Empty`].
    pub fn is_registered(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Unregistered(_) => false,
            Self::Active(_) => true,
        }
    }
    /// Return true if `self` is connected, false if [`Connection::Empty`].
    pub fn is_connected(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Unregistered(_) => true,
            Self::Active(_) => true,
        }
    }
    /// If `is_connected()` is true, this will return the database of this connection.
    /// Panics if `is_connected()` is false.
    pub fn db(&self) -> &ClientDataStore {
        match self {
            Self::Empty => {
                panic!("Connection: calling db() on Empty, always call has() first.")
            }
            Self::Unregistered(connection) => &connection.db,
            Self::Active(connection) => &connection.db,
        }
    }
    // /// If [`Connection::is_connected()`] is true, this will dump the database of this connection to file specified by `path`.
    // /// Panics if [`Connection::is_connected()`] is false.
    // pub fn dump_db_to(&self, path: impl AsRef<Path>) -> Result<()> {
    //     let dump = self.db().dump()?;
    //     let file = File::create(path)?;
    //     serde_json::to_writer_pretty(file, &dump)?;
    //     Ok(())
    // }
    /// If `is_registered()` is true, this will return the client session of this connection.
    pub fn session(&self) -> &ClientSession {
        match self {
            Self::Empty => {
                panic!("Connection: calling session() on Empty, always call is_registered() first.")
            }
            Self::Unregistered(_) => {
                panic!("Connection: calling session() on Unregistered, always call .() first.")
            }
            Self::Active(connection) => &connection.client_session,
        }
    }

    /// Using an `addr` create a connection to the server.
    /// If we find a session in the database, returns an [`Connection::Active`] connection,
    /// otherwise returns [`Connection::Unregistered`]. When [`Connection::Active`], usually
    /// communication is possible, though it could be the session has expired, in which case
    /// a [`Connection::login`] is required. When [`Connection::Unregistered`], a
    /// [`Connection::register_user`] is required.
    pub async fn connect_to_ip(addr: impl ToSocketAddrs) -> Result<Connection> {
        let service_client = Connection::create_service_client(addr).await?;
        let data = service_client
            .get_server_data(tarpc::context::current())
            .instrument(info_span!("Asking server for server data"))
            .await?
            .context("Error while talking to server")?;

        let db = open_client_db(data)?;
        if let Some(client_session) = db.current_session()?.focus {
            Ok(Self::Active(ActiveConnection {
                service_client,
                db,
                client_session,
            }))
        } else {
            Ok(Self::Unregistered(UnregisteredConnection {
                service_client,
                db,
            }))
        }
    }

    /// Registers the user indicated by `user_data` with `password`.
    /// This is only valid on an [`Connection::Unregistered`], otherwise will result in Err.
    /// Exchanges password with the server and on successful exchange, a session is created.
    /// Note that after this a [`Connection::login`] is required.
    pub async fn register_user(self, username: &str, password: &str) -> Result<Self> {
        match self {
            Self::Empty => Err(eyre::anyhow!(
                "Cannot register with Empty connection. Connect first!"
            )),
            // WARN: currently we always assume things work, client and server didn't desync their logins etc. // TODO
            Self::Active(_) => Err(eyre::anyhow!(
                "Cannot register with active connection, as this implies we are already registered."
            )),
            Self::Unregistered(UnregisteredConnection { service_client, db }) => {
                let user_key = service_client
                    .register_user(
                        tarpc::context::current(),
                        username.to_owned(),
                        password.to_owned(),
                    )
                    .instrument(info_span!("Asking server for new user"))
                    .await?
                    .context("Error while talking to server")?;

                let client_data = ClientSession {
                    user_key,
                    token: None,
                };
                db.sync_users([Edge {
                    node: User::new(username.to_owned()),
                    cursor: user_key,
                }])?;
                db.set_current_session(client_data.clone())
                    .context("Error while writing to local database")?;
                Ok(Self::Active(ActiveConnection {
                    service_client,
                    db,
                    client_session: client_data,
                }))
            }
        }
    }

    /// Verifies that we are still logged in.
    /// Communicates with the server to verify the current session is valid,
    /// otherwise returns false.
    pub async fn verify_login(&self) -> Result<bool> {
        Ok(match self {
            Self::Empty => false,
            Self::Unregistered(_) => false,
            Self::Active(ActiveConnection {
                service_client,
                client_session,
                ..
            }) => {
                if let Some(session) = client_session.token {
                    service_client
                        .validate_session(tarpc::context::current(), session)
                        .instrument(info_span!("Validating stored session"))
                        .await?
                        .context("Error while talking to server")?
                } else {
                    false
                }
            }
        })
    }

    /// Logs into the server.
    /// Using `password` authenticates user stored in [`ClientSession`] database.
    /// Returns a new [`Connection::Active`] that is logged into the server.
    pub async fn login(self, password: String) -> Result<Self> {
        if self.verify_login().await? {
            return Ok(self);
        }
        match self {
            Self::Empty => Err(eyre::anyhow!(
                "Cannot login with Empty connection. Connect first!"
            )),
            Self::Unregistered(_) => Err(eyre::anyhow!(
                "Cannot login with an unregistered connection, register first."
            )),
            Self::Active(ActiveConnection {
                service_client,
                db,
                client_session,
            }) => {
                let user_key = client_session.user_key;
                let token = service_client
                    .authenticate_session(tarpc::context::current(), user_key, password.clone())
                    .instrument(info_span!("Authenticating with server using credentials"))
                    .await?
                    .context("Error while talking to server")?;

                let session = ClientSession {
                    user_key: user_key,
                    token: Some(token),
                };
                db.set_current_session(session.clone())
                    .context("Error while writing to local database")?;

                Ok(Self::Active(ActiveConnection {
                    service_client,
                    db,
                    client_session: session,
                }))
            }
        }
    }

    fn with_active_guard(
        &self,
        action: &str,
    ) -> Result<(&RpcServiceClient, ClientDataStore, (UserId, SessionToken))> {
        match self {
            Self::Empty => Err(eyre::anyhow!(
                "Cannot {action} with Empty connection. Connect first!"
            )),
            Self::Unregistered(_) => Err(eyre::anyhow!(
                "Cannot {action} with an unregistered connection, register first."
            )),
            Self::Active(a) => {
                if let ClientSession {
                    user_key,
                    token: Some(token),
                } = a.client_session
                {
                    Ok((&a.service_client, a.db.clone(), (user_key, token)))
                } else {
                    Err(eyre::anyhow!(
                        "Cannot {action} with a connection that is not logged in, login first."
                    ))
                }
            }
        }
    }

    /// Send a message to channel with `channel_key` with MessageData `message`, returning the MessageKey of the send message.
    pub async fn send_message(&self, channel_key: ChannelId, content: String) -> Result<MessageId> {
        let (client, db, (_, token)) = self.with_active_guard("'send messages'")?;
        let ctx = tarpc::context::current();
        let key = client
            .insert_message(ctx, token, channel_key, content)
            .instrument(info_span!("Creating message in server"))
            .await??;

        let message = client.get_message(ctx, token, key).await??;
        db.sync_message(Edge {
            node: message,
            cursor: key,
        })?;
        Ok(key)
    }

    /// Add a channel with `channel` data, returning the ChannelKey of the added channel.
    pub async fn add_channel(&self, channel: Channel) -> Result<ChannelId> {
        let (client, db, (_, token)) = self.with_active_guard("'add channel'")?;
        let ctx = tarpc::context::current();
        let key = client
            .create_channel(ctx, token, channel.clone())
            .instrument(info_span!("Creating channel in server"))
            .await?
            .context("Error while talking to server")?;

        let channel = client.get_channel(ctx, token, key).await??;

        db.sync_channels([Edge {
            node: channel,
            cursor: key,
        }])?;
        Ok(key)
    }

    /// Downloads ALL messages from server, may take significant time.
    /// Returns number of new messages.
    pub async fn download_all_messages(&self) -> Result<usize> {
        let (client, db, (_, token)) = self.with_active_guard("'download all messages'")?;
        let last_known_message = db.messages(Pagination::last(1))?.focus.into_iter().next();

        let new_messages = client
            .get_channel_messages(
                tarpc::context::current(),
                token,
                last_known_message.map(|m| m.cursor),
            )
            .instrument(info_span!("Asking server for ALL messages"))
            .await?
            .context("Error while talking to server")?;
        let len = new_messages.edges.len();
        for edge in new_messages {
            db.sync_message(edge)?;
        }
        Ok(len)
    }

    /// Downloads ALL channels from server, may take some time.
    /// Returns number of new channels.
    pub async fn download_all_channels(&self) -> Result<usize> {
        let (client, db, (_, token)) = self.with_active_guard("'download all channels'")?;
        let last_known_channel = db.channels(Pagination::last(1))?.focus.into_iter().next();
        let new_channels = client
            .get_channels(
                tarpc::context::current(),
                token,
                Pagination::first(usize::MAX).opt_after(last_known_channel.map(|c| c.cursor)),
            )
            .instrument(info_span!("Asking server for channel list"))
            .await?
            .context("Error while talking to server")?;
        let len = new_channels.edges.len();
        db.sync_channels(new_channels)?;
        Ok(len)
    }

    #[allow(unused)]
    pub async fn channel_messages(
        &self,
        channel_key: ChannelId,
        pagination: Pagination<MessageId>,
    ) -> Result<()> {
        todo!() // TODOa
    }
}

// TODO: in the future having only one connection is bad, so current_connection() and clone_current_connection() will probably need overhauls

pub fn current_connection() -> &'static RwLock<Connection> {
    static CONNECTION: OnceLock<RwLock<Connection>> = OnceLock::new();
    CONNECTION.get_or_init(|| RwLock::new(Connection::Empty))
}
pub fn clone_current_connection() -> Connection {
    current_connection().read().unwrap().clone()
}
