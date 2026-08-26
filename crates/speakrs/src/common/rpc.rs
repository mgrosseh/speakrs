use std::convert::Infallible;

use speakrs_storage::{table::TableOpenError, tree::TreeError};

use crate::server::AuthError;

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
    /// Ensure `session` is valid currently.
    async fn validate_session(session: SessionToken) -> ServiceResult<bool>;

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
    #[error("Error during json conversion: `{0}`")]
    SerdeJson(String),
    #[error("error during authentication: `{0}`")]
    Auth(AuthError),
    #[error("{0}")]
    TableOpen(String),
}

impl From<Infallible> for ServiceError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

impl From<sled::Error> for ServiceError {
    fn from(value: sled::Error) -> Self {
        Self::Tree(value.to_string())
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(value: anyhow::Error) -> Self {
        Self::Generic(value.to_string())
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(value: serde_json::Error) -> Self {
        Self::SerdeJson(value.to_string())
    }
}

impl From<TableOpenError> for ServiceError {
    fn from(value: TableOpenError) -> Self {
        Self::TableOpen(value.to_string())
    }
}

impl<E> From<TreeError<E>> for ServiceError
where
    E: Into<ServiceError>,
{
    fn from(value: TreeError<E>) -> Self {
        match value {
            TreeError::Sled(error) => error.into(),
            TreeError::Other(error) => error.into(),
        }
    }
}
