use core::fmt;
use std::{collections::BTreeMap, error::Error, fmt::Display, net::{IpAddr, Ipv4Addr}, num::ParseIntError, str::FromStr, sync::{Arc, Mutex, mpsc}, thread, time::{Duration, SystemTime}};

use nom::{IResult, Parser, bytes::{complete::{tag, take_while}, take, take_while1}, character::complete::char, error::{ErrorKind, ParseError}, sequence::{delimited, preceded}};

pub const VERSION: &str = "v0.1";
pub const PROG: &str = "speakrs";

pub const PROTOCOL_KEYWORD: &str = "SPEAKRS/0.1";
pub const PROTOCOL_END_CHAR: char = '\x1C'; // File Separator character


// ======================================
// => Run Arguments
// ======================================

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Client,
    Server,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Client => write!(f, "client"),
            Mode::Server => write!(f, "server"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Port {
    port: u16,
}

impl Port {
    /// Create a Port from given string
    ///
    /// # Panics
    /// 
    /// The `from_str` will panic if string is not a valid u16 indicating the port
    #[allow(unused)]
    pub fn from_str(string: &str) -> Self {
        let port = string.parse::<u16>().expect("Expecting valid u8.");
        Self { port }
    }

    pub fn new(port: u16) -> Self {
        Self { port }
    }
}
impl Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.port)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Arguments {
    pub mode: Mode,
    pub server_ip: IpAddr,
    pub server_tcp_port: Port,
    pub server_udp_port: Port,
    pub verbose: bool,
    pub quiet: bool,
}


impl Arguments {
    pub fn parse(args: &[String]) -> Option<Self> {
        let mut mode: Option<Mode> = Option::None;
        let mut server_ip: Option<IpAddr> = Option::None;
        let mut server_tcp_port: Option<Port> = Option::None;
        let mut server_udp_port: Option<Port> = Option::None;
        let mut verbose: Option<bool> = Option::None;
        let mut quiet: Option<bool> = Option::None;
        // TODO: use proper library for parsing cmdargs
        for arg in args {
            match arg.as_str() {
                "--help" | "-h" | "help" => {
                    help(&mode);
                    return Option::None;
                },
                "--version" => {
                    println!("{VERSION}");
                    return Option::None;
                },
                "server" => {
                    if let Some(mode) = mode {
                        err_duplicate_mode(&mode, &Mode::Server);
                        return Option::None;
                    }
                    mode = Option::Some(Mode::Server)
                },

                "client" => mode = Option::Some(Mode::Client),
                "--quiet" | "-q" => quiet = Option::Some(true),
                "--verbose" | "-v" => verbose = Option::Some(true),
                x if x.starts_with("--udp=") => server_udp_port = Option::Some(Port::from_str(x.strip_prefix("--udp=").unwrap())),
                x if x.starts_with("--tcp=") => server_tcp_port = Option::Some(Port::from_str(x.strip_prefix("--tcp=").unwrap())),
                x if x.starts_with("--ip=") => server_ip = Option::Some(x.strip_prefix("--ip=").unwrap().parse().expect("")),
                x if x.starts_with("-") => {
                    err_unknown_arg(x);
                    return Option::None;
                },
                x => {
                    err_unknown_command(x);
                    return Option::None;
                },
            }
        }

        Option::Some(Self {
            mode: mode.unwrap_or(Mode::Client),
            server_ip: server_ip.unwrap_or(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))),
            server_tcp_port: server_tcp_port.unwrap_or(Port::new(7878)),
            server_udp_port: server_udp_port.unwrap_or(Port::new(7879)),
            verbose: verbose.unwrap_or(false),
            quiet: quiet.unwrap_or(false),
        })
    }

}

fn err_duplicate_mode(mode: &Mode, other: &Mode) {
    println!("{}: duplicate CMD (mode). Tried `{}` while already in `{}` mode. See `{} --help`", PROG, mode, other, PROG);
}

fn err_unknown_command(cmd: &str) {
    println!("{}: unknown command `{}`. See `{} --help`", PROG, cmd, PROG);
}

fn err_unknown_arg(arg: &str) {
    println!("{}: unsupported argument `{}`. See `{} --help`", PROG, arg, PROG);
}

fn help(mode: &Option<Mode>) {
    match mode {
        Some(Mode::Client) => {
            println!("{} client: starts in client mode (use `{} --help` for help)", PROG, PROG)
        },
        Some(Mode::Server) => {
            println!("{} server: starts in server mode (use `{} --help` for help)", PROG, PROG)
        },
        None => {
            println!("{} ARGS CMD", PROG);
            println!("  where ARGS: ");
            println!("    --help | -h");
            println!("      prints this help");
            println!("    --verbose | -v");
            println!("      be more verbose");
            println!("    --ip=IP");
            println!("      set ip to IP in cannonical string format");
            println!("    --tcp=PORT");
            println!("      sets tcp port. PORT must be a 16 bit unsigned number");
            println!("    --udp=PORT");
            println!("      sets udp port. PORT must be a 16 bit unsigned number");
            println!("  ");
            println!("  where CMD (only one): ");
            println!("    server: starts in server mode");
            println!("    client: starts in client mode");
        }
    }
}

// ======================================
// => Thread Pool
// ======================================

pub(crate) type Job = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct Worker {
    #[allow(unused)]
    id: usize,
    #[allow(unused)]
    thread: thread::JoinHandle<()>,
}
impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>, parent_name: &str, verbose: bool) -> Self {
        let parent_name = parent_name.to_string();
        let thread = thread::Builder::new().spawn(move || {
            loop {
                let message = receiver.lock()
                    .unwrap_or_else(|_| panic!("ThreadPool \"{parent_name}\": Worker {id} could not acquire lock on receiver."))
                    .recv(); // blocks if no job yet
                match message {
                    Ok(job) => {
                        if verbose {
                            println!("ThreadPool \"{parent_name}\": Worker {id} got a job; executing.");
                        }
                        job();
                    }
                    Err(_) => {
                        if verbose {
                            println!("ThreadPool \"{parent_name}\": Worker {id} disconnected; shutting down.");
                        }
                        break;
                    }
                }
            }
        }).unwrap(); // NOTE: consider error handling of thread builder
        Self { id, thread }
    }
}

pub(crate) struct ThreadPool {
    name: String,
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
    verbose: bool,
}


impl ThreadPool {
    /// Create a new ThreadPool
    /// 
    /// `size` is the number of threads in the pool
    ///
    /// # Panics
    ///
    /// The `new` function will panic if size is zero
    #[allow(unused)]
    pub fn new(size: usize, args: Arguments) -> Self {
        Self::with_name(size, "ThreadPool", args)
    }

    /// Create a new ThreadPool with given name
    /// 
    /// `size` is the number of threads in the pool
    ///
    /// # Panics
    ///
    /// The `with_name` function will panic if size is zero
    pub fn with_name(size: usize, name: &str, args: Arguments) -> Self {
        assert!(size > 0);
        let mut workers = Vec::with_capacity(size);

        let (sender, receiver) = mpsc::channel();

        let receiver = Arc::new(Mutex::new(receiver));


        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver), name, args.verbose));
        }

        let sender = Option::Some(sender);
        let name = name.to_string();
        
        Self {
            name, workers, sender, verbose: args.verbose,
        }
    }

    pub(crate) fn execute<F>(&self, f: F) where F: FnOnce() + Send + 'static {
        let job = Box::new(f);
        self.sender.as_ref().unwrap()
            // sender being None indicates ThreadPool has been dropped, execute should not be possible to call
            .send(job).unwrap();
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers.drain(..) {
            if self.verbose {
                println!("ThreadPool \"{}\": Shutting down worker {}", self.name, worker.id);
            }
            worker.thread.join().unwrap(); // NOTE: if unwrap panics, while drop is called in panic, all other cleanup is skipped (bad)
        }
    }
}

// ======================================
// => server struct
// ======================================

#[derive(Debug)]
pub(crate) enum ServerError {
    NoSuchMessage(MessageId),
    NoSuchChannel(ChannelId),
    NoSuchUser(UserId),
}
impl Error for ServerError {
}
impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSuchMessage(id) => write!(f, "Error: Message with requested id does not exist: `{}`", id),
            Self::NoSuchChannel(id) => write!(f, "Error: Channel with requested id does not exist: `{}`", id),
            Self::NoSuchUser(id) => write!(f, "Error: User with requested id does not exist: `{}`", id),
        }
    }
}

pub(crate) type ServerResult<T> = Result<T, ServerError>;

pub(crate) struct Server {
    channels: BTreeMap<ChannelId, TextChannel>,
    users: BTreeMap<UserId, User>,
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

    fn load() -> Self {
        todo!()
    }

    // ==> Users
    fn new_user_id(&self) -> UserId {
        self.users.keys().max().map(|k| k.next()).unwrap_or_default()
    }

    pub(crate) fn load_user(&mut self, id: UserId, name: String) -> ServerResult<UserId> {
        let user = User::new(id, name);
        self.users.insert(id, user);
        Ok(id)
    }

    pub(crate) fn add_user(&mut self, name: String) -> ServerResult<UserId> {
        let id = self.new_user_id();
        self.load_user(id, name)
    }

    pub(crate) fn get_user(&self, id: UserId) -> Option<&User> {
        self.users.get(&id)
    }

    // ==> Channels
    fn new_channel_id(&self) -> ChannelId {
        self.channels.keys().max().map(|k| k.next()).unwrap_or_default()
    }

    pub(crate) fn load_channel(&mut self, id: ChannelId, name: String, desc: String) -> ServerResult<ChannelId> {
        let channel = TextChannel::new(id, name, desc);
        self.channels.insert(id, channel);
        // TODO: check if exists
        Ok(id)
    }

    pub(crate) fn add_channel(&mut self, name: String, desc: String) -> ServerResult<ChannelId> {
        let id = self.new_channel_id();
        self.load_channel(id, name, desc)
    }

    pub(crate) fn get_channel(&self, id: ChannelId) -> Option<&TextChannel> {
        self.channels.get(&id)
    }
    // TODO: consider if needed
    // pub(crate) fn get_channel_mut(&mut self, id: ChannelId) -> Option<&mut TextChannel> {
    //     self.channels.get_mut(&id)
    // }

    // ==> Messages
    pub(crate) fn load_message(&mut self, channel_id: ChannelId, message_id: MessageId, timestamp: SystemTime, user_id: UserId, content: String) -> ServerResult<MessageId> {
        if self.get_user(user_id).is_none() {
            return Err(ServerError::NoSuchUser(user_id));
        }
        let channel = self.channels.get_mut(&channel_id);
        if channel.is_none() {
            return Err(ServerError::NoSuchChannel(channel_id));
        }
        let channel = channel.unwrap();

        channel.load_message(message_id, timestamp, user_id, content)
    }

    pub(crate) fn send_message(&mut self, channel_id: ChannelId, timestamp: SystemTime, user_id: UserId, content: String) -> ServerResult<MessageId> {
        if self.get_user(user_id).is_none() {
            return Err(ServerError::NoSuchUser(user_id));
        }
        let channel = self.channels.get_mut(&channel_id);
        if channel.is_none() {
            return Err(ServerError::NoSuchChannel(channel_id));
        }
        let channel = channel.unwrap();

        channel.add_message(Some(timestamp), user_id, content)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ChannelId {
    id: u8,
}
impl ChannelId {
    fn from(id: u8) -> Self {
        Self {
            id
        }
    }
    fn next(self) -> Self {
        Self::from(self.id + 1)
    }
}
impl Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}
impl FromStr for ChannelId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = s.parse::<u8>()?;
        Ok(Self {
            id
        })
    }
}
pub struct TextChannel {
    id: ChannelId,
    name: String,
    desc: String,
    messages: BTreeMap<MessageId, Message>,
}

impl TextChannel {
    fn new(id: ChannelId, name: String, desc: String) -> Self {
        let messages = BTreeMap::new();

        Self {
            id,
            name,
            desc,
            messages
        }
    }

    fn new_message_id(&self) -> MessageId {
        self.messages.keys().max().map(|k| k.next()).unwrap_or_default()
    }

    fn load_message(&mut self, id: MessageId, timestamp: SystemTime, user: UserId, content: String) -> ServerResult<MessageId> {
        let message = Message::new(id, timestamp, user, content);
        self.messages.insert(id, message);
        Ok(id)
    }

    fn add_message(&mut self, timestamp: Option<SystemTime>, user: UserId, content: String) -> ServerResult<MessageId> {
        let id = self.new_message_id();
        let timestamp = timestamp.unwrap_or(SystemTime::now());
        self.load_message(id, timestamp, user, content)
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct UserId {
    id: u8,
}
impl UserId {
    fn from(id: u8) -> Self {
        Self {
            id
        }
    }
    fn next(self) -> Self {
        Self::from(self.id + 1)
    }
}
impl Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}
impl FromStr for UserId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = s.parse::<u8>()?;
        Ok(Self {
            id
        })
    }
}
pub struct User {
    id: UserId,
    name: String,
}
impl User {
    fn new(id: UserId, name: String) -> Self {
        Self { id, name }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct MessageId {
    id: u32,
}
impl MessageId {
    fn from(id: u32) -> Self {
        Self {
            id
        }
    }
    fn next(self) -> Self {
        Self::from(self.id + 1)
    }
}
impl Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}
impl FromStr for MessageId {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = s.parse::<u32>()?;
        Ok(Self {
            id
        })
    }
}
pub struct Message {
    id: MessageId,
    timestamp: SystemTime,
    user: UserId,
    content: String,
}
impl Message {
    fn new(id: MessageId, timestamp: SystemTime, user: UserId, content: String) -> Self {
        Self {
            id,
            timestamp,
            user,
            content
        }       
    }
}

// ==============================
// => Network Codable
// ==============================
pub(crate) trait NetworkCodable {
    fn matches(string: &[u8]) -> bool;
    fn encode(self) -> String;
    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized;
}
#[derive(Debug, PartialEq)]
pub(crate) enum DecodeError<I> {
  DecodeError,
  Nom(I, ErrorKind),
}

impl<I> ParseError<I> for DecodeError<I> {
  fn from_error_kind(input: I, kind: ErrorKind) -> Self {
    DecodeError::Nom(input, kind)
  }

  fn append(_: I, _: ErrorKind, other: Self) -> Self {
    other
  }
}

// ==============================
// => Speakrs Commands // TODO: move into protocol.rs
// ==============================
pub(crate) enum Protocol {
    CreateChannelRequest(CreateChannelCommand),
    SendMessage(SendMessageCommand),
    GetMessage(GetMessageCommand),
    DeleteMessageRequest(DeleteMessageCommand),
    AddUser(AddUserCommand),
}
impl Protocol {
    pub(crate) fn create_channel(name: String, desc: String) -> Self {
        Protocol::CreateChannelRequest(CreateChannelCommand { name, desc })
    }
    pub(crate) fn send_message(channel: ChannelId, timestamp: SystemTime, user: UserId, content: String) -> Self {
        Protocol::SendMessage(SendMessageCommand { channel, timestamp, user, content } )
    }
}

impl NetworkCodable for Protocol {
    fn matches(string: &[u8]) -> bool {
        CreateChannelCommand::matches(string) || SendMessageCommand::matches(string)
    }

    fn encode(self) -> String {
        match self {
            Self::CreateChannelRequest(cmd) => cmd.encode(),
            Self::SendMessage(cmd) => cmd.encode(),
            Self::GetMessage(_get_message_command) => todo!(),
            Self::DeleteMessageRequest(_delete_message_command) => todo!(),
            Self::AddUser(cmd) => cmd.encode(),
        }
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        if CreateChannelCommand::matches(string) {
            return CreateChannelCommand::decode(string).map(|(input, command)| (input, Self::CreateChannelRequest(command)));
        } 
        if SendMessageCommand::matches(string) {
            return SendMessageCommand::decode(string).map(|(input, command)| (input, Self::SendMessage(command)));
        }
        AddUserCommand::decode(string).map(|(input, command)| (input, Self::AddUser(command)))
    }
}


pub(crate) struct CreateChannelCommand {
    pub name: String,
    pub desc: String,
}
impl NetworkCodable for CreateChannelCommand {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, "CreateChannelCommand").as_bytes())
    }

    fn encode(self) -> String {
        format!("{} CreateChannelCommand name=[{}] desc=[{}]", PROTOCOL_KEYWORD, self.name, self.desc)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag("CreateChannelCommand");
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

pub(crate) struct SendMessageCommand {
    pub channel: ChannelId,
    pub timestamp: SystemTime,
    pub user: UserId,
    pub content: String,
}
impl NetworkCodable for SendMessageCommand {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, "SendMessageCommand").as_bytes())
    }

    fn encode(self) -> String {
        format!("{} SendMessageCommand channel=[{}] timestamp=[{}] user=[{}] content_length=[{}] content=[{}]", 
            PROTOCOL_KEYWORD, 
            self.channel,
            self.timestamp.duration_since(SystemTime::UNIX_EPOCH).expect("Expected time after 1970.").as_millis() as u64,
            self.user, self.content.len(), self.content)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag("SendMessageCommand");
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
pub(crate) struct GetMessageCommand {

}
pub(crate) struct DeleteMessageCommand {

}

pub(crate) struct AddUserCommand {
    pub(crate) username: String,
}
impl NetworkCodable for AddUserCommand {
    fn matches(string: &[u8]) -> bool {
        string.starts_with(format!("{} {}", PROTOCOL_KEYWORD, "SendMessageCommand").as_bytes())
    }

    fn encode(self) -> String {
        format!("{} AddUser username=[{}]", PROTOCOL_KEYWORD, self.username)
    }

    fn decode(string: &[u8]) -> IResult<&[u8], Self> where Self: Sized {
        let protocol = tag(PROTOCOL_KEYWORD);
        let command = tag("AddUser");
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
