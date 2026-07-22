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
use std::{io::Write, net::TcpStream, time::{Duration, SystemTime}};

use nom::{IResult, Parser, bytes::{complete::{tag, take_till, take_while}, take, take_while1}, character::complete::char, error::{self, ErrorKind, ParseError}, sequence::{delimited, preceded}};

use crate::common::{ChannelId, Message, MessageId, NetworkCodable, PROTOCOL_END_CHAR, PROTOCOL_KEYWORD, ServerError, TextChannel, User, UserId};

// ==============================
// => Speakrs Protocol
// ==============================
pub(crate) enum Protocol {
    NewData(NewDataProtocol),
    SendData(SendDataProtocol),
    RegisterData(RegisterDataProtocol),
    GetData(GetDataProtocol),
    DeleteData(DeleteDataProtocol),
    ServerError(ServerError),
}
impl Protocol {
    pub(crate) fn create_channel(name: String, desc: String) -> Self {
        Protocol::NewData(NewDataProtocol::Channel(NewChannelProtocol { name, desc }))
    }
    pub(crate) fn send_message(channel: ChannelId, timestamp: SystemTime, user: UserId, content: String) -> Self {
        Protocol::NewData(NewDataProtocol::Message(NewMessageProtocol { channel, timestamp, user, content } ))
    }
    pub(crate) fn add_user(username: String) -> Self {
        Protocol::NewData(NewDataProtocol::User(NewUserProtocol { username }))
    }
    pub(crate) fn register_messages(channel: ChannelId, messages: Vec<MessageId>) -> Self {
        match RegisterDataProtocol::messages(channel, messages) {
            None => panic!("`messages` must have size > 0"),
            Some(x) => Protocol::RegisterData(x),
        }
    }
    pub(crate) fn register_users(users: Vec<UserId>) -> Self {
        match RegisterDataProtocol::users(users) {
            None => panic!("`users` must have size > 0"),
            Some(x) => Protocol::RegisterData(x),
        }
    }
    pub(crate) fn register_channels(channels: Vec<ChannelId>) -> Self {
        match RegisterDataProtocol::channels(channels) {
            None => panic!("`channels` must have size > 0"),
            Some(x) => Protocol::RegisterData(x),
        }
    }
    pub(crate) fn get_message(channel: ChannelId, messages: Vec<MessageId>) -> Self {
        match GetDataProtocol::messages(channel, messages) {
            None => panic!("`messages` must have size > 0"),
            Some(x) => Protocol::GetData(x),
        }
    }
    pub(crate) fn get_user(users: Vec<UserId>) -> Self {
        match GetDataProtocol::users(users) {
            None => panic!("`users` must have size > 0"),
            Some(x) => Protocol::GetData(x),
        }
    }
    pub(crate) fn get_channel(channels: Vec<ChannelId>) -> Self {
        match GetDataProtocol::channels(channels) {
            None => panic!("`users` must have size > 0"),
            Some(x) => Protocol::GetData(x),
        }
    }

    pub(crate) fn send_protocol(&self, mut stream: &TcpStream) -> Result<(), std::io::Error> {
        let mut encoded = self.encode();
        encoded.push(PROTOCOL_END_CHAR);
        let buf = encoded.as_bytes();
        stream.write_all(buf)
    }


    pub(crate) fn parse_protocol_with_error_handling(request: String, verbose: bool) -> Option<Protocol> {
        let wrapped_command = Protocol::decode(request.as_bytes());
        if let Err(x) = wrapped_command {
            if verbose {
                println!("Failed to parse command: `{}` || error: `{}`", request, x);
            }
            return None;
        }
        let (rest, command) = wrapped_command.unwrap();
        if verbose && !rest.eq(&[PROTOCOL_END_CHAR as u8; 1]) && !rest.is_empty() { // rest should always be empty but avoid crashes here just in case
            print!("Warning: Request contains trailing data: chars: `{}`; ", str::from_utf8(&rest[1..]).unwrap());
            print!(" u8:");
            for c in &rest[1..] {
                print!(" {}", c);
            }
            println!()
        }
        Some(command)
    }
}

impl NetworkCodable for Protocol {
    fn encode(&self) -> String {
        let mut encoded = String::from(PROTOCOL_KEYWORD);
        encoded.push(' ');
        encoded.push_str(match self {
            Self::NewData(cmd) => cmd.encode(),
            Self::SendData(cmd) => cmd.encode(),
            Self::RegisterData(cmd) => cmd.encode(),
            Self::GetData(cmd) => cmd.encode(),
            Self::DeleteData(cmd) => cmd.encode(),
            Self::ServerError(cmd) => todo!(),
        }.as_str());

        encoded
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let (string, _) =  (protocol, tag(" ")).parse(string)?;

        let result = RegisterDataProtocol::decode(string).map(|(input, command)| (input, Self::RegisterData(command)));
        if result.is_ok() {
            return result;
        }
        let result = GetDataProtocol::decode(string).map(|(input, command)| (input, Self::GetData(command)));
        if result.is_ok() {
            return result;
        }
        let result = DeleteDataProtocol::decode(string).map(|(input, command)| (input, Self::DeleteData(command)));
        if result.is_ok() {
            return result;
        }
        let result = NewDataProtocol::decode(string).map(|(input, command)| (input, Self::NewData(command)));
        if result.is_ok() {
            return result;
        }
        let result = SendDataProtocol::decode(string).map(|(input, command)| (input, Self::SendData(command)));
        if result.is_ok() {
            return result;
        }
        // let result = ServerError::decode(string).map(|(input, command)| (input, Self::ServerError(command)));
        // if result.is_ok() {
        //     return result;
        // }

        Err(nom::Err::Failure(error::Error::new(string, ErrorKind::Fail)))
    }
}




// ==============================
// => Helpers
// ==============================
trait HasProtocolKeyword {
    fn keyword() -> &'static str;
}

fn protocol_field_value<'a, E: ParseError<&'a [u8]>>() -> impl Parser<&'a [u8], Output = &'a [u8], Error = E> {
    delimited(char('['), take_while(|c| c != b']'), char(']'))
}
fn protocol_space_ending_word<'a, E: ParseError<&'a [u8]>>() -> impl Parser<&'a [u8], Output = &'a [u8], Error = E> {
    take_till(|c| c == b' ')
}

fn protocol_field<'a, E: ParseError<&'a [u8]>>(name_equals: &[u8]) -> impl Parser<&'a [u8], Output = &'a [u8], Error = E> {
    preceded(tag(name_equals), protocol_field_value())
}

// ==============================
// => New Data
// ==============================
pub(crate) enum NewDataProtocol {
    Message(NewMessageProtocol),
    User(NewUserProtocol),
    Channel(NewChannelProtocol),
}
impl HasProtocolKeyword for NewDataProtocol { fn keyword() -> &'static str { "NewData" } }
impl NetworkCodable for NewDataProtocol {
    fn encode(&self) -> String {
        let mut encoded = String::from(Self::keyword());
        encoded.push(' ');

        encoded.push_str(match self {
            NewDataProtocol::Message(protocol) => protocol.encode(),
            NewDataProtocol::User(protocol) => protocol.encode(),
            NewDataProtocol::Channel(protocol) => protocol.encode(),
        }.as_str());

        encoded
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (string, (_, _)) = (tag(Self::keyword()), tag(" ")).parse(string)?;

        let result = NewChannelProtocol::decode(string).map(|(input, command)| (input, Self::Channel(command)));
        if result.is_ok() {
            return result;
        }
        let result = NewMessageProtocol::decode(string).map(|(input, command)| (input, Self::Message(command)));
        if result.is_ok() {
            return result;
        }
        let result = NewUserProtocol::decode(string).map(|(input, command)| (input, Self::User(command)));
        if result.is_ok() {
            return result;
        }

        Err(nom::Err::Failure(error::Error::new(string, ErrorKind::Fail)))
    }
}

pub(crate) struct NewChannelProtocol {
    pub name: String,
    pub desc: String,
}
impl HasProtocolKeyword for NewChannelProtocol { fn keyword() -> &'static str { "NewChannel" } }
impl NetworkCodable for NewChannelProtocol {
    fn encode(&self) -> String {
        format!("{} name=[{}] desc=[{}]", Self::keyword(), self.name, self.desc)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> {
        let (input, (_, _, name, _, desc)) = (
            tag(Self::keyword()), 
            tag(" "), protocol_field(b"name="),
            tag(" "), protocol_field(b"desc=")).parse(string)?;

        let name = str::from_utf8(name).unwrap().to_string();
        let desc = str::from_utf8(desc).unwrap().to_string();

        Ok((input, Self { name, desc }))
    }
}

pub(crate) struct NewMessageProtocol {
    pub channel: ChannelId,
    pub timestamp: SystemTime,
    pub user: UserId,
    pub content: String,
}
impl HasProtocolKeyword for NewMessageProtocol { fn keyword() -> &'static str { "NewMessage" } }
impl NetworkCodable for NewMessageProtocol {
    fn encode(&self) -> String {
        format!("{} channel=[{}] timestamp=[{}] user=[{}] content_length=[{}] content=[{}]", 
            Self::keyword(),
            self.channel,
            self.timestamp.duration_since(SystemTime::UNIX_EPOCH).expect("Expected time after 1970.").as_millis() as u64,
            self.user, self.content.len(), self.content)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let command = tag(NewMessageProtocol::keyword());

        let (input, (_, _, channel, _, timestamp, _, user, _, content_length, _)) =
        (command,
            tag(" "), protocol_field(b"channel="),
            tag(" "), protocol_field(b"timestamp="),
            tag(" "), protocol_field(b"user="),
            tag(" "), protocol_field(b"content_length="),
            tag(" ")).parse(string)?;

        // TODO: we assume content is always valid utf8

        let content_length = str::from_utf8(content_length).unwrap().parse::<usize>().expect("Expected valid usize content length.");

        let mut content_field = preceded(
            tag("content="),
            delimited(char('['), take(content_length), char(']'))
        );

        let (input, content) = content_field.parse(input)?;

        let channel = str::from_utf8(channel).unwrap().parse::<ChannelId>().expect("Expected valid ChannelId.");
        let timestamp = str::from_utf8(timestamp).unwrap().parse::<u64>().expect("Expected valid u64 timestamp.");
        let timestamp = SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(timestamp)).expect("Expected valid timestamp.");
        let user = str::from_utf8(user).unwrap().parse::<UserId>().expect("Expected valid UserId.");
        let content = str::from_utf8(content).unwrap().to_string();

        Ok((input, Self { channel, timestamp, user, content }))
    }
}

pub(crate) struct NewUserProtocol {
    pub(crate) username: String,
}
impl HasProtocolKeyword for NewUserProtocol { fn keyword() -> &'static str { "NewUser" } }
impl NetworkCodable for NewUserProtocol {

    fn encode(&self) -> String {
        format!("{} username=[{}]", Self::keyword(), self.username)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (input, (_, _, username)) = (
            tag(Self::keyword()), 
            tag(" "), protocol_field(b"username=")).parse(string)?;

        let username = str::from_utf8(username).unwrap().to_string();

        Ok((input, Self { username }))
    }
}

// ==============================
// => Send Data
// ==============================
pub(crate) enum SendDataProtocol {
    Message(Message),
    User(User),
    Channel(TextChannel),
}
impl HasProtocolKeyword for SendDataProtocol { fn keyword() -> &'static str { "SendData" } }
impl NetworkCodable for SendDataProtocol {

    fn encode(&self) -> String {
        todo!()
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        todo!()
    }
}

// ==============================
// => IdsProtocol
// ==============================
pub(crate) struct ChannelIdsProtocol(pub Vec<ChannelId>);
impl HasProtocolKeyword for ChannelIdsProtocol { fn keyword() -> &'static str { "Channel" }}
impl NetworkCodable for ChannelIdsProtocol {
    fn encode(&self) -> String {
        let mut msg = format!("{} {} [{}", Self::keyword(), self.0.len(), self.0[0]);
        for id in &self.0[1..] {
            msg.push_str(format!(" {}", id).as_str())
        }
        msg.push(']');
        msg
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (mut input, (_, _, len, _)) = (
            tag(Self::keyword()), tag(" "), 
            protocol_space_ending_word(), tag(" ")
        ).parse(string)?;
        let len = str::from_utf8(len).unwrap().parse::<usize>().expect("Expected valid length value.");

        let mut channels = Vec::new();

        let (input1, first) = (take_till(|c| c == b' ' || c == b']')).parse(input)?;
        let first = str::from_utf8(first).unwrap().parse::<ChannelId>().expect("Expected valid ChannelId.");
        channels.push(first);
        input = input1;

        while channels.len() < len {
            let (input1, (_, id)) = (tag(" "), take_till(|c| c == b' ' || c == b']')).parse(input)?;
            input = input1;
            let id = str::from_utf8(id).unwrap().parse::<ChannelId>().expect("Expected valid ChannelId.");
            channels.push(id);
        }
        Ok((input, Self(channels)))
    }
}

pub(crate) struct MessageIdsProtocol(pub ChannelId, pub Vec<MessageId>);
impl HasProtocolKeyword for MessageIdsProtocol { fn keyword() -> &'static str { "Message" } }
impl NetworkCodable for MessageIdsProtocol {
    fn encode(&self) -> String {
        let mut msg = format!("{} {} {} [{}", Self::keyword(), self.0, self.1.len(), self.1[0]);
        for id in &self.1[1..] {
            msg.push_str(format!(" {}", id).as_str())
        }
        msg.push(']');
        msg
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (mut input, (channel, _, len, _)) = (
            protocol_space_ending_word(), tag(" "),
            protocol_space_ending_word(), tag(" ")).parse(string)?;
        let channel = str::from_utf8(channel).unwrap().parse::<ChannelId>().expect("Expected valid ChannelId");
        let len = str::from_utf8(len).unwrap().parse::<usize>().expect("Expected valid length value.");

        let mut messages = Vec::new();

        let (input1, first) = (take_till(|c| c == b' ' || c == b']')).parse(input)?;
        let first = str::from_utf8(first).unwrap().parse::<MessageId>().expect("Expected valid MessageId.");
        messages.push(first);
        input = input1;

        while messages.len() < len {
            let (input1, (_, id)) = (tag(" "), take_till(|c| c == b' ' || c == b']')).parse(input)?;
            input = input1;
            let id = str::from_utf8(id).unwrap().parse::<MessageId>().expect("Expected valid MessageId.");
            messages.push(id);
        }
        Ok((input, Self(channel, messages)))
    }

}
pub(crate) struct UserIdsProtocol(pub Vec<UserId>);
impl HasProtocolKeyword for UserIdsProtocol { fn keyword() -> &'static str { "User" } }
impl NetworkCodable for UserIdsProtocol {
    fn encode(&self) -> String {
        let mut msg = format!("{} {} [{}", Self::keyword(), self.0.len(), self.0[0]);
        for id in &self.0[1..] {
            msg.push_str(format!(" {}", id).as_str())
        }
        msg.push(']');
        msg
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (mut input, (_, _, len, _)) = (
            tag(Self::keyword()), tag(" "),
            protocol_space_ending_word(), tag(" ")).parse(string)?;
        let len = str::from_utf8(len).unwrap().parse::<usize>().expect("Expected valid length value.");

        let mut users = Vec::new();

        let (input1, first) = (take_till(|c| c == b' ' || c == b']')).parse(input)?;
        let first = str::from_utf8(first).unwrap().parse::<UserId>().expect("Expected valid UserId.");
        users.push(first);
        input = input1;

        while users.len() < len {
            let (input1, (_, id)) = (tag(" "), take_till(|c| c == b' ' || c == b']')).parse(input)?;
            input = input1;
            let id = str::from_utf8(id).unwrap().parse::<UserId>().expect("Expected valid UserId.");
            users.push(id);
        }
        Ok((input, Self(users)))
    }
}


// ==============================
// => Register Data
// ==============================
pub(crate) enum RegisterDataProtocol {
    Message(MessageIdsProtocol),
    User(UserIdsProtocol),
    Channel(ChannelIdsProtocol),
}
impl HasProtocolKeyword for RegisterDataProtocol { fn keyword() -> &'static str { "RegisterData" } }
impl RegisterDataProtocol {
    fn messages(channel_id: ChannelId, messages: Vec<MessageId>) -> Option<Self> {
        if messages.is_empty() {
            return None;
        }
        Some(Self::Message(MessageIdsProtocol(channel_id, messages)))
    }
    fn users(users: Vec<UserId>) -> Option<Self> {
        if users.is_empty() {
            return None;
        }
        Some(Self::User(UserIdsProtocol(users)))
    }
    fn channels(channels: Vec<ChannelId>) -> Option<Self> {
        if channels.is_empty() {
            return None;
        }
        Some(Self::Channel(ChannelIdsProtocol(channels)))
    }
}
impl NetworkCodable for RegisterDataProtocol {
    fn encode(&self) -> String {
        let mut encoded = String::from(Self::keyword());
        encoded.push(' ');
        encoded.push_str(match self {
            Self::Message(protocol) => protocol.encode(),
            Self::User(protocol) => protocol.encode(),
            Self::Channel(protocol) => protocol.encode(),
        }.as_str());

        encoded
    }


    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (string, _) = (tag(Self::keyword()), tag(" ")).parse(string)?;

        let result = ChannelIdsProtocol::decode(string);
        if result.is_ok() {
            return  result.map(|(input, r)| (input, Self::Channel(r)));
        }
        let result = UserIdsProtocol::decode(string);
        if result.is_ok() {
            return  result.map(|(input, r)| (input, Self::User(r)));
        }
        let result = MessageIdsProtocol::decode(string);
        if result.is_ok() {
                return  result.map(|(input, r)| (input, Self::Message(r)));
        }

        Err(nom::Err::Failure(error::Error::new(string, ErrorKind::Fail)))
    }
}

// ==============================
// => Get Data
// ==============================
pub(crate) enum GetDataProtocol {
    Message(MessageIdsProtocol),
    User(UserIdsProtocol),
    Channel(ChannelIdsProtocol),
}
impl HasProtocolKeyword for GetDataProtocol { fn keyword() -> &'static str { "GetData" } }
impl GetDataProtocol {
    fn messages(channel_id: ChannelId, messages: Vec<MessageId>) -> Option<Self> {
        if messages.is_empty() {
            return None;
        }
        Some(Self::Message(MessageIdsProtocol(channel_id, messages)))
    }
    fn users(users: Vec<UserId>) -> Option<Self> {
        if users.is_empty() {
            return None;
        }
        Some(Self::User(UserIdsProtocol(users)))
    }
    fn channels(channels: Vec<ChannelId>) -> Option<Self> {
        if channels.is_empty() {
            return None;
        }
        Some(Self::Channel(ChannelIdsProtocol(channels)))
    }
}
impl NetworkCodable for GetDataProtocol {
    fn encode(&self) -> String {
        let mut encoded = String::from(Self::keyword());
        encoded.push(' ');
        encoded.push_str(match self {
            Self::Message(protocol) => protocol.encode(),
            Self::User(protocol) => protocol.encode(),
            Self::Channel(protocol) => protocol.encode(),
        }.as_str());

        encoded
    }


    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (string, _) = (tag(Self::keyword()), tag(" ")).parse(string)?;

        let result = ChannelIdsProtocol::decode(string);
        if result.is_ok() {
            return  result.map(|(input, r)| (input, Self::Channel(r)));
        }
        let result = UserIdsProtocol::decode(string);
        if result.is_ok() {
            return  result.map(|(input, r)| (input, Self::User(r)));
        }
        let result = MessageIdsProtocol::decode(string);
        if result.is_ok() {
                return  result.map(|(input, r)| (input, Self::Message(r)));
        }

        Err(nom::Err::Failure(error::Error::new(string, ErrorKind::Fail)))
    }
}


// ==============================
// => Delete Data
// ==============================
pub(crate) enum DeleteDataProtocol {
    Message(MessageIdsProtocol),
    User(UserIdsProtocol),
    Channel(ChannelIdsProtocol),
}
impl HasProtocolKeyword for DeleteDataProtocol { fn keyword() -> &'static str { "DeleteData" } }
impl NetworkCodable for DeleteDataProtocol {

    fn encode(&self) -> String {
        let mut encoded = String::from(Self::keyword());
        encoded.push(' ');
        encoded.push_str(match self {
            Self::Message(protocol) => protocol.encode(),
            Self::User(protocol) => protocol.encode(),
            Self::Channel(protocol) => protocol.encode(),
        }.as_str());

        encoded
    }


    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let (string, _) = (tag(Self::keyword()), tag(" ")).parse(string)?;

        let result = ChannelIdsProtocol::decode(string);
        if result.is_ok() {
            return  result.map(|(input, r)| (input, Self::Channel(r)));
        }
        let result = UserIdsProtocol::decode(string);
        if result.is_ok() {
            return  result.map(|(input, r)| (input, Self::User(r)));
        }
        let result = MessageIdsProtocol::decode(string);
        if result.is_ok() {
                return  result.map(|(input, r)| (input, Self::Message(r)));
        }

        Err(nom::Err::Failure(error::Error::new(string, ErrorKind::Fail)))
    }
}
