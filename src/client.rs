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
use clap::Parser;
use crate::common::{
    self,
    database::ServerDB,
    rpc::RpcServiceClient,
    schema::{ClientData, UserKey},
};
use std::{
    fmt::{Debug},
    path::PathBuf,
    sync::{OnceLock, RwLock},
};
use tarpc::tokio_serde::formats::Json;
use tokio::{net::ToSocketAddrs};

pub mod repl;

mod systemaudio;
use ringbuf::traits::Split;
use cpal::traits::StreamTrait;

#[derive(Debug, Parser)]
pub(crate) struct ClientArguments {
    /// With GUI, if false, runs TUI
    #[clap(long, default_value_t = false)]
    gui: bool,
}

pub(crate) async fn run(args: ClientArguments) -> Result<()> {
    if args.gui {
        gui(args);
        return Ok(());
    } else {
        repl::repl(args).await
    }
}

fn gui(_args: ClientArguments) -> Result<()> {
    let settings = systemaudio::SystemSettings::default();
    //if default audio device works, monitor mic
    if let Some(v) = settings.input_config{
        let audio_buffer: systemaudio::AudioBuffer = systemaudio::AudioBuffer::new(30.0, v);
        let (producer, consumer) = audio_buffer.ring.split();
        let input_stream = systemaudio::capture_audio(Default::default(), producer);
        let output_stream = systemaudio::receive_audio(Default::default(), consumer);
        if let Some(input_stream) = input_stream{
            match input_stream.play() {
                Ok(v) => tracing::info!("audio input stream started"),
                Err(e) => tracing::warn!("audio input stream not started"),
            }
        }
        else {
            tracing::warn!("No input audio")
        }
        if let Some(output_stream) = output_stream{
            match output_stream.play() {
                Ok(v) => tracing::info!("audio output stream started"),
                Err(e) => tracing::warn!("audio output stream not started"),
            }
        }
        else {
            tracing::warn!("No output audio")
        }
    }
    else {
        tracing::warn!("Default Config not found");
    }
    speakrs_gui::run();
    return Ok(());
}


// ==============================
// => Connection
// ==============================
// TODO: in the future having only one connection is bad, so current_connection() and clone_current_connection() will probably need overhauls
#[derive(Debug, Clone)]
pub enum Connection {
    Empty,
    Active(ActiveConnection),
}
#[derive(Debug, Clone)]
pub struct ActiveConnection {
    service_client: RpcServiceClient,
    db: ServerDB,
    client_data: ClientData,
}
impl Connection {
    async fn create_service_client(addr: impl ToSocketAddrs) -> Result<RpcServiceClient> {
        let mut transport = tarpc::serde_transport::tcp::connect(addr, Json::default);
        transport.config_mut().max_frame_length(usize::MAX);
        Ok(RpcServiceClient::new(tarpc::client::Config::default(), transport.await?).spawn())
    }
    fn new(service_client: RpcServiceClient, db: ServerDB, client_data: ClientData) -> Self {
        Self::Active(ActiveConnection { service_client, db, client_data })
    }
    fn has(&self) -> bool {
        match self {
            Self::Empty => false,
            Self::Active(_) => true,
        }
    }
    fn unwrap(self) -> (RpcServiceClient, ServerDB, ClientData) {
        match self {
            Self::Empty => panic!("ReplConnection: calling unwrap() on Empty, always call has() first."),
            Self::Active(c) => (c.service_client, c.db, c.client_data),
        }
    }
    #[allow(unused)]
    fn client(&self) -> &RpcServiceClient {
        match self {
            Self::Empty => panic!("ReplConnection: calling client() on Empty, always call has() first."),
            Self::Active(connection) => &connection.service_client,
        }
    }
    fn db(&self) -> &ServerDB {
        match self {
            Self::Empty => panic!("ReplConnection: calling db() on Empty, always call has() first."),
            Self::Active(connection) => &connection.db,
        }
    }
    #[allow(unused)]
    fn user_key(&self) -> UserKey {
        match self {
            Self::Empty => panic!("ReplConnection: calling user_key() on Empty, always call has() first."),
            Self::Active(connection) => connection.client_data.user_key,
        }
    }
}

pub fn current_connection() -> &'static RwLock<Connection> {
    static CONNECTION: OnceLock<RwLock<Connection>> = OnceLock::new();
    CONNECTION.get_or_init(|| RwLock::new(Connection::Empty))
}
pub fn clone_current_connection() -> Connection {
    current_connection().read().unwrap().clone()
}

// ==============================
// => Config
// ==============================
// NOTE: For Devs: Try to annotate every value with `///` and explain what it does
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ClientConfig {
    /// Database related settings
    database: ClientConfigDatabase,
}
#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct ClientConfigDatabase {
    /// Directory to store client databases in, if empty stores databases next to config.
    /// If set to `/some/dir` creates `/some/dir/client` and `/some/dir/client/<uuid>` for each database.
    directory: Option<String>,
}
impl ClientConfig {
    /// See [`ClientConfig::database`]
    pub fn get_database_directory(&self) -> PathBuf {
        let mut path = if self.database.directory.is_some() {
            PathBuf::from(self.database.directory.clone().unwrap())
        } else {
            let mut path = common::config_home();
            path.push("databases");
            path
        };
        path.push("client");
        path
    }
    /// Get ClientConfig from cached unified Config.
    /// This is a relative expensive operation (clones ClientConfig from R/W locked Config value), it might be deprecated in the future.
    /// TODO: currently throws an error if config does not have a client section
    pub fn get() -> Self {
        common::Config::clone_client()
            .expect("Running client requires config to have client section")
    }
}
