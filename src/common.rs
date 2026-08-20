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

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::RwLock;

use crate::client;

use crate::server;

pub const PROG: &str = "speakrs";
#[allow(unused)] // TODO
pub const PROG_YEAR: &str = "2026";
#[allow(unused)] // TODO
pub const PROG_AUTHORS: &str = "Miranda Große-Heilmann, Julie, Viki";

pub mod audio;
pub mod auth;
pub mod codec;
pub mod database;
pub mod key;
pub mod rpc;
pub mod schema;
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
    /// Be verbose (displays logging on stdout)
    /// Note: this will disable the log file output
    #[clap(short, long, default_value_t = false)]
    pub verbose: bool,
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
const CONFIG_DIR_OVERRIDE_ENV: &str = "SPEAKRS_CONFIG_HOME";
const CONFIG_DIR_NAME: &str = "speakrs";
const CONFIG_NAME: &str = "speakrs.toml";
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    // TODO: remove Option, tell serde to deserialize Default, and ignore serialize of Default values.
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
    #[allow(dead_code)]
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
                let mut buf = PathBuf::from(home);
                buf.push(".config");
                buf.push(CONFIG_DIR_NAME);
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

// =========
// validity
// =========

// TODO: massively improve
pub const USERNAME_RULES: &str = "Must have no whitespace.";
pub fn is_valid_username(x: &impl AsRef<str>) -> bool {
    !x.as_ref().contains(" ")
}
pub const CHANNEL_NAME_RULES: &str = "Must have no whitespace.";
pub fn is_valid_channel_name(x: &impl AsRef<str>) -> bool {
    !x.as_ref().contains(" ")
}
