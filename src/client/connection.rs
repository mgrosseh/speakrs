use crate::common::{
    auth::SessionToken,
    database::DB,
    rpc::RpcServiceClient,
    schema::{ChannelData, ChannelKey, MessageData, MessageKey, UserData, UserKey},
};
use anyhow::{Context, Result};
use std::{
    fmt::Debug,
    fs::File,
    path::Path,
    sync::{OnceLock, RwLock},
};
use tarpc::tokio_serde::formats::Json;
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info_span};

use super::client_schema::{ClientDump, ClientSession};

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
    db: DB,
}
#[derive(Debug, Clone)]
pub struct ActiveConnection {
    service_client: RpcServiceClient,
    db: DB,
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
    pub fn db(&self) -> &DB {
        match self {
            Self::Empty => {
                panic!("Connection: calling db() on Empty, always call has() first.")
            }
            Self::Unregistered(connection) => &connection.db,
            Self::Active(connection) => &connection.db,
        }
    }
    /// If [`Connection::is_connected()`] is true, this will dump the database of this connection to file specified by `path`.
    /// Panics if [`Connection::is_connected()`] is false.
    pub fn dump_db_to(&self, path: impl AsRef<Path>) -> Result<()> {
        let dump = self.db().dump()?;
        let file = File::create(path)?;
        serde_json::to_writer_pretty(file, &dump)?;
        Ok(())
    }
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

        let db = DB::magic_open_client(data.name, data.uuid)?;
        if let Some(client_session) = db
            .get_client_session()
            .context("Error while reading local database")?
        {
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
    pub async fn register_user(self, user_data: UserData, password: &str) -> Result<Self> {
        match self {
            Self::Empty => Err(anyhow::anyhow!(
                "Cannot register with Empty connection. Connect first!"
            )),
            // WARN: currently we always assume things work, client and server didn't desync their logins etc. // TODO
            Self::Active(_) => Err(anyhow::anyhow!(
                "Cannot register with active connection, as this implies we are already registered."
            )),
            Self::Unregistered(UnregisteredConnection { service_client, db }) => {
                let user_key = service_client
                    .register_user(
                        tarpc::context::current(),
                        user_data.clone(),
                        password.to_owned(),
                    )
                    .instrument(info_span!("Asking server for new user"))
                    .await?
                    .context("Error while talking to server")?;

                let client_data = ClientSession {
                    user_key,
                    token: None,
                };
                db.set_client_session(client_data.clone())
                    .context("Error while writing to local database")?;
                db.users()?.insert((user_key, user_data))?;
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
            Self::Empty => Err(anyhow::anyhow!(
                "Cannot login with Empty connection. Connect first!"
            )),
            Self::Unregistered(_) => Err(anyhow::anyhow!(
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
                db.set_client_session(session)
                    .context("Error while writing to local database")?;

                Ok(Self::Active(ActiveConnection {
                    service_client,
                    db,
                    client_session,
                }))
            }
        }
    }

    fn with_active_guard(
        &self,
        action: &str,
    ) -> Result<(&RpcServiceClient, DB, (UserKey, SessionToken))> {
        match self {
            Self::Empty => Err(anyhow::anyhow!(
                "Cannot {action} with Empty connection. Connect first!"
            )),
            Self::Unregistered(_) => Err(anyhow::anyhow!(
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
                    Err(anyhow::anyhow!(
                        "Cannot {action} with a connection that is not logged in, login first."
                    ))
                }
            }
        }
    }

    /// Send a message to channel with `channel_key` with MessageData `message`, returning the MessageKey of the send message.
    pub async fn send_message(
        &self,
        channel_key: ChannelKey,
        message: MessageData,
    ) -> Result<MessageKey> {
        let (client, db, (_, token)) = self.with_active_guard("'send messages'")?;
        let key = client
            .insert_message(
                tarpc::context::current(),
                token,
                channel_key,
                message.clone(),
            )
            .instrument(info_span!("Creating message in server"))
            .await?
            .context("Error while talking to server")?;

        db.messages()?.set(key, message)?;
        Ok(key)
    }

    /// Add a channel with `channel` data, returning the ChannelKey of the added channel.
    pub async fn add_channel(&self, channel: ChannelData) -> Result<ChannelKey> {
        let (client, db, (_, token)) = self.with_active_guard("'add channel'")?;
        let key = client
            .create_channel(tarpc::context::current(), token, channel.clone())
            .instrument(info_span!("Creating channel in server"))
            .await?
            .context("Error while talking to server")?;

        db.channels()?.insert((key, channel))?;
        Ok(key)
    }

    /// Downloads ALL messages from server, may take significant time.
    /// Returns number of new messages.
    pub async fn download_all_messages(&self) -> Result<usize> {
        let (client, db, (_, token)) = self.with_active_guard("'download all messages'")?;
        let new_messages = client
            .get_new_messages_since(tarpc::context::current(), token, None)
            .instrument(info_span!("Asking server for ALL messages"))
            .await?
            .context("Error while talking to server")?;
        let len = new_messages.len();
        for (key, data) in new_messages {
            db.messages()?.set(key, data)?;
        }
        Ok(len)
    }

    /// Downloads ALL channels from server, may take some time.
    /// Returns number of new channels.
    pub async fn download_all_channels(&self) -> Result<usize> {
        let (client, db, (_, token)) = self.with_active_guard("'download all channels'")?;
        let last_known_channel = db.channels()?.last()?.map(|kv| kv.0);
        let new_channels = client
            .clone()
            .get_new_channels_since(tarpc::context::current(), token, last_known_channel)
            .instrument(info_span!("Asking server for channel list"))
            .await?
            .context("Error while talking to server")?;
        let len = new_channels.len();
        for channel in new_channels {
            db.channels()?.insert((channel.0, channel.1))?;
        }
        Ok(len)
    }

    #[allow(unused)]
    pub async fn message_view_paged(&self, pagination: Pagination<MessageKey>) -> Result<()> {
        todo!() // TODO
    }
}

#[derive(Clone, Copy)]
pub struct Pagination<Cursor> {
    before: Option<Cursor>,
    after: Option<Cursor>,
    limit: PaginationLimit,
}

impl<Cursor> Default for Pagination<Cursor> {
    fn default() -> Self {
        Self {
            before: None,
            after: None,
            limit: Default::default(),
        }
    }
}

impl<Cursor> Pagination<Cursor> {
    fn limit(limit: PaginationLimit) -> Self {
        Self {
            before: None,
            after: None,
            limit,
        }
    }

    fn before(self, before: Cursor) -> Self {
        Self {
            before: Some(before),
            ..self
        }
    }

    fn after(self, after: Cursor) -> Self {
        Self {
            after: Some(after),
            ..self
        }
    }
}

#[derive(Clone, Copy)]
enum PaginationLimit {
    First(usize),
    Last(usize),
}

impl Default for PaginationLimit {
    fn default() -> Self {
        PaginationLimit::First(10)
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

// TODO: utilize
// let last_known_message = db
//     .messages()?
//     .try_filter(|kv| kv.0.prefix() == channel.0)
//     .last()
//     .transpose()?
//     .map(|kv| kv.0);
