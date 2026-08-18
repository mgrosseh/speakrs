// ======================================
// => specific value / key implementation
// ======================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::common::{
    key::{PrefixedKey, PrefixedKeygen, UuidKey},
    table::{SerdeSingleton, SerdeTree, TableDecl},
};

use super::auth::SessionToken;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum ChannelType {
    Text,
    Voice,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChannelData {
    channel_type: ChannelType,
    display_name: String,
    desc: String,
}

impl ChannelData {
    /// Create a text channel
    pub fn text(display_name: String, desc: String) -> Self {
        Self {
            channel_type: ChannelType::Text,
            display_name,
            desc,
        }
    }
    /// Create a voice channel
    #[allow(unused)] // TODO
    pub fn voice(display_name: String, desc: String) -> Self {
        Self {
            channel_type: ChannelType::Voice,
            display_name,
            desc,
        }
    }
    pub fn get_name(&self) -> &str {
        self.display_name.as_str()
    }
    pub fn get_description(&self) -> &str {
        self.desc.as_str()
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    // IDEA: join date, bio
    pub name: String,
}
impl UserData {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct MessageData {
    pub timestamp: DateTime<Utc>,
    pub author: UserKey,
    pub content: String,
}
impl MessageData {
    /// Create MessageData with timestamp now
    #[allow(unused)] // TODO
    pub fn now(author: UserKey, content: String) -> Self {
        Self::new(Utc::now(), author, content)
    }
    #[allow(unused)] // TODO
    pub fn new(timestamp: DateTime<Utc>, author: UserKey, content: String) -> Self {
        Self {
            timestamp,
            author,
            content,
        }
    }
}

// TODO: move into client only code
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ClientData {
    pub user_key: UserKey,
    pub session: Option<SessionToken>,  // TODO: handle cases when token expires and session is none
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ServerInfoData { // TODO: better name
    /// Host system unique name
    pub name: String,
    pub uuid: Uuid,
}

pub type UserKey = UuidKey<UserData>;
pub type ChannelKey = UuidKey<ChannelData>;
pub type MessageKey = PrefixedKey<ChannelKey, MessageData>;

pub type UsersTable = SerdeTree<UserData>;
pub const USERS_TABLE: TableDecl<UsersTable> = UsersTable::decl("users");

pub type ChannelsTable = SerdeTree<ChannelData>;
pub const CHANNELS_TABLE: TableDecl<ChannelsTable> = ChannelsTable::decl("channels");

pub type MessagesTable = SerdeTree<MessageData, MessageKey, PrefixedKeygen<ChannelKey>>;
pub const MESSAGES_TABLE: TableDecl<MessagesTable> = MessagesTable::decl("messages");

pub type ServerDataTable = SerdeSingleton<ServerInfoData>;
pub const SERVER_DATA_TABLE: TableDecl<ServerDataTable> = ServerDataTable::decl("server_data");

pub type ClientDataTable = SerdeSingleton<ClientData>;
pub const CLIENT_DATA_TABLE: TableDecl<ClientDataTable> = ClientDataTable::decl("client_data");
