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
use crate::common::{
    self,
    database::ServerDB,
    rpc::RpcServiceClient,
    schema::{ChannelData, ChannelKey, ClientData, UserData, UserKey},
};
use anyhow::{Context, Result};
use clap::Parser;
use std::{
    fmt::Debug,
    io::{self, Write},
    path::PathBuf,
};
use tarpc::tokio_serde::formats::Json;
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, info, info_span};

mod systemaudio;

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
        tui(args).await
    }
}

fn print_error(e: impl Debug) {
    println!("{e:?}");
}

/// Handle Result error case by printing the error and using "continue". Returns unwrapped result value.
macro_rules! try_continue {
    ($expr:expr $(,)?) => {
        match $expr {
            Ok(value) => value,
            Err(e) => {
                print_error(e);
                continue;
            }
        }
    };
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

// ==============================
// => GUI
// ==============================
fn gui(_args: ClientArguments) {
    speakrs_gui::run();
}

// ==============================
// => Client Data
// ==============================

#[tracing::instrument]
async fn tui(args: ClientArguments) -> Result<()> {
    let mut buffer = String::new();
    let stdin = io::stdin(); // We get `Stdin` here.

    loop {
        buffer.clear();
        let _ = stdin.read_line(&mut buffer)?;
        let repl_input = buffer.trim();
        let repl_args = repl_input.split(' ').collect::<Vec<&str>>();

        match repl_args[..] {
            ["exit"] => return Ok(()),
            ["help", ..] => println!("exit, connect <IP>:<PORT>, help"),
            ["connect", address] => {
                try_continue!(
                    repl_connect(address)
                        .await
                        .context("Error during connection")
                );
            }
            ["connect"] => println!("Missing connection address. Use `help`.",),
            ["connect", ..] => println!("Too many parameters. Use `help`.",),
            [cmd, ..] => println!("Unknown command `{cmd}`. Use `help`.",),
            [] => {}
        };
    }
}

// TODO connection state

async fn repl_connect(addr: impl ToSocketAddrs + Debug) -> Result<()> {
    info!("Connecting to address: {:?}", addr);
    let mut transport = tarpc::serde_transport::tcp::connect(addr, Json::default);
    transport.config_mut().max_frame_length(usize::MAX);
    let client = RpcServiceClient::new(tarpc::client::Config::default(), transport.await?).spawn();

    let data = client
        .clone()
        .get_server_data(tarpc::context::current())
        .instrument(info_span!("Asking server for server data"))
        .await
        .context("RpcError during connection attempt")?
        .context("Error while talking to server")?;

    println!("Found server `{}`", data.name);

    let db = ServerDB::magic_open_client(data.name, data.uuid)?;
    let mut client_data = db
        .get_client_data()
        .context("Error while reading local database")?;
    if client_data.is_none() {
        println!("Server has not been registered with, would you like to create a user? [y/n]");
        let mut buffer = String::new();
        let _ = io::stdin().read_line(&mut buffer)?;
        if !(buffer.starts_with("y") || buffer.starts_with("Y")) {
            println!("answer didn't start with y, assuming no.");
            return Ok(());
        }
        print!("Username: ");
        if let Err(e) = io::stdout().flush() {
            panic!("could not flush stdout: {}", e);
        }
        buffer.clear();
        let _ = io::stdin().read_line(&mut buffer)?;
        let username = buffer;
        let user_data = UserData::new(username.clone());

        let user_key = client
            .clone()
            .create_user(tarpc::context::current(), user_data.clone())
            .instrument(info_span!("Asking server for new user"))
            .await
            .context("RpcError during connection attempt")?
            .context("Error while talking to server")?;

        let data = ClientData {
            user_key: user_key.clone(),
        };
        client_data = Some(data.clone());
        db.set_client_data(data)
            .context("Error while writing to local database")?;

        db.users()?.insert((user_key, user_data))?;
        println!("Created user {} with uuid {}.", username, user_key);
    }
    let client_data = client_data.unwrap();
    let user = client_data.user_key;

    repl_channel_sync(client.clone(), db.clone(), user.clone()).await?;

    loop {
        let mut buffer = String::new();
        buffer.clear();
        let _ = io::stdin().read_line(&mut buffer)?;
        let repl_input = buffer.trim();
        let repl_args = repl_input.split(' ').collect::<Vec<&str>>();
        match repl_args[..] {
            ["exit"] => return Ok(()),
            ["help", ..] => println!("exit, help, channel"),
            ["channel", ..] => {
                try_continue!(
                    repl_channel(client.clone(), db.clone(), user.clone(), &repl_args[1..])
                        .await
                        .context("Error during channel command")
                )
            }
            ["connect"] => println!("Missing connection address. Use `help`.",),
            ["connect", ..] => println!("Too many parameters. Use `help`.",),
            [cmd, ..] => println!("Unknown command `{cmd}`. Use `help`.",),
            [] => {}
        };
    }
}

async fn repl_channel(
    client: RpcServiceClient,
    db: ServerDB,
    user: UserKey,
    args: &[&str],
) -> Result<()> {
    match args {
        ["sync"] => return repl_channel_sync(client, db, user).await,
        ["add"] => return repl_channel_add(client, db, user).await,
        ["list"] => return repl_channel_list(db).await,
        ["help"] | [] => println!("help, sync, list, add"),
        [cmd, ..] => println!("Unknown subcommand `channel {cmd}`. Use `channel help`."),
    }
    Ok(())
}

async fn repl_channel_sync(client: RpcServiceClient, db: ServerDB, user: UserKey) -> Result<()> {
    println!("Syncing channels...");
    let last_known_channel = db.channels()?.last()?.map(|kv| kv.0);
    let new_channels = client
        .clone()
        .get_new_channels_since(tarpc::context::current(), user.clone(), last_known_channel)
        .instrument(info_span!("Asking server for channel list"))
        .await?
        .context("Error while talking to server")?;
    let len = new_channels.len();
    for channel in new_channels {
        db.channels()?.insert((channel.0, channel.1))?;
    }
    println!("Got {} new channels. Use `channel list` to list them.", len);
    Ok(())
}
async fn repl_channel_add(client: RpcServiceClient, db: ServerDB, user: UserKey) -> Result<()> {
    print!("Channel name: ");
    io::stdout().flush()?;
    let mut buffer = String::new();
    let _ = io::stdin().read_line(&mut buffer)?;
    let name = buffer.clone();
    buffer.clear();
    print!("Channel description: ");
    io::stdout().flush()?;
    let _ = io::stdin().read_line(&mut buffer)?;
    let desc = buffer;
    let data = ChannelData::text(name.clone(), desc);

    let key = client
        .clone()
        .create_channel(tarpc::context::current(), user.clone(), data.clone())
        .instrument(info_span!("Creating channel in server"))
        .await?
        .context("Error while talking to server")?;

    db.channels()?.insert((key, data))?;
    println!("Created channel {} with uuid {}", name, key);
    Ok(())
}

async fn repl_channel_list(db: ServerDB) -> Result<()> {
    let channels = db
        .channels()?
        .range(..)
        .collect::<anyhow::Result<Vec<(ChannelKey, ChannelData)>>>()?;
    for (_, value) in channels {
        println!(
            "Channel `{}`: \"{}\"",
            value.get_name(),
            value.get_description()
        )
    }
    Ok(())
}
