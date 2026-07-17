use std::{collections::BTreeMap, fmt::Display, net::{IpAddr, Ipv4Addr}, sync::{Arc, Mutex, mpsc}, thread, time::SystemTime};

pub const VERSION: &str = "v0.1";
pub const PROG: &str = "speakrs";


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

struct Server {
    channels: BTreeMap<ChannelId, TextChannel>,
    users: BTreeMap<UserId, User>,
}

impl Server {

    fn new() -> Self {
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

    fn test(&mut self) {
        //self.channels.get_mut(&ChannelId::from(0));
    }

    pub fn load_user(&mut self, id: UserId, name: String) -> UserId {
        let user = User::new(id, name);
        self.users.insert(id, user);
        id
    }

    pub fn add_user(&mut self, name: String) -> UserId {
        let id = *self.users.keys().max().unwrap_or(&UserId::default());
        self.load_user(id, name)
    }


    pub fn load_channel(&mut self, id: ChannelId, name: String, desc: String) -> ChannelId {
        let channel = TextChannel::new(id, name, desc);
        self.channels.insert(id, channel);
        id
    }

    pub fn add_channel(&mut self, name: String, desc: String) -> ChannelId {
        let id = *self.channels.keys().max().unwrap_or(&ChannelId::default());
        self.load_channel(id, name, desc)
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct ChannelId {
    id: u8,
}
impl ChannelId {
    fn from(id: u8) -> Self {
        Self {
            id
        }
    }
}
struct TextChannel {
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

    pub fn load_message(&mut self, id: MessageId, timestamp: SystemTime, user: UserId, content: String) -> MessageId {
        let message = Message::new(id, timestamp, user, content);
        self.messages.insert(id, message);
        id
    }

    pub fn add_message(&mut self, timestamp: Option<SystemTime>, user: UserId, content: String) -> MessageId {
        let id = *self.messages.keys().max().unwrap_or(&MessageId::default());
        let timestamp = timestamp.unwrap_or(SystemTime::now());
        self.load_message(id, timestamp, user, content)
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct UserId {
    id: u8,
}
impl UserId {
    fn from(id: u8) -> Self {
        Self {
            id
        }
    }
}
struct User {
    id: UserId,
    name: String,
}
impl User {
    fn new(id: UserId, name: String) -> Self {
        Self { id, name }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
struct MessageId {
    id: u32,
}
impl MessageId {
    fn from(id: u32) -> Self {
        Self {
            id
        }
    }
}
struct Message {
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
