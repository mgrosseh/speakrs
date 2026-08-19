use std::fmt::Display;

use crate::{
    common::{table::TableOpenError, tree::TreeError},
    server::AuthError,
};

use super::{
    audio::AudioPacket,
    auth::SessionToken,
    schema::{ChannelData, ChannelKey, MessageData, MessageKey, ServerInfoData, UserData, UserKey},
};

#[tarpc::service]
pub trait RpcService {
    /// Get server name and uuid
    async fn get_server_data() -> ServiceResult<ServerInfoData>;

    /// Register a user with `data` and `password`, returning newly created `UserKey`.
    async fn register_user(data: UserData, password: String) -> ServiceResult<UserKey>;
    async fn authenticate_session(user: UserKey, password: String) -> ServiceResult<SessionToken>;

    /// Get all channels created after `since` (or ALL if None).
    async fn get_new_channels_since(
        session: SessionToken,
        since: Option<ChannelKey>,
    ) -> ServiceResult<Vec<(ChannelKey, ChannelData)>>;
    /// Create a channel, returning the ChannelKey of the new channel.
    async fn create_channel(session: SessionToken, data: ChannelData) -> ServiceResult<ChannelKey>;
    /// Get channel data corresponding to `key`, if present.
    async fn get_channel(
        session: SessionToken,
        key: ChannelKey,
    ) -> ServiceResult<Option<ChannelData>>;

    /// Get user data corresponding to `key`, if present.
    async fn get_user_info(session: SessionToken, key: UserKey) -> ServiceResult<Option<UserData>>;

    /// Get all messages created after `since` (or ALL if None).
    async fn get_new_messages_since(
        session: SessionToken,
        since: Option<MessageKey>,
    ) -> ServiceResult<Vec<(MessageKey, MessageData)>>; // TODO: channel parameter to restrict messages to a channel
    /// Create a message in `channel` with `data`, returning the MessageKey of the new message.
    async fn insert_message(
        session: SessionToken,
        channel: ChannelKey,
        data: MessageData,
    ) -> ServiceResult<MessageKey>;
    /// Get message data corresponding to `key`, if present.
    async fn get_message(
        session: SessionToken,
        key: MessageKey,
    ) -> ServiceResult<Option<MessageData>>;

    /// Send an audio packet to the server.
    async fn send_audio(session: SessionToken, packet: AudioPacket) -> ServiceResult<()>;
}

pub type ServiceResult<T = ()> = Result<T, ServiceError>;

#[derive(thiserror::Error, Debug, serde::Deserialize, serde::Serialize)]
pub enum ServiceError {
    #[error("Generic service error: `{0}`")]
    Generic(String),
    #[error("Error in database tree: `{0}`")]
    Tree(String),
    #[error("error during authentication: `{0}`")]
    Auth(AuthError),
    #[error("{0}")]
    TableOpen(String),
}

impl<Enc, Dec> From<TreeError<Enc, Dec>> for ServiceError
where
    TreeError<Enc, Dec>: Display,
{
    fn from(value: TreeError<Enc, Dec>) -> Self {
        Self::Tree(value.to_string())
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(value: anyhow::Error) -> Self {
        Self::Generic(value.to_string())
    }
}

impl From<TableOpenError> for ServiceError {
    fn from(value: TableOpenError) -> Self {
        Self::TableOpen(value.to_string())
    }
}
