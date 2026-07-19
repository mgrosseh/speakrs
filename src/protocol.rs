use std::{io::Write, net::TcpStream, time::{Duration, SystemTime}};

use nom::{IResult, Parser, bytes::{complete::{tag, take_till, take_while}, take, take_while1}, character::complete::char, error::{self, ErrorKind}, sequence::{delimited, preceded}};

use crate::common::{ChannelId, Message, MessageId, NetworkCodable, PROTOCOL_END_CHAR, PROTOCOL_KEYWORD, ServerError, TextChannel, User, UserId};

// ==============================
// => Speakrs Protocol
// ==============================
pub(crate) enum Protocol {
    AddChannel(AddChannelProtocol),
    AddMessage(AddMessageProtocol),
    AddUser(AddUserProtocol),
    NewData(NewDataProtocol),
    SendData(SendDataProtocol),
    RegisterData(RegisterDataProtocol),
    GetData(GetDataProtocol),
    DeleteData(DeleteDataProtocol),
    ServerError(ServerError),
}
impl Protocol {
    pub(crate) fn create_channel(name: String, desc: String) -> Self {
        Protocol::AddChannel(AddChannelProtocol { name, desc })
    }
    pub(crate) fn send_message(channel: ChannelId, timestamp: SystemTime, user: UserId, content: String) -> Self {
        Protocol::AddMessage(AddMessageProtocol { channel, timestamp, user, content } )
    }
    pub(crate) fn add_user(username: String) -> Self {
        Protocol::AddUser(AddUserProtocol { username })
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
    fn matches(string: &[u8]) -> bool {
        AddChannelProtocol::matches(string) || AddMessageProtocol::matches(string)
    }

    fn encode(&self) -> String {
        match self {
            Self::AddChannel(cmd) => cmd.encode(),
            Self::AddMessage(cmd) => cmd.encode(),
            Self::AddUser(cmd) => cmd.encode(),
            Self::NewData(cmd) => cmd.encode(),
            Self::SendData(cmd) => cmd.encode(),
            Self::RegisterData(cmd) => cmd.encode(),
            Self::GetData(cmd) => cmd.encode(),
            Self::DeleteData(cmd) => cmd.encode(),
            Self::ServerError(cmd) => todo!(),
        }
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        if AddChannelProtocol::matches(string) {
            return AddChannelProtocol::decode(string).map(|(input, command)| (input, Self::AddChannel(command)));
        } 
        if AddMessageProtocol::matches(string) {
            return AddMessageProtocol::decode(string).map(|(input, command)| (input, Self::AddMessage(command)));
        }
        if AddUserProtocol::matches(string) {
            return AddUserProtocol::decode(string).map(|(input, command)| (input, Self::AddUser(command)));
        }
        if RegisterDataProtocol::matches(string) {
            return RegisterDataProtocol::decode(string).map(|(input, command)| (input, Self::RegisterData(command)));
        }
        if GetDataProtocol::matches(string) {
            return GetDataProtocol::decode(string).map(|(input, command)| (input, Self::GetData(command)));
        }
        if DeleteDataProtocol::matches(string) {
            return DeleteDataProtocol::decode(string).map(|(input, command)| (input, Self::DeleteData(command)));
        }

        Err(nom::Err::Failure(error::Error::new(string, ErrorKind::Fail)))
    }
}

trait HasProtocolKeyword {
    fn keyword() -> &'static str;
}


// ==============================
// => New Data
// ==============================
pub(crate) enum NewDataProtocol {
    Message(AddMessageProtocol),
    User(AddUserProtocol),
    Channel(AddChannelProtocol),
}
impl HasProtocolKeyword for NewDataProtocol { fn keyword() -> &'static str { "NewData" } }
impl NetworkCodable for NewDataProtocol {
    fn matches(string: &[u8]) -> bool {
        todo!()
    }

    fn encode(&self) -> String {
        todo!()
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        todo!()
    }
}

pub(crate) struct AddChannelProtocol {
    pub name: String,
    pub desc: String,
}
impl HasProtocolKeyword for AddChannelProtocol { fn keyword() -> &'static str { "AddChannel" } }
impl NetworkCodable for AddChannelProtocol {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, AddChannelProtocol::keyword()).as_bytes())
    }

    fn encode(&self) -> String {
        format!("{} {} name=[{}] desc=[{}]", PROTOCOL_KEYWORD, AddChannelProtocol::keyword(), self.name, self.desc)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag(AddChannelProtocol::keyword());
        let name_field = preceded(
            tag("name="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );
        let desc_field = preceded(
            tag("desc="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );

        let (input, (_, _, _, _, name, _, desc)) =
        (protocol, take_while1(|c| c == b' '), command, take_while1(|c| c == b' '), name_field, take_while1(|c| c == b' '), desc_field).parse(string)?;

        let name = str::from_utf8(name).unwrap().to_string();
        let desc = str::from_utf8(desc).unwrap().to_string();

        Ok((input, Self { name, desc }))
    }
}

pub(crate) struct AddMessageProtocol {
    pub channel: ChannelId,
    pub timestamp: SystemTime,
    pub user: UserId,
    pub content: String,
}
impl HasProtocolKeyword for AddMessageProtocol { fn keyword() -> &'static str { "AddMessage" } }
impl NetworkCodable for AddMessageProtocol {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, AddMessageProtocol::keyword()).as_bytes())
    }

    fn encode(&self) -> String {
        format!("{} {} channel=[{}] timestamp=[{}] user=[{}] content_length=[{}] content=[{}]", 
            PROTOCOL_KEYWORD, 
            AddMessageProtocol::keyword(),
            self.channel,
            self.timestamp.duration_since(SystemTime::UNIX_EPOCH).expect("Expected time after 1970.").as_millis() as u64,
            self.user, self.content.len(), self.content)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag(AddMessageProtocol::keyword());
        let channel_field = preceded(
            tag("channel="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );
        let timestamp_field = preceded(
            tag("timestamp="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );
        let user_field = preceded(
            tag("user="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );
        let content_length_field = preceded(
            tag("content_length="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );

        let (input, (_, _, _, _, channel, _, timestamp, _, user, _, content_length, _)) =
        (protocol, 
            take_while1(|c| c == b' '), command,
            take_while1(|c| c == b' '), channel_field,
            take_while1(|c| c == b' '), timestamp_field,
            take_while1(|c| c == b' '), user_field,
            take_while1(|c| c == b' '), content_length_field,
            take_while1(|c| c == b' ')).parse(string)?;

        // we assume content is always valid utf8

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

pub(crate) struct AddUserProtocol {
    pub(crate) username: String,
}
impl HasProtocolKeyword for AddUserProtocol { fn keyword() -> &'static str { "AddUser" } }
impl NetworkCodable for AddUserProtocol {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, AddUserProtocol::keyword()).as_bytes())
    }

    fn encode(&self) -> String {
        format!("{} {} username=[{}]", PROTOCOL_KEYWORD, AddUserProtocol::keyword(), self.username)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag(AddUserProtocol::keyword());
        let name_field = preceded(
            tag("username="),
            delimited(char('['), take_while(|c| c != b']'), char(']'))
        );

        let (input, (_, _, _, _, username)) =
        (protocol, take_while1(|c| c == b' '), command, take_while1(|c| c == b' '), name_field).parse(string)?;

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
    fn matches(string: &[u8]) -> bool {
        todo!()
    }

    fn encode(&self) -> String {
        todo!()
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        todo!()
    }
}



// ==============================
// => Register Data
// ==============================
pub(crate) enum RegisterDataProtocol {
    Message(ChannelId, Vec<MessageId>),
    User(Vec<UserId>),
    Channel(Vec<ChannelId>),
}
impl HasProtocolKeyword for RegisterDataProtocol { fn keyword() -> &'static str { "RegisterData" } }
impl RegisterDataProtocol {
    fn messages(channel_id: ChannelId, messages: Vec<MessageId>) -> Option<Self> {
        if messages.is_empty() {
            return None;
        }
        Some(RegisterDataProtocol::Message(channel_id, messages))
    }
    fn users(users: Vec<UserId>) -> Option<Self> {
        if users.is_empty() {
            return None;
        }
        Some(RegisterDataProtocol::User(users))
    }
    fn channels(channels: Vec<ChannelId>) -> Option<Self> {
        if channels.is_empty() {
            return None;
        }
        Some(RegisterDataProtocol::Channel(channels))
    }
}
impl NetworkCodable for RegisterDataProtocol {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, RegisterDataProtocol::keyword()).as_bytes())
    }

    fn encode(&self) -> String {
        match self {
            RegisterDataProtocol::Message(channel_id, messages) => {
                let mut msg = format!("{} {} Message {} {} [{}", PROTOCOL_KEYWORD, RegisterDataProtocol::keyword(), channel_id, messages.len(), messages[0]);
                for id in &messages[1..] {
                    msg.push_str(format!(" {}", id).as_str())
                }
                msg.push(']');
                msg
            },
            RegisterDataProtocol::User(users) => {
                let mut msg = format!("{} {} User {} [{}", PROTOCOL_KEYWORD, RegisterDataProtocol::keyword(), users.len(), users[0]);
                for id in &users[1..] {
                    msg.push_str(format!(" {}", id).as_str())
                }
                msg.push(']');
                msg
            },
            RegisterDataProtocol::Channel(channels) => {
                let mut msg = format!("{} {} Channel {} [{}", PROTOCOL_KEYWORD, RegisterDataProtocol::keyword(), channels.len(), channels[0]);
                for id in &channels[1..] {
                    msg.push_str(format!(" {}", id).as_str())
                }
                msg.push(']');
                msg
            },
        }
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag(RegisterDataProtocol::keyword());
        let subtype = take_till(|c| c == b' ');

        let (input, (_, _, _, _, subtype, _)) =
        (protocol, 
            tag(" "), command, 
            tag(" "), subtype,
            tag(" ")).parse(string)?;

        match subtype {
            b"User" => {
                let (mut input, (len, _)) = (take_till(|c| c == b' '), tag(" ")).parse(input)?;
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
                Ok((input, Self::User(users)))
            },
            b"Channel" => {
                let (mut input, (len, _)) = (take_till(|c| c == b' '), tag(" ")).parse(input)?;
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
                Ok((input, Self::Channel(channels)))
            },
            b"Message" => {
                let (mut input, (channel, _, len, _)) = (
                    take_till(|c| c == b' '), tag(" "),
                    take_till(|c| c == b' '), tag(" ")).parse(input)?;
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
                Ok((input, Self::Message(channel, messages)))
            },
            x => {
                println!("WARNING: Encountered unexpected word during decode of RegisterDataProtocol. This could indicate problems. (`{}`)", str::from_utf8(x).unwrap_or("<utf8-error>"));
                Err(nom::Err::Failure(error::Error::new(input, ErrorKind::Fail)))
            },
        }
    }
}


// ==============================
// => Get Data
// ==============================
pub(crate) enum GetDataProtocol {
    Message(ChannelId, Vec<MessageId>),
    User(Vec<UserId>),
    Channel(Vec<ChannelId>),
}
impl HasProtocolKeyword for GetDataProtocol { fn keyword() -> &'static str { "GetData" } }
impl GetDataProtocol {
    fn messages(channel_id: ChannelId, messages: Vec<MessageId>) -> Option<Self> {
        if messages.is_empty() {
            return None;
        }
        Some(Self::Message(channel_id, messages))
    }
    fn users(users: Vec<UserId>) -> Option<Self> {
        if users.is_empty() {
            return None;
        }
        Some(Self::User(users))
    }
    fn channels(channels: Vec<ChannelId>) -> Option<Self> {
        if channels.is_empty() {
            return None;
        }
        Some(Self::Channel(channels))
    }
}
impl NetworkCodable for GetDataProtocol {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, RegisterDataProtocol::keyword()).as_bytes())
    }

    fn encode(&self) -> String {
        match self {
            GetDataProtocol::Message(channel_id, messages) => {
                let mut msg = format!("{} {} Message {} {} [{}", PROTOCOL_KEYWORD, GetDataProtocol::keyword(), channel_id, messages.len(), messages[0]);
                for id in &messages[1..] {
                    msg.push_str(format!(" {}", id).as_str())
                }
                msg.push(']');
                msg
            },
            GetDataProtocol::User(users) => {
                let mut msg = format!("{} {} User {} [{}", PROTOCOL_KEYWORD, GetDataProtocol::keyword(), users.len(), users[0]);
                for id in &users[1..] {
                    msg.push_str(format!(" {}", id).as_str())
                }
                msg.push(']');
                msg
            },
            GetDataProtocol::Channel(channels) => {
                let mut msg = format!("{} {} Channel {} [{}", PROTOCOL_KEYWORD, GetDataProtocol::keyword(), channels.len(), channels[0]);
                for id in &channels[1..] {
                    msg.push_str(format!(" {}", id).as_str())
                }
                msg.push(']');
                msg
            },
        }
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag(GetDataProtocol::keyword());
        let subtype = take_till(|c| c == b' ');

        let (input, (_, _, _, _, subtype, _)) =
        (protocol, 
            tag(" "), command, 
            tag(" "), subtype,
            tag(" ")).parse(string)?;

        match subtype {
            b"User" => {
                let (mut input, (len, _)) = (take_till(|c| c == b' '), tag(" ")).parse(input)?;
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
                Ok((input, Self::User(users)))
            },
            b"Channel" => {
                let (mut input, (len, _)) = (take_till(|c| c == b' '), tag(" ")).parse(input)?;
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
                Ok((input, Self::Channel(channels)))
            },
            b"Message" => {
                let (mut input, (channel, _, len, _)) = (
                    take_till(|c| c == b' '), tag(" "),
                    take_till(|c| c == b' '), tag(" ")).parse(input)?;
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
                Ok((input, Self::Message(channel, messages)))
            },
            x => {
                println!("WARNING: Encountered unexpected word during decode of GetDataProtocol. This could indicate problems. (`{}`)", str::from_utf8(x).unwrap_or("<utf8-error>"));
                Err(nom::Err::Failure(error::Error::new(input, ErrorKind::Fail)))
            },
        }
    }
}


// ==============================
// => Delete Data
// ==============================
pub(crate) enum DeleteDataProtocol {
    Message(ChannelId, MessageId),
    User(UserId),
    Channel(ChannelId),
}
impl HasProtocolKeyword for DeleteDataProtocol { fn keyword() -> &'static str { "DeleteData" } }
impl NetworkCodable for DeleteDataProtocol {
    fn matches(string: &[u8]) -> bool {
        todo!()
    }

    fn encode(&self) -> String {
        todo!()
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        todo!()
    }
}
