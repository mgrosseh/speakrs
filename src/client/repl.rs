use super::{ClientArguments, Connection, clone_current_connection, current_connection};

use crate::{
    client::client_schema::ClientData,
    common::{
        self,
        database::DB,
        schema::{ChannelData, ChannelKey, MessageData, MessageKey, UserData},
    },
};
use anyhow::{Context, Result};
use linefeed::{Completion, Interface, ReadResult};
use std::fmt::Debug;
use std::{
    io::{self},
    sync::Arc,
};
use tokio::task::JoinHandle;
use tracing::{Instrument, error, info, info_span, warn};

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

fn quick_prompt(prompt: &str) -> Result<ReadResult> {
    let interface = Arc::new(Interface::new("speakrs_prompt")?);
    interface.set_prompt(prompt)?;
    interface.set_report_signal(linefeed::Signal::Interrupt, true);
    Ok(interface.read_line()?)
}
/// Opens new prompt, terminates when input line matches [`terminate`], returning entered lines as a vec.
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

// TODO: move proper place
pub fn fetch_all_channel_names() -> Result<Vec<String>> {
    let connection = current_connection().read().unwrap();
    if !connection.has() {
        return Ok(vec![]);
    }
    let db = connection.db();
    Ok(db
        .channels()?
        .map(|(_, v)| v.get_name().to_owned())
        .collect::<Result<Vec<String>, _>>()?)
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
        let (cmd, _) = split_first_word(&line);
        if cmd == "quit" {
            break;
        }
        match COMMANDS.execute_command(line).await {
            Ok(false) => println!(
                "You are currently not connected to a server, use `connect` or see `help`."
            ),
            Ok(true) => (),
            Err(e) => print_error(e),
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

fn check_connection() -> bool {
    current_connection().read().unwrap().has()
}

#[derive(Clone, Copy)]
pub(super) enum ArgumentType {
    ChannelName,
    String,
    Int,
    SocketAddress,
}
impl Argument for ArgumentType {
    fn matches_simple(self, _word: &str) -> bool {
        true // TODO: could depending on type include a quick test of sorts to categorize this argument roughly, full test might be too expensive
    }
    fn matches_full(self, _word: &str) -> bool {
        true // TODO
    }

    fn add_completions(self, compls: &mut Vec<Completion>, word: &str) {
        match self {
            ArgumentType::ChannelName => match fetch_all_channel_names() {
                Ok(names) => {
                    for name in names {
                        if word.is_empty() || name.starts_with(word) {
                            compls.push(Completion::simple(name))
                        }
                    }
                }
                Err(e) => warn!(
                    "Could not do ChannelName completion, error during fetch: {:?}",
                    e
                ),
            },
            ArgumentType::String => (),
            ArgumentType::Int => (),
            ArgumentType::SocketAddress => (), // TODO: maybe expand LOCALHOST or other neat shortcuts into ip_addresses
        }
    }
}

static COMMANDS: &CommandTree<ArgumentType> = &CommandTree(&[
    CommandTreeMember::binding("help", "Open this help.", &[], help),
    CommandTreeMember::simple("quit", "Quit the repl.", &[]),
    CommandTreeMember::binding(
        "connect",
        "Connect to server with IP on PORT.",
        &[CommandTreeArgument::Required(
            ArgumentType::SocketAddress,
            "IP:PORT",
        )],
        connect,
    ),
    CommandTreeMember::group(
        "message",
        "Manipulate messages",
        &[
            CommandTreeMember::binding_if(
                "add",
                "Add a message in CHANNEL with CONTENT. If none, CONTENT can be entered interactively.",
                &[
                    CommandTreeArgument::Required(ArgumentType::ChannelName, "CHANNEL"),
                    CommandTreeArgument::Optional(ArgumentType::String, "CONTENT"),
                ],
                repl_message_add,
                check_connection,
            ),
            CommandTreeMember::binding_if(
                "sync",
                "Sync messages in CHANNEL with server",
                &[CommandTreeArgument::Required(
                    ArgumentType::ChannelName,
                    "CHANNEL",
                )],
                repl_message_sync,
                check_connection,
            ),
            CommandTreeMember::binding_if(
                "view",
                "View COUNT messages in CHANNEL, skipping the last OFFSET messages",
                &[
                    CommandTreeArgument::Required(ArgumentType::ChannelName, "CHANNEL"),
                    CommandTreeArgument::WithDefault(ArgumentType::Int, "COUNT", "5"),
                    CommandTreeArgument::WithDefault(ArgumentType::Int, "OFFSET", "0"),
                ],
                repl_message_view,
                check_connection,
            ),
        ],
    ),
    CommandTreeMember::group(
        "channel",
        "Manipulate channels",
        &[
            CommandTreeMember::binding_if(
                "add",
                "Add a channel with NAME and DESCRIPTION. If not provided, asks for them interactively (Multi-line DESCRIPTION).",
                &[
                    CommandTreeArgument::Optional(ArgumentType::String, "NAME"),
                    CommandTreeArgument::OptionalMany(ArgumentType::String, "DESCRIPTION"),
                ],
                repl_channel_add,
                check_connection,
            ),
            CommandTreeMember::binding_if(
                "sync",
                "Sync channels with server",
                &[],
                repl_channel_sync,
                check_connection,
            ),
            CommandTreeMember::binding_if(
                "list",
                "List all locally known channels (see `sync`)",
                &[],
                repl_channel_list,
                check_connection,
            ),
        ],
    ),
]);

fn help(_: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async {
        println!("{}", COMMANDS);
        Ok(())
    })
}

fn connect(args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        if !arg_guard(rest) {
            return Ok(());
        }
        // TODO: bound and arg check
        info!("Creating connection to server...");
        let connection = Connection::create_service_client(arg).await?;
        let data = connection
            .get_server_data(tarpc::context::current())
            .instrument(info_span!("Asking server for server data"))
            .await
            .context("RpcError during connection attempt")?
            .context("Error while talking to server")?;
        println!("Found server `{}`", data.name);

        let db = DB::magic_open_client(data.name, data.uuid)?;
        let mut client_data = db
            .get_client_data()
            .context("Error while reading local database")?;
        if client_data.is_none() {
            println!("Server has not been registered with, would you like to create a user? [y/n]");
            match quick_prompt("[y/n]? ")? {
                ReadResult::Input(x) if !(x.starts_with("y") || x.starts_with("Y")) => {
                    println!("Aborted.");
                    return Ok(());
                }
                ReadResult::Input(_) => (),
                _ => return Ok(()),
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
                    _ => return Ok(()),
                }
            }
            let mut password = String::new();
            while password.is_empty() {
                match quick_prompt("Password: ")? {
                    ReadResult::Input(x) => {
                        password = x;
                        break;
                    }
                    _ => {
                        println!("Cancelled.");
                        return Ok(());
                    }
                }
            }
            let user_data = UserData::new(username.clone());

            let user_key = connection
                .register_user(
                    tarpc::context::current(),
                    user_data.clone(),
                    password.clone(),
                )
                .instrument(info_span!("Asking server for new user"))
                .await
                .context("RpcError during connection attempt")?
                .context("Error while talking to server")?;

            let session = connection
                .authenticate_session(tarpc::context::current(), user_key, password.clone())
                .instrument(info_span!(
                    "Authenticating with server using new credentials"
                ))
                .await
                .context("RpcError during connection attempt")?
                .context("Error while talking to server")?;

            let data = ClientData {
                user_key: user_key.clone(),
                session: Some(session),
            };
            client_data = Some(data.clone());
            db.set_client_data(data)
                .context("Error while writing to local database")?;

            db.users()?.insert((user_key, user_data))?;
            println!("Created user {} with uuid {}.", username, user_key);
        }

        *current_connection().write().unwrap() =
            Connection::new(connection, db, client_data.unwrap());
        println!("Connected to server.");
        Ok(())
    })
}

fn repl_message_add(args: String) -> JoinHandle<Result<()>> {
    let (client, db, client_data) = clone_current_connection().unwrap();
    let user = client_data.user_key;
    let session = client_data.session.unwrap(); // TODO handle no session
    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        if args.is_empty() {
            println!("Expected CHANNEL_NAME arg.");
            return Ok(());
        }
        let channel = db.channels()?.try_find(|kv| kv.1.get_name() == arg);
        if channel.is_none() {
            println!("Could not find channel with name {}.", arg);
            return Ok(());
        }
        let (channel_key, channel_data) = channel.unwrap()?;
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
            .insert_message(
                tarpc::context::current(),
                session,
                channel_key,
                data.clone(),
            )
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
fn repl_message_sync(args: String) -> JoinHandle<Result<()>> {
    // TODO: long-term syncing ALL messages, will be a bad idea, the system ideally will have some notion of pages
    let (client, db, client_data) = clone_current_connection().unwrap();
    let session = client_data.session.unwrap(); // TODO handle no session

    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        // TODO: these arg checks should be handled by arg system
        if !arg_guard(rest) {
            return Ok::<(), anyhow::Error>(());
        }
        if arg.is_empty() {
            println!("Expected argument CHANNEL_NAME.");
            return Ok(());
        }
        let channel = db.channels()?.try_find(|kv| kv.1.get_name() == arg);
        if channel.is_none() {
            println!("Could not find channel with name {}.", arg);
            return Ok(());
        }
        let channel = channel.unwrap()?;
        println!("Syncing messages...");
        let last_known_message = db
            .messages()?
            .try_filter(|kv| kv.0.prefix() == channel.0)
            .last()
            .transpose()?
            .map(|kv| kv.0);

        let new_messages = client
            .get_new_messages_since(tarpc::context::current(), session, last_known_message)
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
        Ok::<(), anyhow::Error>(())
    })
}
fn repl_message_view(_args: String) -> JoinHandle<Result<()>> {
    // TODO: channels
    let (_, db, __data) = clone_current_connection().unwrap();
    tokio::spawn(async move {
        let channels = db
            .messages()?
            .range(..)
            .collect::<Result<Vec<(MessageKey, MessageData)>, _>>()?;
        for (_, value) in channels {
            println!("Message `{}`: \"{}\"", value.author, value.content)
        }
        Ok(())
    })
}

fn repl_channel_sync(_args: String) -> JoinHandle<Result<()>> {
    let (client, db, client_data) = clone_current_connection().unwrap();
    let session = client_data.session.unwrap(); // TODO handle no session
    tokio::spawn(async move {
        println!("Syncing channels...");
        let last_known_channel = db.channels()?.last()?.map(|kv| kv.0);
        let new_channels = client
            .clone()
            .get_new_channels_since(tarpc::context::current(), session, last_known_channel)
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
fn repl_channel_add(args: String) -> JoinHandle<Result<()>> {
    let (client, db, client_data) = clone_current_connection().unwrap();
    let session = client_data.session.unwrap(); // TODO handle no session
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
                // TODO: BUG: ^C does not cancel the way its expected
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
            .create_channel(tarpc::context::current(), session, data.clone())
            .instrument(info_span!("Creating channel in server"))
            .await?
            .context("Error while talking to server")?;

        db.channels()?.insert((key, data))?;
        println!("Created channel {} with uuid {}", name, key);
        Ok(())
    })
}

fn repl_channel_list(_args: String) -> JoinHandle<Result<()>> {
    let (_, db, _) = clone_current_connection().unwrap();
    tokio::spawn(async move {
        let channels = db
            .channels()?
            .range(..)
            .collect::<Result<Vec<(ChannelKey, ChannelData)>, _>>()?;
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
