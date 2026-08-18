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
mod common;
mod client;
mod server;

use clap::Parser;

use crate::common::Arguments;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args =  Arguments::parse();

    // TODO: make sure files dont get tooo big
    let log_directory = common::config_home().join("logs");
    let log_file = match args.command {
        common::Commands::Client(_) => "client_log.log",
        common::Commands::Server(_) => "server_log.log",
    };
    let (non_blocking, _guard) = tracing_appender::non_blocking(tracing_appender::rolling::never(log_directory, log_file));
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .init();

    match args.command {
        common::Commands::Client(args) => {
            client::run(args).await
        }
        common::Commands::Server(args) => {
            server::run(args).await
        }
    }
}
