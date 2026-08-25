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

use crate::client;
use clap::{Parser, Subcommand};

use crate::server;

pub const PROG: &str = "speakrs";
#[allow(unused)] // TODO
pub const PROG_YEAR: &str = "2026";
#[allow(unused)] // TODO
pub const PROG_AUTHORS: &str = "Miranda Große-Heilmann, Julie, Viki";

pub mod audio;
pub mod auth;
pub mod config;
pub mod database;
pub mod pagination;
pub mod rpc;
pub mod schema;

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
