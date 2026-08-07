use super::ClientArguments;

use crate::common::{
    self,
    database::ServerDB,
    rpc::RpcServiceClient,
    schema::{ChannelData, ChannelKey, ClientData, UserData, UserKey},
};
use anyhow::{Context, Result};
use linefeed::{Completion, Interface, Prompter, ReadResult, Terminal};
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    sync::OnceLock,
};
use std::{
    io::{self},
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

const HISTORY_FILE: &str = "repl.history";

// TODO: use new COMMANDS system to show more useful help after mistyped command

#[tracing::instrument]
pub async fn repl(args: ClientArguments) -> Result<()> {
    let mut connection: ReplConnection = ReplConnection::empty();

    let interface = Arc::new(Interface::new("speakrs")?);

    println!("Speakrs repl. Use \"help\" for a list of commands.");
    interface.set_completer(Arc::new(*COMMANDS));
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
                    println!(
                        "Connected to server. You might want to run sync to load new content."
                    );
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
            "message" => {
                if !connection.has() {
                    println!(
                        "You are currently not connected to a server, use `connect` or see `help`."
                    );
                    continue;
                }
                try_continue!(
                    repl_message(
                        connection.client(),
                        connection.db(),
                        connection.user_key(),
                        args
                    )
                    .await
                    .context("Error during message command")
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
        "list" => return repl_channel_list(db).await, // TODO: use pager if possible
        cmd => println!("Unknown subcommand `channel {cmd}`. Use `help`."),
    }
    Ok(())
}

async fn repl_message(
    client: RpcServiceClient,
    db: ServerDB,
    user: UserKey,
    args: &str,
) -> Result<()> {
    let (cmd, args) = split_first_word(args);
    match cmd {
        "sync" => return repl_message_sync(client, db, user, args).await,
        "add" => return repl_message_add(client, db, user, args).await,
        "view" => return repl_message_list(db, args).await, // TODO: use pager if possible
        cmd => println!("Unknown subcommand `message {cmd}`. Use `help`."),
    }
    Ok(())
}

async fn repl_message_add(
    client: RpcServiceClient,
    db: ServerDB,
    user: UserKey,
    args: &str,
) -> Result<()> {
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
    println!(
        "Got {} new messages. Use `message view` to go though them.",
        len
    );
    Ok(())
}
async fn repl_message_sync(
    client: RpcServiceClient,
    db: ServerDB,
    user: UserKey,
    args: &str,
) -> Result<()> {
    Ok(())
}
async fn repl_message_list(db: ServerDB, args: &str) -> Result<()> {
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
async fn repl_channel_add(
    client: RpcServiceClient,
    db: ServerDB,
    user: UserKey,
    args: &str,
) -> Result<()> {
    let (args, rest) = split_opt_words(1, args);
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

trait CommandTreeCompletion<'a> {
    fn get_completion(&self, word: &str) -> Option<Completion>;
    fn matches(&self, word: &str) -> bool;
    fn find_match(&self, word: &str) -> Option<CommandTreeEither<'a>>;
    fn add_completions(&self, compls: &mut Vec<Completion>, word: &str);

}
#[derive(Debug, Clone, Copy)]
enum CommandTreePart<'a> {
    Members(&'a [CommandTreeMember<'a>]),
    Arguments(&'a [CommandTreeArgument<'a>]),
}
#[derive(Debug, Clone, Copy)]
enum CommandTreeEither<'a> {
    Member(&'a CommandTreeMember<'a>),
    Argument(&'a CommandTreeArgument<'a>),
}
impl<'a> CommandTreeCompletion<'a> for CommandTreeEither<'a> {
    fn get_completion(&self, word: &str) -> Option<Completion> {
        match self {
            CommandTreeEither::Member(member) => member.get_completion(word),
            CommandTreeEither::Argument(argument) => argument.get_completion(word),
        }
    }

    fn matches(&self, word: &str) -> bool {
        match self {
            CommandTreeEither::Member(member) => member.matches(word),
            CommandTreeEither::Argument(argument) => argument.matches(word),
        }
    }

    fn find_match(&self, word: &str) -> Option<CommandTreeEither<'a>> {
        match self {
            CommandTreeEither::Member(member) => member.find_match(word),
            CommandTreeEither::Argument(argument) => argument.find_match(word),
        }
    }
    fn add_completions(&self, mut compls: &mut Vec<Completion>, word: &str) {
        match self {
            CommandTreeEither::Member(member) => member.add_completions(&mut compls, word),
            CommandTreeEither::Argument(argument) => argument.add_completions(&mut compls, word),
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct CommandTreeMember<'a> {
    name: &'a str,
    desc: &'a str,
    children: CommandTreePart<'a>,
}
impl<'a> CommandTreeCompletion<'a> for CommandTreeMember<'a> {
    fn get_completion(&self, word: &str) -> Option<Completion> {
        if self.name.starts_with(word) {
            Some(Completion::simple(self.name.to_string()))
        } else {
            None
        }
    }
    fn matches(&self, word: &str) -> bool {
        self.name == word
    }

    fn find_match(&self, word: &str) -> Option<CommandTreeEither<'a>> {
        match self.children {
            CommandTreePart::Arguments(args) => {
                for arg in args {
                    if arg.matches(word) {
                        return Some(CommandTreeEither::Argument(arg));
                    }
                }
            }
            CommandTreePart::Members(members) => {
                for member in members {
                    if member.matches(word) {
                        return Some(CommandTreeEither::Member(member));
                    }
                }
            }
        }
        None
    }
    fn add_completions(&self, compls: &mut Vec<Completion>, word: &str) {
        match self.children {
            CommandTreePart::Arguments(arguments) => {
                for arg in arguments {
                    if let Some(x) = arg.get_completion(word) {
                        compls.push(x);
                    }
                }
            },
            CommandTreePart::Members(members) => {
                for child in members {
                    if let Some(x) = child.get_completion(word) {
                        compls.push(x);
                    }
                }
            },
        }
    }
}
impl<'a> CommandTreeMember<'a> {
    const fn single(name: &'a str, desc: &'a str, args: &'a [CommandTreeArgument<'a>]) -> Self {
        let children = CommandTreePart::Arguments(args);
        CommandTreeMember {
            desc,
            name,
            children,
        }
    }
    const fn group(name: &'a str, desc: &'a str, children: &'a [CommandTreeMember<'a>]) -> Self {
        let children = CommandTreePart::Members(children);
        CommandTreeMember {
            desc,
            name,
            children,
        }
    }

    fn help_fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        depth: usize,
        fill: usize,
    ) -> std::fmt::Result {
        let prefix = " ".repeat(depth);
        match self.children {
            CommandTreePart::Members(members) => {
                let mut command = format!("{} SUBCOMMAND", self.name);
                let dofill = fill - depth - command.len() - 1; // -1 since below we add a space in write!
                if dofill > 0 {
                    command.push_str(&" ".repeat(dofill));
                }
                write!(f, "{prefix}{command} {}", self.desc)?;
                for child in members {
                    write!(f, "\n")?;
                    child.help_fmt(f, depth + 1, fill)?;
                }
            }
            CommandTreePart::Arguments(args) => {
                let mut command = self.name.to_owned();
                for child in args {
                    command.push(' ');
                    command.push_str(&child.help());
                }
                let dofill = fill - depth - command.len() - 1; // -1 since below we add a space in write!
                if dofill > 0 {
                    command.push_str(&" ".repeat(dofill));
                }
                write!(f, "{prefix}{command} {}", self.desc)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy)]
enum CommandTreeArgument<'a> {
    Required(&'a str),
    RequiredMany(&'a str),
    Optional(&'a str),
    OptionalMany(&'a str),
    WithDefault(&'a str, &'a str),
}
impl<'a> CommandTreeArgument<'a> {
    fn help(&self) -> String {
        match self {
            CommandTreeArgument::Required(s) => format!("{s}"),
            CommandTreeArgument::RequiredMany(s) => format!("{s}..."),
            CommandTreeArgument::Optional(s) => format!("[{s}]"),
            CommandTreeArgument::OptionalMany(s) => format!("[{s}...]"),
            CommandTreeArgument::WithDefault(s, d) => format!("[{s}={d}]"),
        }
    }
}
impl<'a> CommandTreeCompletion<'a> for CommandTreeArgument<'a> {
    fn get_completion(&self, _word: &str) -> Option<Completion> {
        None // TODO
    }

    fn matches(&self, _word: &str) -> bool {
        true // always match, so that future completion works; don't need to do double work
        // TODO: this stance might change if there are multiple branches, probably not
    }

    fn find_match(&self, _word: &str) -> Option<CommandTreeEither<'a>> {
        None // since we have no children, always None
    }

    fn add_completions(&self, _compls: &mut Vec<Completion>, _word: &str) {
        // TODO
    }
}

#[derive(Clone, Copy)]
struct CommandTree<'a>(&'a [CommandTreeMember<'a>]);
impl<'a> CommandTree<'a> {
    fn help_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for member in self.0 {
            member.help_fmt(f, 0, 24)?;
            write!(f, "\n")?;
        }
        Ok(())
    }

    fn find_match(&self, word: &str) -> Option<CommandTreeEither<'a>> {
        for child in self.0 {
            if child.matches(word) {
                return Some(CommandTreeEither::Member(child));
            }
        }
        None
    }
}
impl<'a> Display for CommandTree<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.help_fmt(f)
    }
}

impl<'a, Term: Terminal> linefeed::Completer<Term> for CommandTree<'a> {
    fn complete(
        &self,
        word: &str,
        prompter: &Prompter<Term>,
        start: usize,
        _end: usize,
    ) -> Option<Vec<Completion>> {
        let line = prompter.buffer();

        let mut words = line[..start].split_whitespace();

        // TODO: with backtracking or recursion we can add many compls by traversing multiple branches that match instead of only the first

        let mut compls = Vec::new();
        match words.next() {
            None => {
                for child in self.0 {
                    if let Some(x) = child.get_completion(word) {
                        compls.push(x);
                    }
                }
                return Some(compls);
            }
            Some(current) => {
                let node = self.find_match(current);
                if node.is_none() {
                    return None;
                }
                let mut node = node.unwrap();
                while let Some(current) = words.next() {
                    if let Some(next) = node.find_match(current) {
                        node = next;
                        continue;
                    }
                    return None;
                }
                node.add_completions(&mut compls, word);
                return Some(compls);
            }
        }
    }
}

static COMMANDS: &CommandTree = &CommandTree(&[
    CommandTreeMember::single("help", "Open this help.", &[]),
    CommandTreeMember::single("quit", "Quit the repl.", &[]),
    CommandTreeMember::single(
        "connect",
        "Connect to server with IP on PORT.",
        &[CommandTreeArgument::Required("IP:PORT")],
    ),
    CommandTreeMember::group(
        "message",
        "Manipulate messages",
        &[
            CommandTreeMember::single(
                "add",
                "Add a message in CHANNEL with CONTENT. If none, CONTENT can be entered interactively.",
                &[
                    CommandTreeArgument::Required("CHANNEL"),
                    CommandTreeArgument::Optional("CONTENT"),
                ],
            ),
            CommandTreeMember::single(
                "sync",
                "Sync messages in CHANNEL with server",
                &[CommandTreeArgument::Required("CHANNEL")],
            ),
            CommandTreeMember::single(
                "view",
                "View COUNT messages in CHANNEL, skipping the last OFFSET messages",
                &[
                    CommandTreeArgument::Required("CHANNEL"),
                    CommandTreeArgument::WithDefault("COUNT", "5"),
                    CommandTreeArgument::WithDefault("OFFSET", "0"),
                ],
            ),
        ],
    ),
    CommandTreeMember::group(
        "channel",
        "Manipulate channels",
        &[
            CommandTreeMember::single(
                "add",
                "Add a channel with NAME and DESCRIPTION. If not provided, asks for them interactively (Multi-line DESCRIPTION).",
                &[
                    CommandTreeArgument::Optional("NAME"),
                    CommandTreeArgument::OptionalMany("DESCRIPTION"),
                ],
            ),
            CommandTreeMember::single("sync", "Sync channels with server", &[]),
            CommandTreeMember::single("list", "List all locally known channels (see `sync`)", &[]),
        ],
    ),
]);
