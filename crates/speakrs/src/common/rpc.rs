use std::convert::Infallible;

use speakrs_storage::{
    pagination::{Page, Pagination},
    table::TableOpenError,
    tree::TreeError,
};

use crate::{
    schema::{
        SessionToken,
        channel::{Channel, ChannelId},
        message::{Message, MessageId},
        server_info::ServerInfo,
        user::{User, UserId},
    },
    server::AuthError,
};

use super::audio::AudioPacket;

#[tarpc::service]
pub trait RpcService {
    /// Get server name and uuid
    async fn get_server_data() -> ServiceResult<ServerInfo>;

    /// Register a user with `data` and `password`, returning newly created `UserKey`.
    async fn register_user(name: String, password: String) -> ServiceResult<UserId>;
    async fn authenticate_session(user: UserId, password: String) -> ServiceResult<SessionToken>;
    /// Ensure `session` is valid currently.
    async fn validate_session(session: SessionToken) -> ServiceResult<bool>;

    /// Get all channels created after `since` (or ALL if None).
    async fn get_channels(
        session: SessionToken,
        pagination: Pagination<ChannelId>,
    ) -> ServiceResult<Page<Channel>>;
    /// Create a channel, returning the ChannelKey of the new channel.
    async fn create_channel(session: SessionToken, data: Channel) -> ServiceResult<ChannelId>;
    /// Get channel data corresponding to `key`, if present.
    async fn get_channel(session: SessionToken, key: ChannelId) -> ServiceResult<Channel>;

    /// Get user data corresponding to `key`, if present.
    async fn get_user_info(session: SessionToken, key: UserId) -> ServiceResult<User>;

    /// Get a page of messages within a channel.
    async fn get_channel_messages(
        session: SessionToken,
        since: Option<MessageId>,
    ) -> ServiceResult<Page<Message, MessageId>>;
    /// Create a message in `channel` with `data`, returning the MessageKey of the new message.
    async fn insert_message(
        session: SessionToken,
        channel: ChannelId,
        content: String,
    ) -> ServiceResult<MessageId>;
    /// Get message data corresponding to `key`, if present.
    async fn get_message(session: SessionToken, key: MessageId) -> ServiceResult<Message>;

    /// Send an audio packet to the server.
    async fn send_audio(session: SessionToken, packet: AudioPacket) -> ServiceResult<()>;
}

pub type ServiceResult<T = ()> = Result<T, ServiceError>;

#[derive(thiserror::Error, Debug, serde::Deserialize, serde::Serialize)]
pub enum ServiceError {
    #[error("Service error: {0}")]
    Generic(String),
    #[error("Error from database storage: {0}")]
    Storage(String),
    #[error("Error during json conversion: {0}")]
    SerdeJson(String),
    #[error("error during authentication: {0}")]
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
        Self::Storage(value.to_string())
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(value: anyhow::Error) -> Self {
        Self::Generic(value.to_string())
    }
}

// impl From<serde_json::Error> for ServiceError {
//     fn from(value: serde_json::Error) -> Self {
//         Self::SerdeJson(value.to_string())
//     }
// }

impl From<TableOpenError> for ServiceError {
    fn from(value: TableOpenError) -> Self {
        Self::TableOpen(value.to_string())
    }
}

impl From<TreeError> for ServiceError {
    fn from(value: TreeError) -> Self {
        match value {
            TreeError::Storage(error) => error.into(),
            TreeError::Other(error) => error.into(),
        }
    }
}
