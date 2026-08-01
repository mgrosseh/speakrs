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
#![allow(dead_code)]

use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::Utc;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::RwLock;
use uuid::Uuid;

use crate::client;
use crate::client::ClientConfig;
use crate::common::key::PrefixedKey;
use crate::common::key::SingletonKey;
use crate::common::key::UuidKey;
use crate::common::table::SerdeTree;
use crate::common::table::TableDecl;
use crate::server;
use crate::server::ServerConfig;

pub const PROG: &str = "speakrs";
pub const _PROG_YEAR: &str = "2026";
pub const _PROG_AUTHORS: &str = "Miranda Große-Heilmann, Julie, Viki";

pub mod codec;
pub mod key;
pub mod table;
pub mod tree;

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
const _CONFIG_ENV_VAR_PREFIX: &str = "SPEAKRS";
const _CONFIG_CLIENT_PREFIX: &str = "SPEAKRS_CLIENT";
const _CONFIG_SERVER_PREFIX: &str = "SPEAKRS_SERVER";
const CONFIG_DIR_OVERRIDE_ENV: &str = "SPEAKRS_CONFIG_HOME";
const CONFIG_DIR_NAME: &str = "speakrs";
const CONFIG_NAME: &str = "speakrs.toml";
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    /// Config for server, required if running in server mode
    server: Option<server::ServerConfig>,
    /// Config for client, required if running in client mode
    client: Option<client::ClientConfig>,
}
impl Config {
    // TODO: writing values / storing to disk
    /// Get global config
    /// The global config might change or get reloaded, be aware if storing values longterm.
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
        let contents = std::fs::read_to_string(path).expect("Failed to read config file."); // TODO: proper handling
        toml::from_str(contents.as_str()).expect("Could not parse toml") // TODO: proper handling
    }
    fn reload_from_disk() {
        *Self::get().write().unwrap() = Self::load();
    }

    /// Acquire read lock of global config and clone snapshot of server config
    /// Config might reload from disk, the cloned config would then be outdated.
    pub fn clone_server() -> Option<server::ServerConfig> {
        let conf = Self::get().read().unwrap();
        conf.server.clone()
    }
    /// Acquire read lock of global config and clone snapshot of client config
    /// Config might reload from disk, the cloned config would then be outdated.
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
                std::env::VarError::NotPresent => {} // let other cases set home
                std::env::VarError::NotUnicode(_) => println!(
                    "{}: WARNING: {} is not valid unicode, using fallback",
                    PROG, value
                ), // TODO: proper logging
            }
            return None;
        } else {
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
            }
            None => panic!(
                "HOME env var cannot be read, use {} env var or fix your environment.",
                CONFIG_DIR_OVERRIDE_ENV
            ),
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
// => specific value / key implementation
// ======================================

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
    /// Create a text channel
    pub fn text(display_name: String, desc: String) -> Self {
        Self {
            channel_type: ChannelType::Text,
            display_name,
            desc,
        }
    }
    /// Create a voice channel
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
    /// Create MessageData with timestamp now
    pub fn now(author: UserKey, content: String) -> Self {
        Self::new(Utc::now(), author, content)
    }
    pub fn new(timestamp: DateTime<Utc>, author: UserKey, content: String) -> Self {
        Self {
            timestamp,
            author,
            content,
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

pub type UserKey = UuidKey<UserData>;
pub type ChannelKey = UuidKey<ChannelData>;
pub type MessageKey = PrefixedKey<ChannelKey, MessageData>;

const USERS_TABLE: TableDecl<SerdeTree<UserData>> = TableDecl::named("users");
const CHANNELS_TABLE: TableDecl<SerdeTree<ChannelData>> = TableDecl::named("channels");
const MESSAGES_TABLE: TableDecl<SerdeTree<MessageData, MessageKey>> = TableDecl::named("messages");
const SERVER_DATA_TABLE: TableDecl<SerdeTree<ServerData, SingletonKey>> =
    TableDecl::named("server_data");

impl ServerDB {
    /// Opens database at [`database_location`].
    /// If database did not exist before, it is NOT initialized!
    pub fn open(database_location: &str) -> Self {
        let db = sled::open(database_location).expect("open");
        Self { db }
    }

    /// Open database at [`location_location`]`.
    /// If database did not exist, use data to initialize it.
    pub fn create_or_open(database_location: PathBuf, data: ServerData) -> Result<Self> {
        let db = sled::open(database_location).expect("open");
        let server_db = Self { db };

        if server_db.is_init()? {
            return Ok(server_db);
        }
        server_db.set_server_data(data)?;

        Ok(server_db)
    }

    /// Open database with name [`name`].
    /// Automatically (magically) find the location where databases are stored and select the database with name from it.
    /// If the database does not exist, creates it and initializes it with new server data using current time as uuid seed.
    pub fn magic_open_server(name: String) -> Result<Self> {
        let mut path = ServerConfig::get().get_database_directory();
        path.push(name.as_str());
        let uuid = Uuid::now_v7();
        Self::create_or_open(path, ServerData { name, uuid })
    }
    /// Open database with corresponding to [`uuid`].
    /// Automatically (magically) find the location where databases are stored and select the database with corresponding uuid from it.
    /// If the database does not exist, creates it and initializes it with new server data.
    pub fn magic_open_client(uuid: Uuid) -> Result<Self> {
        let mut path = ClientConfig::get().get_database_directory();
        let name = uuid.to_string();
        path.push(name.as_str());
        Self::create_or_open(path, ServerData { name, uuid })
    }

    /// Queries the database, if initialized (server data was set) return true.
    pub fn is_init(&self) -> sled::Result<bool> {
        let tree = self.db.open_tree("server_data")?;
        Ok(tree.get("data")?.is_some())
    }

    /// Get server data
    /// Run [`ServerDB::is_init()`] first to check if it's safe to get data
    pub fn get_server_data(&self) -> Result<ServerData> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        tree.get(SingletonKey)?.ok_or_else(|| anyhow!("Expect data in server_data, run is_init() before accessing or set_server_data() on db."))
    }
    /// Sets server data.
    /// Either replaces existing data with new one or initializes the database with corresponding data.
    pub fn set_server_data(&self, data: ServerData) -> Result<()> {
        let tree = SERVER_DATA_TABLE.open(&self.db)?;
        tree.insert(SingletonKey, data)?;
        Ok(())
    }

    /// Get DBTree of all Messages, allowing querying, and storing data.
    pub fn messages(&self) -> sled::Result<SerdeTree<MessageData, MessageKey>> {
        MESSAGES_TABLE.open(&self.db)
    }
    /// Get DBTree of all Channels, allowing querying, and storing data.
    pub fn channels(&self) -> sled::Result<SerdeTree<ChannelData>> {
        CHANNELS_TABLE.open(&self.db)
    }
    /// Get DBTree of all Users, allowing querying, and storing data.
    pub fn users(&self) -> sled::Result<SerdeTree<UserData>> {
        USERS_TABLE.open(&self.db)
        // self.db.open_tree("users").map(|t| DBTree::from_raw(t))
    }
}
