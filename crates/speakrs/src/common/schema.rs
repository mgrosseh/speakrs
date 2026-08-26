// ======================================
// => specific value / key implementation
// ======================================

use chrono::{DateTime, Utc};
use uuid::Uuid;

use speakrs_storage::{
    key::UuidKey,
    table::{OneToMany, SerdeSingleton, SerdeTree, TableDecl},
};

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
    pub channel: ChannelKey,
}
impl MessageData {
    /// Create MessageData with timestamp now
    #[allow(unused)] // TODO
    pub fn now(author: UserKey, channel: ChannelKey, content: String) -> Self {
        Self::new(Utc::now(), channel, author, content)
    }
    #[allow(unused)] // TODO
    pub fn new(
        timestamp: DateTime<Utc>,
        channel: ChannelKey,
        author: UserKey,
        content: String,
    ) -> Self {
        Self {
            timestamp,
            author,
            content,
            channel,
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ServerInfoData {
    // TODO: better name
    /// Host system unique name
    pub name: String,
    pub uuid: Uuid,
}

pub type UserKey = UuidKey<UserData>;
pub type ChannelKey = UuidKey<ChannelData>;
pub type MessageKey = UuidKey<MessageData>;

pub type UsersTable = SerdeTree<UserData>;
pub const USERS_TABLE: TableDecl<UsersTable> = UsersTable::decl("users");

pub type ChannelsTable = SerdeTree<ChannelData>;
pub const CHANNELS_TABLE: TableDecl<ChannelsTable> = ChannelsTable::decl("channels");

pub type MessagesTable = SerdeTree<MessageData, MessageKey>;
pub const MESSAGES_TABLE: TableDecl<MessagesTable> = MessagesTable::decl("messages");

pub type MessagesInChannelTable = OneToMany<ChannelKey, MessageKey>;
pub const MESSAGES_IN_CHANNEL_TABLE: TableDecl<MessagesInChannelTable> =
    MessagesInChannelTable::decl("messages_in_channel");

pub type ServerDataTable = SerdeSingleton<ServerInfoData>;
pub const SERVER_DATA_TABLE: TableDecl<ServerDataTable> = ServerDataTable::decl("server_data");
