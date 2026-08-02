use super::schema::{ChannelData, ChannelKey, MessageData, MessageKey, ServerData, UserData, UserKey};

#[tarpc::service]
pub trait RpcService {
    /// Returns a greeting for name.
    async fn hello(name: String) -> String;

    async fn get_server_data() -> ServiceResult<ServerData>;

    async fn get_new_channels_since(user: UserKey, since: Option<ChannelKey>) -> ServiceResult<Vec<(ChannelKey, ChannelData)>>;
    async fn create_channel(user: UserKey, data: ChannelData) -> ServiceResult<ChannelKey>;
    async fn get_channel(key: ChannelKey) -> ServiceResult<Option<ChannelData>>;

    async fn create_user(data: UserData) -> ServiceResult<UserKey>;
    async fn get_user(key: UserKey) -> ServiceResult<Option<UserData>>;

    async fn insert_message(channel: ChannelKey, data: MessageData) -> ServiceResult<()>;
    async fn get_message(key: MessageKey) -> ServiceResult<Option<MessageData>>;
}

pub type ServiceResult<T = ()> = Result<T, ServiceError>;

#[derive(thiserror::Error, Debug, serde::Deserialize, serde::Serialize)]
pub enum ServiceError {
    #[error("generic placeholder error, with error message: `{0}`")]
    Failed(String),
    #[error("generic anyhow error, with error message: `{0}`")]
    Anyhow(String),
    #[error("generic sled (db) error, with error message: `{0}`")]
    Sled(String),
}

impl From<anyhow::Error> for ServiceError {
    fn from(value: anyhow::Error) -> Self {
        Self::Anyhow(value.to_string())
    }
}

impl From<sled::Error> for ServiceError {
    fn from(value: sled::Error) -> Self {
        Self::Sled(value.to_string())
    }
}
