use core::fmt;
use std::{collections::BTreeMap, error::Error, fmt::Display, io::Write, net::{IpAddr, Ipv4Addr, TcpStream}, num::ParseIntError, str::FromStr, sync::{Arc, Mutex, mpsc}, thread, time::{Duration, SystemTime}};

use nom::{Err, IResult, Parser, bytes::{complete::{tag, take_till, take_while}, take, take_while1}, character::complete::char, error::{self, ErrorKind, ParseError}, sequence::{delimited, preceded}};

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

    // ==> Users
    fn new_user_id(&self) -> UserId {
        self.users.keys().max().map(|k| k.next()).unwrap_or_default()
    }

    pub(crate) fn register_user(&mut self, id: UserId) -> ServerResult<UserId> {
        self.users.insert(id, None); // TODO: handle if key exists
        Ok(id)
    }

    pub(crate) fn load_user(&mut self, id: UserId, name: String) -> ServerResult<UserId> {
        let user = User::new(id, name); // TODO: handle if key exists
        self.users.insert(id, Some(user));
        Ok(id)
    }

    pub(crate) fn add_user(&mut self, name: String) -> ServerResult<UserId> {
        let id = self.new_user_id();
        self.load_user(id, name)
    }

    pub(crate) fn get_user(&self, id: UserId) -> Option<&User> {
        match self.users.get(&id) {
            None => None,
            Some(x) => x.as_ref()
        }
    }

    // ==> Channels
    fn new_channel_id(&self) -> ChannelId {
        self.channels.keys().max().map(|k| k.next()).unwrap_or_default()
    }

    pub(crate) fn register_channel(&mut self, id: ChannelId) -> ServerResult<ChannelId> {
        self.channels.insert(id, None); // TODO: handle if key exists
        Ok(id)
    }

    pub(crate) fn load_channel(&mut self, id: ChannelId, name: String, desc: String) -> ServerResult<ChannelId> {
        let channel = TextChannel::new(id, name, desc);
        self.channels.insert(id, Some(channel)); // TODO: handle if exists
        Ok(id)
    }

    pub(crate) fn add_channel(&mut self, name: String, desc: String) -> ServerResult<ChannelId> {
        let id = self.new_channel_id();
        self.load_channel(id, name, desc)
    }

    pub(crate) fn get_channel(&self, id: ChannelId) -> Option<&TextChannel> {
        match self.channels.get(&id) {
            None => None,
            Some(x) => x.as_ref()
        }
    }

    // ==> Messages
    fn checked_get_channel(&mut self, id: ChannelId) -> ServerResult<&mut TextChannel> {
        let channel = self.channels.get_mut(&id);
        if channel.is_none() {
            return Err(ServerError::NoSuchChannel(id));
        }
        let channel = channel.unwrap();
        if channel.is_none() {
            return Err(ServerError::ChannelNotLoaded(id));
        }
        Ok(channel.as_mut().unwrap())
    }

    pub(crate) fn register_message(&mut self, channel_id: ChannelId, message_id: MessageId) -> ServerResult<MessageId> {
        let channel = self.checked_get_channel(channel_id)?;
        channel.register_message(message_id)
    }

    pub(crate) fn load_message(&mut self, channel_id: ChannelId, message_id: MessageId, timestamp: SystemTime, user_id: UserId, content: String) -> ServerResult<MessageId> {
        if self.get_user(user_id).is_none() {
            return Err(ServerError::NoSuchUser(user_id));
        }
        let channel = self.checked_get_channel(channel_id)?;

        channel.load_message(message_id, timestamp, user_id, content)
    }

    pub(crate) fn send_message(&mut self, channel_id: ChannelId, timestamp: SystemTime, user_id: UserId, content: String) -> ServerResult<MessageId> {
        if self.get_user(user_id).is_none() {
            return Err(ServerError::NoSuchUser(user_id));
        }
        let channel = self.checked_get_channel(channel_id)?;
        channel.add_message(Some(timestamp), user_id, content)
    }

    pub(crate) fn get_messages(&self, channel_id: ChannelId, messages: &[MessageId]) -> ServerResult<Vec<&Message>> {
        let channel = self.get_channel(channel_id);
        if channel.is_none() {
            return Err(ServerError::NoSuchChannel(channel_id));
        }
        let channel = channel.unwrap();
        let mut vec = Vec::new();

        for &id in messages {
            let message = channel.get_message(id);
            if message.is_none() {
                return Err(ServerError::NoSuchMessage(id));
            }
            let message = message.unwrap();
            vec.push(message);
        }

        Ok(vec)
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
    messages: BTreeMap<MessageId, Option<Message>>,
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
        self.messages.insert(id, Some(message)); // TODO: handle if key exists
        Ok(id)
    }

    fn add_message(&mut self, timestamp: Option<SystemTime>, user: UserId, content: String) -> ServerResult<MessageId> {
        let id = self.new_message_id();
        let timestamp = timestamp.unwrap_or(SystemTime::now());
        self.load_message(id, timestamp, user, content)
    }

    fn register_message(&mut self, id: MessageId) -> ServerResult<MessageId> {
        self.messages.insert(id, None); // TODO: handle if key exists
        Ok(id)
    }

    fn get_message(&self, id: MessageId) -> Option<&Message> {
        match self.messages.get(&id) {
            None => None,
            Some(x) => x.as_ref()
        }
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
    fn encode(&self) -> String;
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
// => Speakrs Protocol // TODO: move into protocol.rs
// ==============================
pub(crate) enum Protocol {
    AddChannel(AddChannelProtocol),
    AddMessage(AddMessageProtocol),
    AddUser(AddUserProtocol),
    RegisterData(RegisterDataProtocol),
    GetData(GetDataProtocol),
    DeleteData(DeleteDataProtocol),
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
            Self::RegisterData(cmd) => cmd.encode(),
            Self::GetData(cmd) => cmd.encode(),
            Self::DeleteData(cmd) => cmd.encode(),
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
