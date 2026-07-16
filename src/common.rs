use std::{fmt::Display, sync::{Arc, Mutex, mpsc}, thread};

use crate::common::IP::IPv4;

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
pub(crate) enum IP {
    IPv4(u8, u8, u8, u8),
    #[allow(unused)]
    IPv6(u16, u16, u16, u16, u16, u16, u16, u16),
}

impl IP {
    /// Create an IP from given string (using format n.n.n.n where each n is a u8)
    ///
    /// # Panics
    /// 
    /// The `from_str_v4` will panic if string is not a valid ip
    pub(crate) fn from_str_v4(string: &str) -> Self {
        let mut split = string.split('.');
        let a = split
            .next().expect("Expecting first part of ip.")
            .parse::<u8>().expect("Expecting valid u8.");
        let b = split
            .next().expect("Expecting second part of ip.")
            .parse::<u8>().expect("Expecting valid u8.");
        let c = split
            .next().expect("Expecting third part of ip.")
            .parse::<u8>().expect("Expecting valid u8.");
        let d = split
            .next().expect("Expecting forth part of ip.")
            .parse::<u8>().expect("Expecting valid u8.");
        Self::IPv4(a, b, c, d)
    }
}
impl Display for IP {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IPv4(a, b, c, d) => {
                write!(f, "{a}.{b}.{c}.{d}")
            }
            _ => {
                todo!("IPv6 not implemented yet.")
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Arguments {
    pub mode: Mode,
    pub server_ip: IP,
    pub server_tcp_port: Port,
    pub server_udp_port: Port,
    pub verbose: bool,
    pub quiet: bool,
}


impl Arguments {
    pub fn parse(args: &[String]) -> Option<Self> {
        let mut mode: Option<Mode> = Option::None;
        let mut server_ip: Option<IP> = Option::None;
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
                x if x.starts_with("--ip=") => server_ip = Option::Some(IP::from_str_v4(x.strip_prefix("--ip=").unwrap())),
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
            server_ip: server_ip.unwrap_or(IP::from_str_v4("127.0.0.1")),
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
