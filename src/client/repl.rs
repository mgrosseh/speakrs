use super::ClientArguments;

use crate::common::{
    self,
    database::ServerDB,
    rpc::RpcServiceClient,
    schema::{ChannelData, ChannelKey, ClientData, MessageData, MessageKey, UserData, UserKey},
};
use anyhow::{Context, Result};
use linefeed::{Interface, ReadResult};
use std::{
    fmt::{Debug},
    sync::{OnceLock, RwLock},
};
use std::{
    io::{self},
    sync::Arc,
};
use tarpc::tokio_serde::formats::Json;
use tokio::{net::ToSocketAddrs, task::JoinHandle};
use tracing::{Instrument, error, info, info_span};

use command_system::*;

pub mod command_system;

fn print_error(e: impl Debug) {
    println!("{e:?}");
}

pub fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();

    match s.find(char::is_whitespace) {
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
fn multiline_prompt(
    prompt: &str,
    terminate: impl Fn(&String) -> bool,
) -> Result<Option<Vec<String>>> {
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
            }
            ReadResult::Signal(linefeed::Signal::Interrupt) => return Ok(None),
            _ => break,
        }
    }

    Ok(Some(lines))
}

/// returns true if args is empty, else prints an error and returns false
fn arg_guard(args: &str) -> bool {
    if !args.is_empty() {
        println!("Warning got trailing data, try again. (Unexpected: \"{args}\")");
        return false;
    }
    true
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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

fn current_connection() -> &'static RwLock<ReplConnection> {
    static CONNECTION: OnceLock<RwLock<ReplConnection>> = OnceLock::new();
    CONNECTION.get_or_init(|| RwLock::new(ReplConnection::empty()))
}

const HISTORY_FILE: &str = "repl.history";

// TODO: use new COMMANDS system to show more useful help after mistyped command

#[tracing::instrument]
pub async fn repl(args: ClientArguments) -> Result<()> {
    let interface = Arc::new(Interface::new("speakrs")?);

    println!("Speakrs repl. Use \"help\" for a list of commands.");
    interface.set_completer(Arc::new(COMMANDS));
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
                println!("{}", COMMANDS);
                continue;
            }
            "quit" => break,
            "connect" => {
                let (address, rest) = split_first_word(args);
                if !arg_guard(rest) {
                    continue;
                }
                let connection = try_continue!(
                    get_connection(address)
                        .await
                        .context("Error during connect command")
                );
                if connection.has() {
                    *current_connection().write().unwrap() = connection;
                    println!(
                        "Connected to server. You might want to run sync to load new content."
                    );
                }
            }
            _ => {
                if !current_connection().read().unwrap().has() {
                    println!(
                        "You are currently not connected to a server, use `connect` or see `help`."
                    );
                    continue;
                }
                execute_command(COMMANDS, line).await?;
            }
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
                    println!(
                        "Usernames must follow these rules: {}",
                        common::USERNAME_RULES
                    );
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

async fn execute_command(commands: &CommandTree<'_>, line: String) -> Result<()> {
    let option = commands.traverse_to_member(&line);
    if option.is_none() {
        println!("Command not found.");
        return Ok(());
    }
    let (member, args) = option.unwrap();
    if member.binding.is_none() {
        println!("Command has no associated binding, please report this bug.");
        return Ok(());
    }
    if let Err(e) = member.binding.unwrap()(current_connection().read().unwrap().clone(), args)
        .await
        .context(format!("While executing command: {}", line))
    {
        print_error(e);
    }
    Ok(())
}

static COMMANDS: &CommandTree = &CommandTree(&[
    CommandTreeMember::single("help", "Open this help.", &[]),
    CommandTreeMember::single("quit", "Quit the repl.", &[]),
    CommandTreeMember::single(
        "connect",
        "Connect to server with IP on PORT.",
        &[CommandTreeArgument::Required(
            ArgumentType::IpAddress,
            "IP:PORT",
        )],
    ),
    CommandTreeMember::group(
        "message",
        "Manipulate messages",
        &[
            CommandTreeMember::binding(
                "add",
                "Add a message in CHANNEL with CONTENT. If none, CONTENT can be entered interactively.",
                &[
                    CommandTreeArgument::Required(ArgumentType::ChannelName, "CHANNEL"),
                    CommandTreeArgument::Optional(ArgumentType::String, "CONTENT"),
                ],
                repl_message_add,
            ),
            CommandTreeMember::binding(
                "sync",
                "Sync messages in CHANNEL with server",
                &[CommandTreeArgument::Required(
                    ArgumentType::ChannelName,
                    "CHANNEL",
                )],
                repl_message_sync,
            ),
            CommandTreeMember::binding(
                "view",
                "View COUNT messages in CHANNEL, skipping the last OFFSET messages",
                &[
                    CommandTreeArgument::Required(ArgumentType::ChannelName, "CHANNEL"),
                    CommandTreeArgument::WithDefault(ArgumentType::Int, "COUNT", "5"),
                    CommandTreeArgument::WithDefault(ArgumentType::Int, "OFFSET", "0"),
                ],
                repl_message_view,
            ),
        ],
    ),
    CommandTreeMember::group(
        "channel",
        "Manipulate channels",
        &[
            CommandTreeMember::binding(
                "add",
                "Add a channel with NAME and DESCRIPTION. If not provided, asks for them interactively (Multi-line DESCRIPTION).",
                &[
                    CommandTreeArgument::Optional(ArgumentType::String, "NAME"),
                    CommandTreeArgument::OptionalMany(ArgumentType::String, "DESCRIPTION"),
                ],
                repl_channel_add,
            ),
            CommandTreeMember::binding("sync", "Sync channels with server", &[], repl_channel_sync),
            CommandTreeMember::binding(
                "list",
                "List all locally known channels (see `sync`)",
                &[],
                repl_channel_list,
            ),
        ],
    ),
]);

fn repl_message_add(connection: ReplConnection, args: String) -> JoinHandle<Result<()>> {
    let client = connection.client();
    let db = connection.db();
    let user = connection.user_key();
    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        if args.is_empty() {
            println!("Expected CHANNEL_NAME arg.");
            return Ok(());
        }
        let channel = get_channel_by_name(db.clone(), arg)?;
        if channel.is_none() {
            println!("Could not find channel with name {}.", arg);
            return Ok(());
        }
        let (channel_key, channel_data) = channel.unwrap();
        let content = if !rest.trim().is_empty() {
            rest.to_string()
        } else {
            println!(
                "Enter message content (multiline). Send EOF or enter an empty line to confirm, ^C to cancel."
            );
            let x = multiline_prompt("msg: ", |x| x.is_empty())?;
            if x.is_none() {
                println!("Cancelled. Did not send message.");
                return Ok(());
            }
            x.unwrap().join("\n")
        };
        let data = MessageData::now(user, content);

        let key = client
            .clone()
            .insert_message(tarpc::context::current(), channel_key, data.clone())
            .instrument(info_span!("Creating message in server"))
            .await?
            .context("Error while talking to server")?;

        db.messages()?.set(key, data.clone())?;
        println!(
            "Created message at {} with key {} in channel \"{}\"",
            data.timestamp,
            key,
            channel_data.get_name()
        );
        Ok(())
    })
}
// TODO: make function on DBTree that gets elements based on filter, to generalize these types of actions
fn get_channel_by_name(db: ServerDB, name: &str) -> Result<Option<(ChannelKey, ChannelData)>> {
    match db
        .channels()?
        .range(..)
        .filter(|result| match result {
            Err(_e) => {
                false // TODO this error now is ignored
            }
            Ok(kv) => kv.1.get_name() == name,
        })
        .nth(0)
    {
        Some(x) => match x {
            Err(e) => Err(e),
            Ok(v) => Ok(Some(v)),
        },
        None => Ok(None),
    }
}
fn repl_message_sync(connection: ReplConnection, args: String) -> JoinHandle<Result<()>> {
    let client = connection.client();
    let db = connection.db();
    let user = connection.user_key();

    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        // TODO: these arg checks should be handled by arg system
        if !arg_guard(rest) {
            return Ok(());
        }
        if arg.is_empty() {
            println!("Expected argument CHANNEL_NAME.");
            return Ok(());
        }
        let channel = get_channel_by_name(db.clone(), arg)?;
        if channel.is_none() {
            println!("Could not find channel with name {}.", arg);
            return Ok(());
        }
        let channel = channel.unwrap();
        println!("Syncing messages...");
        let last_known_message = db
            .messages()?
            .range(..)
            .filter(|res| match res {
                Err(_e) => false, // TODO: ideally there would be some filter on DBTree key-value-pairs
                Ok(kv) => kv.0.prefix() == channel.0,
            })
            .map(|x| x)
            .last()
            .map(|res| res.map(|kv| kv.0));
        let last_known_message = match last_known_message {
            Some(Err(e)) => return Err(e),
            Some(Ok(x)) => Some(x),
            None => None,
        };
        let new_messages = client
            .get_new_messages_since(tarpc::context::current(), user.clone(), last_known_message)
            .instrument(info_span!("Asking server for message list"))
            .await?
            .context("Error while talking to server")?;
        let len = new_messages.len();
        for (key, data) in new_messages {
            db.messages()?.set(key, data)?;
        }
        println!(
            "Got {} new messages. Use `message view CHANNEL` to list them.",
            len
        );
        Ok(())
    })
}
fn repl_message_view(connection: ReplConnection, _args: String) -> JoinHandle<Result<()>> {
    // TODO: channels
    let db = connection.db();
    tokio::spawn(async move {
        let channels = db
            .messages()?
            .range(..)
            .collect::<anyhow::Result<Vec<(MessageKey, MessageData)>>>()?;
        for (_, value) in channels {
            println!("Message `{}`: \"{}\"", value.author, value.content)
        }
        Ok(())
    })
}

fn repl_channel_sync(connection: ReplConnection, _args: String) -> JoinHandle<Result<()>> {
    let client = connection.client();
    let db = connection.db();
    let user = connection.user_key();
    tokio::spawn(async move {
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
    })
}
fn repl_channel_add(connection: ReplConnection, args: String) -> JoinHandle<Result<()>> {
    let client = connection.client();
    let db = connection.db();
    let user = connection.user_key();
    tokio::spawn(async move {
        let (args, rest) = split_opt_words(1, &args);
        let name = if args.len() == 1 {
            args.get(0).unwrap().to_string()
        } else {
            match quick_prompt("Channel name: ")? {
                ReadResult::Input(x) if common::is_valid_channel_name(&x) => x,
                ReadResult::Input(x) => {
                    println!(
                        "`{}` is not a valid channel name.\nChannel names must follow rules: {}",
                        x,
                        common::CHANNEL_NAME_RULES
                    );
                    return Ok(());
                }
                _ => {
                    println!("Did not add channel.");
                    return Ok(());
                }
            }
        };
        let desc = if !rest.trim().is_empty() {
            rest.to_string()
        } else {
            println!(
                "Enter channel description (multiline). Send EOF or enter an empty line to confirm, ^C to cancel."
            );
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
    })
}

fn repl_channel_list(connection: ReplConnection, _args: String) -> JoinHandle<Result<()>> {
    let db = connection.db();
    tokio::spawn(async move {
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
    })
}
