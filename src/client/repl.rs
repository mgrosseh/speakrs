use super::ClientArguments;

use crate::common::{
    self,
    database::ServerDB,
    rpc::RpcServiceClient,
    schema::{ChannelData, ChannelKey, ClientData, UserData, UserKey},
};
use anyhow::{Context, Result};
use linefeed::{Completion, Interface, Prompter, ReadResult, Terminal};
use std::{collections::HashMap, fmt::Debug, sync::OnceLock};
use std::{
    io::{self, Write},
    sync::Arc,
};
use tarpc::tokio_serde::formats::Json;
use tokio::net::ToSocketAddrs;
use tracing::{Instrument, error, info, info_span};

fn print_error(e: impl Debug) {
    println!("{e:?}");
}

fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();

    match s.find(|ch: char| ch.is_whitespace()) {
        Some(pos) => (&s[..pos], s[pos..].trim_start()),
        None => (s, ""),
    }
}
/// Parse at most [`count`] words from [`text`] sequence, returning vec of words and rest.
/// Ends early if reaching end of [`text`]. Whitespace in-between words is ignored.
/// Rest may not include trailing whitespace at the end of the last word.
fn split_opt_words(count: usize, text: &str) -> (Vec<&str>, &str) {
    let mut vec = Vec::new();
    if count == 0 {
        return (vec, text);
    }
    let mut rest = text;
    loop {
        rest = rest.trim();
        if rest.is_empty() {
            return (vec, rest);
        }
        let (word, r) = split_first_word(rest);
        vec.push(word);
        rest = r;
        if vec.len() >= count {
            break;
        }
    }
    (vec, rest)
}


/// Handle Result error case by printing the error and using "continue". Returns unwrapped result value.
macro_rules! try_continue {
    ($expr:expr $(,)?) => {
        match $expr {
            Ok(value) => value,
            Err(e) => {
                print_error(e);
                continue;
            }
        }
    };
}

fn quick_prompt(prompt: &str) -> Result<ReadResult> {
    let interface = Arc::new(Interface::new("speakrs_prompt")?);
    interface.set_prompt(prompt)?;
    interface.set_report_signal(linefeed::Signal::Interrupt, true);
    Ok(interface.read_line()?)
}
/// Opens new prompt, terminates when input matches [`terminate`], returning entered lines as a vec.
/// Each line begins with [`prompt`]. If ^C i.e. an interrupt signal is send, aborts returning None.
/// If Eof is received, terminates as well.
fn multiline_prompt(prompt: &str, terminate: impl Fn(&String) -> bool) -> Result<Option<Vec<String>>> {
    let interface = Arc::new(Interface::new("speakrs_prompt")?);
    interface.set_prompt(prompt)?;
    interface.set_report_signal(linefeed::Signal::Interrupt, true);
    let mut lines = Vec::new();
    loop {
        match interface.read_line()? {
            ReadResult::Input(line) => {
                if terminate(&line) {
                    break;
                }
                lines.push(line);
            },
            ReadResult::Signal(linefeed::Signal::Interrupt) => return Ok(None),
            _ => break,
        }
    }

    Ok(Some(lines))
}

const HISTORY_FILE: &str = "repl.history";

#[tracing::instrument]
pub async fn repl(args: ClientArguments) -> Result<()> {
    let mut connection: ReplConnection = ReplConnection::empty();

    let interface = Arc::new(Interface::new("speakrs")?);

    println!("Speakrs repl. Use \"help\" for a list of commands.");
    interface.set_completer(Arc::new(Completer));
    interface.set_prompt("> ")?;

    let mut history_file = common::config_home();
    history_file.push(HISTORY_FILE);
    if let Err(e) = interface.load_history(history_file.clone()) {
        if e.kind() == io::ErrorKind::NotFound {
            info!(
                "History file {} doesn't exists, not loading history.",
                HISTORY_FILE
            );
        } else {
            error!("Could not load history file {}: {:?}", HISTORY_FILE, e);
        }
    }

    while let ReadResult::Input(line) = interface.read_line()? {
        if !line.trim().is_empty() {
            interface.add_history_unique(line.clone());
        }
        let (cmd, args) = split_first_word(&line);
        match cmd {
            "help" => {
                help_for("", "");
                continue;
            }
            "quit" => break,
            "connect" => {
                let (address, rest) = split_first_word(args);
                if !rest.trim().is_empty() {
                    println!(
                        "Unexpected args in command connect after \"{} {}\": \"{}\"",
                        cmd, address, rest
                    );
                    continue;
                }
                connection = try_continue!(
                    get_connection(address)
                        .await
                        .context("Error during connect command")
                );
                if connection.has() {
                    println!("Connected to server. You might want to run sync to load new content.");
                }
            }
            "channel" => {
                if !connection.has() {
                    println!(
                        "You are currently not connected to a server, use `connect` or see `help`."
                    );
                    continue;
                }
                try_continue!(
                    repl_channel(
                        connection.client(),
                        connection.db(),
                        connection.user_key(),
                        args
                    )
                    .await
                    .context("Error during channel command")
                )
            }
            _ => println!("Unknown command {}", cmd),
        }
    }
    if let Err(e) = interface.save_history(history_file.clone()) {
        error!(
            "Could not save history file {}: {}",
            history_file.to_string_lossy(),
            e
        );
    } else {
        info!("History saved to {}", history_file.to_string_lossy());
    }

    Ok(())
}

fn help_for(root: &str, prefix: &str) {
    let mut buffer = 0;
    if prefix.len() < 20 {
        buffer = 20 - prefix.len();
    }
    for (cmd, args, help) in commands().get(root) {
        let mut command_display = format!("{} {}", cmd, args);
        if command_display.len() < buffer {
            command_display.push_str(" ".repeat(buffer - command_display.len()).as_str())
        }
        println!("{prefix}{} {}", command_display, help);
        if commands().has(cmd) {
            let prefix = format!("  {prefix}");
            help_for(cmd, prefix.as_str());
        }
    }
}

struct ReplConnection {
    connection: Option<Connection>,
    db: Option<ServerDB>,
    client_data: Option<ClientData>,
}
impl ReplConnection {
    fn empty() -> Self {
        Self {
            connection: None,
            db: None,
            client_data: None,
        }
    }
    fn create(connection: Connection, db: ServerDB, client_data: ClientData) -> Self {
        Self {
            connection: Some(connection),
            db: Some(db),
            client_data: Some(client_data),
        }
    }
    fn has(&self) -> bool {
        self.connection.is_some()
    }
    fn client(&self) -> RpcServiceClient {
        self.connection.as_ref().map(|c| c.get_client()).unwrap()
    }
    fn db(&self) -> ServerDB {
        self.db.clone().unwrap()
    }
    fn user_key(&self) -> UserKey {
        self.client_data.as_ref().map(|d| d.user_key).unwrap()
    }
}

struct Connection {
    client: RpcServiceClient,
}
impl Connection {
    async fn create(addr: impl ToSocketAddrs) -> Result<Self> {
        let mut transport = tarpc::serde_transport::tcp::connect(addr, Json::default);
        transport.config_mut().max_frame_length(usize::MAX);
        let client =
            RpcServiceClient::new(tarpc::client::Config::default(), transport.await?).spawn();
        Ok(Connection { client })
    }
    fn get_client(&self) -> RpcServiceClient {
        self.client.clone()
    }
}

async fn get_connection(address: impl ToSocketAddrs + Debug) -> Result<ReplConnection> {
    let connection = Connection::create(address).await?;
    let data = connection
        .get_client()
        .get_server_data(tarpc::context::current())
        .instrument(info_span!("Asking server for server data"))
        .await
        .context("RpcError during connection attempt")?
        .context("Error while talking to server")?;
    println!("Found server `{}`", data.name);

    let db = ServerDB::magic_open_client(data.name, data.uuid)?;
    let mut client_data = db
        .get_client_data()
        .context("Error while reading local database")?;
    if client_data.is_none() {
        println!("Server has not been registered with, would you like to create a user? [y/n]");
        match quick_prompt("[y/n]? ")? {
            ReadResult::Input(x) if !(x.starts_with("y") || x.starts_with("Y")) => {
                println!("Aborted.");
                return Ok(ReplConnection::empty());
            }
            ReadResult::Input(_) => (),
            _ => return Ok(ReplConnection::empty()),
        }
        let mut username = String::new();
        while username.is_empty() {
            match quick_prompt("Username: ")? {
                ReadResult::Input(x) if common::is_valid_username(&x) => {
                    username = x;
                    break;
                }
                ReadResult::Input(x) => {
                    println!("`{}` is not a valid username, try again.", x);
                }
                _ => return Ok(ReplConnection::empty()),
            }
        }
        let user_data = UserData::new(username.clone());

        let user_key = connection
            .get_client()
            .create_user(tarpc::context::current(), user_data.clone())
            .instrument(info_span!("Asking server for new user"))
            .await
            .context("RpcError during connection attempt")?
            .context("Error while talking to server")?;

        let data = ClientData {
            user_key: user_key.clone(),
        };
        client_data = Some(data.clone());
        db.set_client_data(data)
            .context("Error while writing to local database")?;

        db.users()?.insert((user_key, user_data))?;
        println!("Created user {} with uuid {}.", username, user_key);
    }
    Ok(ReplConnection::create(connection, db, client_data.unwrap()))
}

async fn repl_channel(
    client: RpcServiceClient,
    db: ServerDB,
    user: UserKey,
    args: &str,
) -> Result<()> {
    let (cmd, args) = split_first_word(args);
    match cmd {
        "sync" => return repl_channel_sync(client, db, user).await,
        "add" => return repl_channel_add(client, db, user, args).await,
        "list" => return repl_channel_list(db).await,             // TODO: use pager if possible
        cmd => println!("Unknown subcommand `channel {cmd}`. Use `help`."),
    }
    Ok(())
}

async fn repl_channel_sync(client: RpcServiceClient, db: ServerDB, user: UserKey) -> Result<()> {
    println!("Syncing channels...");
    let last_known_channel = db.channels()?.last()?.map(|kv| kv.0);
    let new_channels = client
        .clone()
        .get_new_channels_since(tarpc::context::current(), user.clone(), last_known_channel)
        .instrument(info_span!("Asking server for channel list"))
        .await?
        .context("Error while talking to server")?;
    let len = new_channels.len();
    for channel in new_channels {
        db.channels()?.insert((channel.0, channel.1))?;
    }
    println!("Got {} new channels. Use `channel list` to list them.", len);
    Ok(())
}
async fn repl_channel_add(client: RpcServiceClient, db: ServerDB, user: UserKey, args: &str) -> Result<()> {
    let (args, rest) = split_opt_words(1, args);
    let name = if args.len() == 1 {
        args.get(0).unwrap().to_string()
    } else {
        match quick_prompt("Channel name: ")? {
            ReadResult::Input(x) if common::is_valid_channel_name(&x) => x,
            ReadResult::Input(x) => {
                println!("`{}` is not a valid channel name.\nChannel names must follow rules: {}", x, common::CHANNEL_NAME_RULES);
                return Ok(());
            }
            _ => {
                println!("Did not add channel.");
                return Ok(());
            },
        }
    };
    let desc = if !rest.trim().is_empty() {
        rest.to_string()
    } else {
        println!("Enter channel description (multiline). Send EOF or enter an empty line to confirm, ^C to cancel.");
        let x = multiline_prompt("desc: ", |x| x.is_empty())?;
        if x.is_none() {
            println!("Cancelled. Did not add channel.");
            return Ok(());
        }
        x.unwrap().join("\n")
    };
    let data = ChannelData::text(name.clone(), desc);

    let key = client
        .clone()
        .create_channel(tarpc::context::current(), user.clone(), data.clone())
        .instrument(info_span!("Creating channel in server"))
        .await?
        .context("Error while talking to server")?;

    db.channels()?.insert((key, data))?;
    println!("Created channel {} with uuid {}", name, key);
    Ok(())
}

async fn repl_channel_list(db: ServerDB) -> Result<()> {
    let channels = db
        .channels()?
        .range(..)
        .collect::<anyhow::Result<Vec<(ChannelKey, ChannelData)>>>()?;
    for (_, value) in channels {
        println!(
            "Channel `{}`: \"{}\"",
            value.get_name(),
            value.get_description()
        )
    }
    Ok(())
}

type Command<'a> = (&'a str, &'a str, &'a str);
fn commands() -> &'static AllCommands {
    static SUBS: OnceLock<AllCommands> = OnceLock::new();
    SUBS.get_or_init(|| {
        let mut map = HashMap::new();
        map.insert("", COMMANDS);
        map.insert("channel", CHANNEL_COMMANDS);

        AllCommands { map }
    })
}
static CHANNEL_COMMANDS: &[Command] = &[
    ("add", "NAME", "Add a channel with NAME"),
    ("sync", "", "Sync channels with server"),
    ("list", "", "List all channels"),
];

static COMMANDS: &[Command] = &[
    (
        "connect",
        "IP:PORT",
        "Connect to server with given ip and port",
    ),
    ("channel", "SUBCOMMAND", "Channel command group"),
    ("message", "SUBCOMMAND", "Message command group"),
    ("help", "", "open this help"),
    ("quit", "", "quit the repl"),
];

struct AllCommands {
    map: HashMap<&'static str, &'static [Command<'static>]>,
}
impl AllCommands {
    fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }
    fn get(&self, key: &str) -> &'static [Command<'static>] {
        self.map.get(key).unwrap()
    }
}

struct Completer;

impl<Term: Terminal> linefeed::Completer<Term> for Completer {
    fn complete(
        &self,
        word: &str,
        prompter: &Prompter<Term>,
        start: usize,
        _end: usize,
    ) -> Option<Vec<Completion>> {
        let line = prompter.buffer();

        let mut words = line[..start].split_whitespace();
        let key = match words.next() {
            None => "",
            Some(y) => y,
        };
        if !commands().has(key) {
            return None;
        }
        let mut compls = Vec::new();
        for &(cmd, _, _) in commands().get(key) {
            if cmd.starts_with(word) {
                compls.push(Completion::simple(cmd.to_owned()));
            }
        }
        Some(compls)
        // TODO: check if more words and do better completion
        // TODO: complete e.g. channel name etc in commands
    }
}
