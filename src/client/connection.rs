use crate::common::{
    auth::SessionToken,
    database::DB,
    rpc::RpcServiceClient,
    schema::{ChannelData, ChannelKey, MessageData, MessageKey, UserData, UserKey},
};
use anyhow::{Context, Result};
use std::{
    fmt::Debug,
    sync::{OnceLock, RwLock},
};
use tarpc::tokio_serde::formats::Json;
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info_span};

use super::client_schema::ClientSession;

// TODO: in the future having only one connection is bad, so current_connection() and clone_current_connection() will probably need overhauls
#[derive(Debug, Clone)]
pub enum Connection {
    Empty,
    Unregistered(UnregisteredConnection),
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
    pub fn is_registered(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Unregistered(_) => false,
            Self::Active(_) => true,
        }
    }
    pub fn is_connected(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Unregistered(_) => true,
            Self::Active(_) => true,
        }
    }
    pub fn db(&self) -> &DB {
        match self {
            Self::Empty => {
                panic!("Connection: calling db() on Empty, always call has() first.")
            }
            Self::Unregistered(connection) => &connection.db,
            Self::Active(connection) => &connection.db,
        }
    }
    pub fn session(&self) -> &ClientSession {
        match self {
            Self::Empty => {
                panic!("Connection: calling session() on Empty, always call is_registered() first.")
            }
            Self::Unregistered(_) => {
                panic!(
                    "Connection: calling session() on Unregistered, always call is_registered() first."
                )
            }
            Self::Active(connection) => &connection.client_session,
        }
    }

    // TODO: no longer panic here:

    pub async fn connect_to_ip(addr: impl ToSocketAddrs) -> Result<Connection> {
        let service_client = Connection::create_service_client(addr).await?;
        let data = service_client
            .get_server_data(tarpc::context::current())
            .instrument(info_span!("Asking server for server data"))
            .await?
            .context("Error while talking to server")?;

        let db = DB::magic_open_client(data.name, data.uuid)?;
        if let Some(client_data) = db
            .get_client_data()
            .context("Error while reading local database")?
        {
            Ok(Self::Active(ActiveConnection {
                service_client,
                db,
                client_session: client_data,
            }))
        } else {
            Ok(Self::Unregistered(UnregisteredConnection {
                service_client,
                db,
            }))
        }
    }

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
                db.set_client_data(client_data.clone())
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

    pub async fn verify_login(&self) -> Result<bool> {
        Ok(match self {
            Self::Empty => false,
            Self::Unregistered(_) => false,
            Self::Active(ActiveConnection {
                service_client,
                client_session: client_data,
                ..
            }) => {
                if let Some(session) = client_data.token {
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
                db.set_client_data(session)
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
        let (client, db, (_, token)) = self.with_active_guard("'download all messages'")?;
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
    pub async fn message_view_paged(&self, _channel: ChannelKey, page: usize) -> Result<()> {
        todo!() // TODO
    }
}

pub fn current_connection() -> &'static RwLock<Connection> {
    static CONNECTION: OnceLock<RwLock<Connection>> = OnceLock::new();
    CONNECTION.get_or_init(|| RwLock::new(Connection::Empty))
}
pub fn clone_current_connection() -> Connection {
    current_connection().read().unwrap().clone()
}
