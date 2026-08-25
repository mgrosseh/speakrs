use std::{
    path::PathBuf,
    sync::{OnceLock, RwLock},
};

use crate::{client, common::PROG, server};

/// Common Config
///
/// TODO: using watch.rs example as reference, implement hot-reloading
///       see https://github.com/rust-cli/config-rs/blob/main/examples/watch.rs to create hot-reloading
/// TODO: full docs

const CONFIG_DIR_OVERRIDE_ENV: &str = "SPEAKRS_CONFIG_HOME";
const CONFIG_DIR_NAME: &str = "speakrs";
const CONFIG_NAME: &str = "speakrs.toml";
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Config {
    // TODO: remove Option, tell serde to deserialize Default, and ignore serialize of Default values.
    /// Config for server, required if running in server mode
    server: Option<crate::server::ServerConfig>,
    /// Config for client, required if running in client mode
    client: Option<crate::client::ClientConfig>,
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
