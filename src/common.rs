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
use bytemuck::Pod;
use bytemuck::Zeroable;
use chrono::DateTime;
use chrono::Utc;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use serde::Serialize;
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
// => db / key / value abstract
// ======================================
#[derive(Debug, Clone)]
pub struct DBValue<T>(T);
impl<'a, T> Deserialize<'a> for DBValue<T> where T: Deserialize<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a> {
        Ok(Self(T::deserialize(deserializer)?))
    }
}
impl<T> Serialize for DBValue<T> where T: Serialize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        Ok(self.0.serialize(serializer)?)
    }
}

impl<T> Into<IVec> for DBValue<T> where T: serde::Serialize {
    fn into(self) -> IVec {
        serde_json::to_string(&self).unwrap().as_str().into() // unless serializers of struct members fail, this will never fail
    }
}
impl<T> DBValue<T> {
    fn from(value: IVec) -> anyhow::Result<Self> where for<'a> T: Deserialize<'a> {
        let value_str = str::from_utf8(&value[..])?; // serde_json promises to always output valid utf8
        Ok(serde_json::from_str(value_str)?) // on correct write, this should always deserialize
    }
}

trait DBKeyable {
    fn from_ref(data: &[u8]) -> Option<Self> where Self: Sized;
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct UuidKey(Uuid);
impl AsRef<[u8]> for UuidKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}
impl Default for UuidKey {
    fn default() -> Self {
        Self(Uuid::now_v7())
    }
}
impl DBKeyable for UuidKey {
    fn from_ref(data: &[u8]) -> Option<Self> {
        if data.len() != 16 {
            return None;
        }
        Some(Self(Uuid::from_slice(data).expect("Should never be null since guard condition is same as error condition")))
    }
}
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize, Zeroable, Pod)]
pub struct UuidKey2(Uuid, Uuid);
impl AsRef<[u8]> for UuidKey2 {
    fn as_ref(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
}
impl Default for UuidKey2 {
    fn default() -> Self {
        Self(Uuid::now_v7(), Uuid::now_v7())
    }
}
impl DBKeyable for UuidKey2 {
    fn from_ref(data: &[u8]) -> Option<Self> {
        if data.len() != 32 {
            return None;
        }
        let uuid1 = Uuid::from_slice(&data[..16]).expect("Should never be err because of guard");
        let uuid2 = Uuid::from_slice(&data[16..]).expect("Should never be err because of guard");
        Some(Self(uuid1, uuid2))
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
struct DBKey<T>(T);
impl<T> AsRef<[u8]> for DBKey<T> where T: AsRef<[u8]> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl<T> Default for DBKey<T> where T: Default {
    fn default() -> Self {
        Self(T::default())
    }
}
impl<T> DBKey<T> {
    fn from(value: IVec) -> Option<Self> where T: DBKeyable  {
        let x: &[u8] = &value[..];
        T::from_ref(x).map(|v| Self(v))
    }
}

pub struct DBTree<TKey, TValue>(DBTreeInner<TKey, TValue>);
impl<TKey, TValue> DBTree<TKey, TValue> {
    fn with(tree: Tree) -> Self {
        Self(DBTreeInner::Tree(tree))
    }

    fn this(&self) -> &Tree {
        match &self.0 {
            DBTreeInner::Tree(tree) => tree,
            DBTreeInner::_UnusedKey(_) => panic!("Never construct with DBTreeInner::UnusedKey"),
            DBTreeInner::_UnusedValue(_) => panic!("Never construct with DBTreeInner::UnusedValue"),
        }
    }

    pub fn insert(&self, key: TKey, value: TValue) -> anyhow::Result<()> where TKey: AsRef<[u8]>, TValue: serde::Serialize {
       Ok(self.this().insert(DBKey(key), DBValue(value)).map(|_| ())?)
    }
    pub fn insert_replace(&self, key: TKey, value: TValue) -> anyhow::Result<Option<TValue>> where TKey: AsRef<[u8]>, TValue: serde::Serialize + for<'a> Deserialize<'a> {
        let old = self.this().insert(DBKey(key), DBValue(value))?;
        match old {
            Some(v) => {
                Ok(Some(DBValue::from(v)?.0))
            }
            None => {
                Ok(None)
            }
        }
    }
    pub fn get(&self, key: TKey) -> anyhow::Result<Option<TValue>> where TKey: AsRef<[u8]>, for<'a> TValue: Deserialize<'a> {
        Ok(match self.this().get(DBKey(key))? {
            Some(v) => Some(DBValue::from(v)?.0),
            None => None
        })
    }

    fn map_ivec_pair(pair: Result<Option<(IVec, IVec)>, sled::Error>) -> anyhow::Result<Option<(TKey, TValue)>> where TKey: DBKeyable, TValue: for<'a> Deserialize<'a> {
        Ok(match pair? {
            Some((i_key, i_value)) => {
                let key = DBKey::from(i_key).expect("we assume correctness of our keys read from db").0;
                let value = DBValue::from(i_value)?.0;
                Some((key, value))
            },
            None => None
        })
    }

    pub fn first(&self) -> anyhow::Result<Option<(TKey, TValue)>> where TKey: DBKeyable, TValue: for<'a> Deserialize<'a> {
        Self::map_ivec_pair(self.this().first())
    }
    pub fn last(&self) -> anyhow::Result<Option<(TKey, TValue)>> where TKey: DBKeyable, TValue: for<'a> Deserialize<'a> {
        Self::map_ivec_pair(self.this().last())
    }
    pub fn next(&self, key: TKey) -> anyhow::Result<Option<(TKey, TValue)>> where TKey: DBKeyable + AsRef<[u8]>, TValue: for<'a> Deserialize<'a> {
        Self::map_ivec_pair(self.this().get_gt(DBKey(key)))
    }
    pub fn prev(&self, key: TKey) -> anyhow::Result<Option<(TKey, TValue)>> where TKey: DBKeyable + AsRef<[u8]>, TValue: for<'a> Deserialize<'a> {
        Self::map_ivec_pair(self.this().get_lt(DBKey(key)))
    }

    pub fn get_n_next_from(&self, key: TKey, n: usize) -> anyhow::Result<Vec<(TKey, TValue)>> where TKey: Copy + DBKeyable + AsRef<[u8]>, TValue: for<'a> Deserialize<'a> {
        let mut out = Vec::new();
        let start = self.get(key)?;
        if start.is_none() || n == 0 {
            return Ok(out);
        }
        out.push((key, start.unwrap()));
        if n == 1 {
            return Ok(out);
        }
        let mut count: usize = 1;
        let mut key = key;
        loop {
            let next = Self::map_ivec_pair(self.this().get_gt(key))?;
            if next.is_none() {
                break;
            }
            count += 1;
            let next = next.unwrap();
            key = next.0;
            let value = next.1;

            out.push((key, value));
            if count >= n {
                break;
            }
        }

        Ok(out)
    }

    // TODO: iter translation layer
    // TODO: batch translation layer
    // TODO: transaction translation layer
}

/// Do not use
enum DBTreeInner<TKey, TValue> {
    Tree(Tree),
    _UnusedKey(TKey),
    _UnusedValue(TValue),
}


// ======================================
// => specific value / key implementation
// ======================================

pub type UserKey = UuidKey;
pub type ChannelKey = UuidKey;
pub type MessageKey = UuidKey2;
impl MessageKey {
    pub fn with_channel(channel_key: ChannelKey) -> Self {
        UuidKey2(channel_key.0, Uuid::default())
    }
}

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
    pub fn text(display_name: String, desc: String) -> Self {
        Self {
            channel_type: ChannelType::Text,
            display_name,
            desc,
        }
    }
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
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    // IDEA: join data, bio
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
    pub fn now(author: UserKey, content: String) -> Self {
        Self::new(Utc::now(), author, content)
    }
    pub fn new(timestamp: DateTime<Utc>, author: UserKey, content: String) -> Self {
        Self {
            timestamp,
            author,
            content
        }       
    }
}

// ======================================
// => database
// ======================================
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

    pub fn messages(&self) -> Result<DBTree<MessageKey, MessageData>, sled::Error> {
       self.db.open_tree("messages").map(|t| DBTree::with(t))
    }
    pub fn channels(&self) -> Result<DBTree<ChannelKey, ChannelData>, sled::Error> {
       self.db.open_tree("channels").map(|t| DBTree::with(t))
    }
    pub fn users(&self) -> Result<DBTree<UserKey, UserData>, sled::Error> {
       self.db.open_tree("users").map(|t| DBTree::with(t))
    }
}
