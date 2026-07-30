/* TODO author, description
 * Speakrs - A communication client / server program
 * Copyright (C) 2026  Miranda Große-Heilmann
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/gpl-3.0>.
 */
use std::time::SystemTime;
use bytemuck::NoUninit;
use bytemuck::Pod;
use bytemuck::Zeroable;
use chrono::DateTime;
use chrono::Utc;
use clap::{Parser, Subcommand};
use rkyv::Archive;
use rkyv::CheckBytes;
use rkyv::Serialize;
use rkyv::de::deserializers::SharedDeserializeMap;
use rkyv::ser::serializers::AlignedSerializer;
use rkyv::ser::serializers::AllocScratch;
use rkyv::ser::serializers::CompositeSerializer;
use rkyv::ser::serializers::FallbackScratch;
use rkyv::ser::serializers::HeapScratch;
use rkyv::ser::serializers::SharedSerializeMap;
use rkyv::util::AlignedVec;
use rkyv::validation::validators::DefaultValidator;
use sled::IVec;
use sled::Tree;
use uuid::Uuid;

use crate::server;
use crate::client;

pub const PROG: &str = "speakrs";
pub const PROG_YEAR: &str = "2026";
pub const PROG_AUTHORS: &str = "Miranda Große-Heilmann, Julie, Viki";

// ======================================
// => Run Arguments
// ======================================

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Arguments {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run in server mode
    Server(server::ServerArguments),
    /// Run in client mode
    Client(client::ClientArguments),
}

// ======================================
// => RPC
// ======================================

#[tarpc::service]
pub trait World {
    /// Returns a greeting for name.
    async fn hello(name: String) -> String;
    // async fn pull_messages(channel_id: ChannelKey, limit: usize) -> anyhow::Result<Vec<MessageData>>;
    // async fn send_message(channel_id: ChannelKey, user_id: UserKey, content: String) -> anyhow::Result<MessageKey>;
}

// ======================================
// => server struct
// ======================================

#[repr(transparent)]
#[derive(Debug, Clone, Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct DBVal<T>(T);

// TODO: I assume N=32 here to make compiler happy, but it might not be the right constant. I made it match the rkyv-32 import, since rkyv itself said it uses 32-bit as default for maximum compat (which may be unrelated)
impl<T> Into<IVec> for DBVal<T> where T: rkyv::Archive + rkyv::Serialize<CompositeSerializer<AlignedSerializer<AlignedVec>, FallbackScratch<HeapScratch<32>, AllocScratch>, SharedSerializeMap>> { // for<'a> Serialize<HighSerializer<AlignedVec, ArenaHandle<'a>, Error>, >
    fn into(self) -> IVec {
        rkyv::to_bytes(&self)
            .unwrap()
            .as_slice()
            .into()
    }
}
impl<T> From<IVec> for DBVal<T> where T: Archive, T::Archived: CheckBytes<DefaultValidator<'_>> + rkyv::Deserialize<T, SharedDeserializeMap> {
    fn from(value: IVec) -> Self {
        rkyv::from_bytes(&value[..])
            .unwrap() // TODO: proper error handling
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize, Zeroable, Pod, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct DBKey(Uuid);

impl AsRef<[u8]> for DBKey {
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}
impl DBKey {
    fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
pub type MessageKey = DBKey;
pub type UserKey = DBKey;
pub type ChannelKey = DBKey;
pub type ChannelMessageKey = (ChannelKey, MessageKey);

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum ChannelType {
    Text,
    Voice,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ChannelData {
    channel_type: ChannelType,
    display_name: String,
    desc: String,
}

impl ChannelData {
    fn text(display_name: String, desc: String) -> Self {
        Self {
            channel_type: ChannelType::Text,
            display_name,
            desc,
        }
    }
    fn voice(display_name: String, desc: String) -> Self {
        Self {
            channel_type: ChannelType::Voice,
            display_name,
            desc,
        }
    }
}
impl Into<IVec> for ChannelData {
    fn into(self) -> IVec {
        serde_json::to_string(&self).unwrap().as_str().into() // unless serializers of struct members fail, this will never fail
    }
}
impl From<IVec> for ChannelData {
    fn from(value: IVec) -> Self {
        // WARNING: if data is corrupted somehow these unwraps might fail
        // TODO: fix
        let value_str = str::from_utf8(&value[..]).unwrap(); // serde_json promises to always output valid utf8
        serde_json::from_str(value_str).unwrap() // on correct write, this should always deserialize
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    // IDEA: join data, bio
    name: String,
}
impl UserData {
    fn new(name: String) -> Self {
        Self { name }
    }
}
impl Into<IVec> for UserData {
    fn into(self) -> IVec {
        serde_json::to_string(&self).unwrap().as_str().into() // unless serializers of struct members fail, this will never fail
    }
}
impl From<IVec> for UserData {
    fn from(value: IVec) -> Self {
        // WARNING: if data is corrupted somehow these unwraps might fail
        // TODO: fix
        let value_str = str::from_utf8(&value[..]).unwrap(); // serde_json promises to always output valid utf8
        serde_json::from_str(value_str).unwrap() // on correct write, this should always deserialize
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MessageData {
    pub timestamp: DateTime<Utc>,
    pub author: UserKey,
    pub content: String,
}
impl MessageData {
    fn new(timestamp: DateTime<Utc>, author: UserKey, content: String) -> Self {
        Self {
            timestamp,
            author,
            content
        }       
    }
}
impl Into<IVec> for MessageData {
    fn into(self) -> IVec {
        serde_json::to_string(&self).unwrap().as_str().into() // unless serializers of struct members fail, this will never fail
    }
}
impl From<IVec> for MessageData {
    fn from(value: IVec) -> Self {
        // WARNING: if data is corrupted somehow these unwraps might fail
        // TODO: fix
        let value_str = str::from_utf8(&value[..]).unwrap(); // serde_json promises to always output valid utf8
        serde_json::from_str(value_str).unwrap() // on correct write, this should always deserialize
    }
}

// ======================================
// => database
// ======================================

enum ServerElements {
    Users,
    Channels,
    Messages,
    MessageChannelMap,
}

impl AsRef<[u8]> for ServerElements {
    fn as_ref(&self) -> &[u8] {
        match self {
            ServerElements::Users => b"users",
            ServerElements::Channels => b"channels",
            ServerElements::Messages => b"messages",
            ServerElements::MessageChannelMap => b"message_channel_map",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerDB {
    db: sled::Db,
}

impl ServerDB {
    pub fn new(database_location: &str) -> Self {
        let db = sled::open(database_location).expect("open");
        Self {
            db
        }
    }

    pub fn insert_message(&self, channel_key: ChannelKey, message_key: MessageKey, message: MessageData) -> anyhow::Result<Option<IVec>> {
        let messages = self.db.open_tree(ServerElements::Messages)?;
        let old = messages.insert(message_key, DBVal(message))?;

        Ok(old)
    }

    pub fn add_message(&self, channel_key: ChannelKey, message: MessageData) -> anyhow::Result<()> {
        self.insert_message(channel_key, MessageKey::new(), message).map(|_| ())
    }

    pub fn get_message(&self, channel_key: ChannelKey, message_key: MessageKey) -> anyhow::Result<Option<MessageData>> {
        let messages = self.db.open_tree(ServerElements::Messages)?;
        //messages.get(message_key).map(|maybe| maybe.map(|value| ))
        Ok(None)
    }

    pub fn insert_channel(&self, channel_key: ChannelKey, channel: ChannelData) -> anyhow::Result<Option<IVec>> {
        let channels = self.db.open_tree(ServerElements::Channels)?;
        Ok(channels.insert(channel_key, channel)?)
    }

    pub fn insert_user(&self, username: UserKey, user: UserData) -> anyhow::Result<Option<IVec>> {
        let users = self.db.open_tree(ServerElements::Users)?;
        Ok(users.insert(username, user)?)
    }
}
