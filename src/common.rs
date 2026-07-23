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
use core::fmt;
use std::{collections::BTreeMap, error::Error, time::SystemTime};
use clap::{Parser, Subcommand};

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
    async fn pull_messages(channel_id: ChannelId, limit: usize) -> ServerResult<Vec<Message>>;
    async fn send_message(channel_id: ChannelId, user_id: UserId, content: String) -> ServerResult<MessageId>;
}

// ======================================
// => server struct
// ======================================


#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum ServerError {
    NoSuchMessage(MessageId),
    NoSuchChannel(ChannelId),
    NoSuchUser(UserId),
    MessageNotLoaded(MessageId),
    ChannelNotLoaded(ChannelId),
    UserNotLoaded(UserId),
}
impl Error for ServerError {
}
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchMessage(id) => write!(f, "Error: Message with requested id does not exist: `{}`", id),
            Self::NoSuchChannel(id) => write!(f, "Error: Channel with requested id does not exist: `{}`", id),
            Self::NoSuchUser(id) => write!(f, "Error: User with requested id does not exist: `{}`", id),
            Self::MessageNotLoaded(id) => write!(f, "Error: Message with requested id is not loaded: `{}`", id),
            Self::ChannelNotLoaded(id) => write!(f, "Error: Channel with requested id is not loaded: `{}`", id),
            Self::UserNotLoaded(id) => write!(f, "Error: User with requested id is not loaded: `{}`", id),
        }
    }
}

pub(crate) type ServerResult<T> = Result<T, ServerError>;

pub type MessageId = u64;
pub type UserId = u64;
pub type ChannelId = u64;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Server {
    channels: BTreeMap<ChannelId, Option<TextChannel>>,
    users: BTreeMap<UserId, Option<User>>,
}

impl Server {

    pub(crate) fn new() -> Self {
        let channels = BTreeMap::new();
        let users = BTreeMap::new();
        Self {
            channels,
            users,
        }
    }

    pub(crate) fn send_message(&mut self, channel: ChannelId, user: UserId, content: String) -> ServerResult<MessageId> {
        if !self.channels.contains_key(&channel) {
            self.channels.insert(channel, Some(TextChannel::new("AutoChannel".to_string(), "Auto Generated Channel".to_string())));
        }
        let channel = self.channels.get_mut(&channel).unwrap().as_mut().unwrap();
        channel.add_message(Option::None, user, content)
    }
    pub(crate) fn pull_messages(&self, channel: ChannelId, limit: usize) -> ServerResult<Vec<Message>> {
        let channel = self.channels.get(&channel).unwrap().as_ref().unwrap();
        Ok(channel.messages.values().map(|x| x.clone().unwrap()).take(limit).collect())
    }

}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct TextChannel {
    name: String,
    desc: String,
    messages: BTreeMap<MessageId, Option<Message>>,
}

impl TextChannel {
    fn new(name: String, desc: String) -> Self {
        let messages = BTreeMap::new();

        Self {
            name,
            desc,
            messages
        }
    }

    fn new_message_id(&self) -> MessageId {
        self.messages.keys().max().map(|k| k + 1).unwrap_or_default()
    }

    fn load_message(&mut self, id: MessageId, timestamp: SystemTime, user: UserId, content: String) -> ServerResult<MessageId> {
        let message = Message::new(timestamp, user, content);
        self.messages.insert(id, Some(message)); // TODO: handle if key exists
        Ok(id)
    }

    fn add_message(&mut self, timestamp: Option<SystemTime>, user: UserId, content: String) -> ServerResult<MessageId> {
        let id = self.new_message_id();
        let timestamp = timestamp.unwrap_or(SystemTime::now());
        self.load_message(id, timestamp, user, content)
    }

    fn get_message(&self, id: MessageId) -> Option<&Message> {
        match self.messages.get(&id) {
            None => None,
            Some(x) => x.as_ref()
        }
    }

}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    id: UserId,
    name: String,
}
impl User {
    fn new(id: UserId, name: String) -> Self {
        Self { id, name }
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub timestamp: SystemTime,
    pub author: UserId,
    pub content: String,
}
impl Message {
    fn new(timestamp: SystemTime, author: UserId, content: String) -> Self {
        Self {
            timestamp,
            author,
            content
        }       
    }
}
