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
use std::{net::SocketAddr, sync::{Arc, Mutex}, time::SystemTime};

use clap::{Parser};

use tarpc::tokio_serde::formats::Json;
use tracing::{Instrument, info, info_span, warn};

use crate::common::{self};

#[derive(Debug, Parser)]
pub(crate) struct ClientArguments {
    /// The address to connect to (port should be same as server)
    #[clap(long)]
    server_addr: SocketAddr,
    /// With GUI, if false, runs TUI
    #[clap(long, default_value_t = false)]
    gui: bool,
}

pub(crate) async fn run(args: ClientArguments) -> anyhow::Result<()> {
    if args.gui {
        gui(args);
        return Ok(());
    }
    else {
        tui(args).await
    }
}

// ==============================
// => Put GUI code here
// ==============================
fn gui(args: ClientArguments) {
    speakrs_gui::run();
}


// ==============================
// => Client Data
// ==============================

#[tracing::instrument]
async fn tui(args: ClientArguments) -> anyhow::Result<()> {
    info!("Sending data to address: {}", args.server_addr);
    let mut transport = tarpc::serde_transport::tcp::connect(args.server_addr, Json::default);
    transport.config_mut().max_frame_length(usize::MAX);

    let client = common::WorldClient::new(tarpc::client::Config::default(), transport.await?).spawn();
    let send_client = client.clone();

    let result = async move {
        send_client.send_message(tarpc::context::current(), 0, 0, "Test Message".to_string()).await
    }
    .instrument(info_span!("Sending Message"))
    .await;

    let message_id = result.unwrap().unwrap();
    println!("Send id {}", message_id);


    let send_client = client.clone();
    let result = async move {
        send_client.pull_messages(tarpc::context::current(), 0, 10).await
    }
    .instrument(info_span!("Pulling Messages"))
    .await;

    let messages = result.unwrap().unwrap();

    println!();
    println!("Messages ({}):", messages.len());
    for m in messages {
        println!("<{}> Author{}: {}", m.timestamp.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(), m.author, m.content);
    }

    // match hello {
    //     Ok(_) => info!("{hello:?}"),
    //     Err(e) => warn!("{:?}", anyhow::Error::from(e)),
    // }

    Ok(())
}

// ==============================
// => Client Data
// ==============================
#[derive(Clone, Debug)]
struct ClientData {
    db: Arc<Mutex<common::Server>>,
}

impl ClientData {
    fn new() -> Self {
        let db = Arc::new(Mutex::new(common::Server::new()));
        Self {
            db
        }
    }

    fn lock_db(&mut self) -> Result<std::sync::MutexGuard<'_, common::Server>, std::sync::PoisonError<std::sync::MutexGuard<'_, common::Server>>> {
        self.db.lock()
    }
}
