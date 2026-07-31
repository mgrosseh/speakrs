use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::RwLock;

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

use crate::client::ClientConfig;
use crate::server;
use crate::client;
use crate::server::ServerConfig;

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
// => Common Config
// ======================================
// TODO: using watch.rs example as reference, implement hot-reloading
//       see https://github.com/rust-cli/config-rs/blob/main/examples/watch.rs to create hot-reloading
// TODO: full docs
// We assume valid utf-8 for paths and values
const CONFIG_ENV_VAR_PREFIX: &str = "SPEAKRS";
const CONFIG_CLIENT_PREFIX: &str = "SPEAKRS_CLIENT";
const CONFIG_SERVER_PREFIX: &str = "SPEAKRS_SERVER";
const CONFIG_DIR_OVERRIDE_ENV: &str = "SPEAKRS_CONFIG_HOME";
const CONFIG_DIR_NAME: &str = "speakrs";
const CONFIG_NAME: &str = "speakrs.toml";
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    /// TODO
    server: Option<server::ServerConfig>,
    /// TODO
    client: Option<client::ClientConfig>,
}
impl Config {
    // TODO: writing values / storing to disk
    pub fn get() -> &'static RwLock<Config> {
        static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();
        CONFIG.get_or_init(|| {
            let config = Config::load();

            RwLock::new(config)
        })
    }
    fn load() -> Config {
        let mut path = config_home();
        path.push(CONFIG_NAME);
        let contents = std::fs::read_to_string(path)
            .expect("Should have been able to read file"); // TODO: proper handling
        toml::from_str(contents.as_str())
            .expect("Could not parse toml") // TODO: proper handling
    }
    fn reload_from_disk() {
        *Self::get().write().unwrap() = Self::load();
    }

    pub fn clone_server() -> Option<server::ServerConfig> {
        let conf = Self::get().read().unwrap();
        conf.server.clone()
    }
    pub fn clone_client() -> Option<client::ClientConfig> {
        let conf = Self::get().read().unwrap();
        conf.client.clone()
    }
}

// TODO currently this is called multiple times and recalculates path everytime -- should cache!
pub fn config_home() -> PathBuf {
    let unpack_env = |candidate_path: Result<String, std::env::VarError>, value: &str| {
        if candidate_path.is_err() {
            match candidate_path.unwrap_err() {
                std::env::VarError::NotPresent => {}, // let other cases set home
                std::env::VarError::NotUnicode(_) => println!("{}: WARNING: {} is not valid unicode, using fallback", PROG, value), // TODO: proper logging
            }
            return None;
        }
        else {
            return Some(PathBuf::from(candidate_path.unwrap()));
        }
    };
    if cfg!(target_os = "linux") {
        if let Some(v) = unpack_env(env::var(CONFIG_DIR_OVERRIDE_ENV), CONFIG_DIR_OVERRIDE_ENV) {
            return v;
        }
        let xdg_config_home = unpack_env(env::var("XDG_CONFIG_HOME"), "XDG_CONFIG_HOME");
        if let Some(mut v) = xdg_config_home {
            v.push(CONFIG_DIR_NAME);
            return v;
        }
        match unpack_env(env::var("HOME"), "HOME") {
            Some(home) => {
                let buf = PathBuf::from(home);
                return buf;
            },
            None => panic!("HOME env var cannot be read, use {} env var or fix your environment.", CONFIG_DIR_OVERRIDE_ENV),
        }
    }
    // see also target_family for more generic approach
    // other values (of intrest): windows, macos, ios, android, freebsd, openbsd, netbsd
    todo!("Other operating systems are not supported currently.")
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

pub trait DBKeyable {
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

pub struct DBTree<TKey, TValue>(Tree, PhantomData<(TKey, TValue)>);
impl<TKey, TValue> DBTree<TKey, TValue> {
    fn with(tree: Tree) -> Self {
        Self(tree, PhantomData)
    }

    fn this(&self) -> &Tree {
        &self.0
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ServerData {
    /// Host system unique name
    pub name: String,
    pub uuid: Uuid,
}

// ======================================
// => database
// ======================================
#[derive(Debug, Clone)]
pub struct ServerDB {
    db: sled::Db,
}

impl ServerDB {
    /// Opens database at [`database_location`].
    /// If database did not exist before, it is NOT initialized!
    pub fn open(database_location: &str) -> Self {
        let db = sled::open(database_location).expect("open");
        Self {
            db
        }
    }

    /// Open database at [`location_location`]`.
    /// If database did not exist, use data to initialize it.
    pub fn create_or_open(database_location: PathBuf, data: ServerData) -> Result<Self, sled::Error> {
        let db = sled::open(database_location).expect("open");
        let server_db = Self {
            db
        };

        if server_db.is_init()? {
            return Ok(server_db);
        }
        server_db.set_server_data(data)?;

        Ok(server_db)
    }

    /// Open database with name [`name`].
    /// Automatically (magically) find the location where databases are stored and select the database with name from it.
    /// If the database does not exist, creates it and initializes it with new server data using current time as uuid seed.
    pub fn magic_open_server(name: String) -> Result<Self, sled::Error> {
        let mut path = ServerConfig::get().get_database_directory();
        path.push(name.as_str());
        let uuid = Uuid::now_v7();
        Self::create_or_open(path, ServerData {name, uuid,})
    }
    /// Open database with corresponding to [`uuid`].
    /// Automatically (magically) find the location where databases are stored and select the database with corresponding uuid from it.
    /// If the database does not exist, creates it and initializes it with new server data.
    pub fn magic_open_client(uuid: Uuid) -> Result<Self, sled::Error> {
        let mut path = ClientConfig::get().get_database_directory();
        let name = uuid.to_string();
        path.push(name.as_str());
        Self::create_or_open(path, ServerData {name, uuid,})
    }

    /// Queries the database, if initialized (server data was set) return true.
    pub fn is_init(&self) -> Result<bool, sled::Error> {
        let tree = self.db.open_tree("server_data")?;
        Ok(tree.get("data")?.is_some())
    }

    /// Get server data
    /// Run [`ServerDB::is_init()`] first to check if it's safe to get data
    pub fn get_server_data(&self) -> anyhow::Result<ServerData> {
        let tree = self.db.open_tree("server_data")?;
        let val = tree.get("data")?.expect("Expect data in server_data, run is_init() before accessing or set_server_data() on db.");
        Ok(DBValue::from(val)?.0)
    }
    /// Sets server data.
    /// Either replaces existing data with new one or initializes the database with corresponding data.
    pub fn set_server_data(&self, data: ServerData) -> Result<(), sled::Error> {
        let tree = self.db.open_tree("server_data")?;
        tree.insert("data", DBValue(data))?;
        Ok(())
    }

    /// Get DBTree of all Messages, allowing querying, and storing data.
    pub fn messages(&self) -> Result<DBTree<MessageKey, MessageData>, sled::Error> {
       self.db.open_tree("messages").map(|t| DBTree::with(t))
    }
    /// Get DBTree of all Channels, allowing querying, and storing data.
    pub fn channels(&self) -> Result<DBTree<ChannelKey, ChannelData>, sled::Error> {
       self.db.open_tree("channels").map(|t| DBTree::with(t))
    }
    /// Get DBTree of all Users, allowing querying, and storing data.
    pub fn users(&self) -> Result<DBTree<UserKey, UserData>, sled::Error> {
       self.db.open_tree("users").map(|t| DBTree::with(t))
    }
}
