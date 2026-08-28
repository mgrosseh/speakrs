use super::{
    ClientArguments,
    connection::{Connection, clone_current_connection, current_connection},
    notifications,
};

use crate::{
    common::{self, config::config_home},
    schema::channel::Channel,
};
use anyhow::Result;
use linefeed::{Completion, Interface, ReadResult};
use speakrs_storage::pagination::Pagination;
use std::{fmt::Debug, usize};
use std::{
    io::{self},
    sync::Arc,
};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

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
    if !connection.is_registered() {
        return Ok(vec![]);
    }
    let db = connection.db();
    Ok(db
        .channels(Pagination::first(usize::MAX))?
        .nodes()
        .map(|c| c.get_name().to_owned())
        .collect::<Vec<_>>())
}

const HISTORY_FILE: &str = "repl.history";

// TODO: use new COMMANDS system to show more useful help after mistyped command

#[tracing::instrument]
pub async fn repl(args: ClientArguments) -> Result<()> {
    let interface = Arc::new(Interface::new("speakrs")?);

    println!("Speakrs repl. Use \"help\" for a list of commands.");
    interface.set_completer(Arc::new(COMMANDS));
    interface.set_prompt("> ")?;

    let mut history_file = config_home();
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
    current_connection().read().unwrap().is_registered()
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
    CommandTreeMember::binding_if(
        "login",
        "Log into the connected server (or reauth). Ask for password interactively.",
        &[],
        login,
        check_connection,
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
                "Download channel messages from server",
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
    CommandTreeMember::binding_if(
        "dump_db",
        "Dumps the DB of currently connected server to FILE. WARNING: this will load the whole database into memory, depending on size of the database this may cause significant lag and/or crashes.",
        &[CommandTreeArgument::Required(ArgumentType::String, "FILE")],
        dump,
        check_connection,
    ),
]);

fn help(_: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async {
        println!("{}", COMMANDS);
        Ok(())
    })
}

// TODO: add dump_name which does not require an active connection.
fn dump(args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        if arg.is_empty() {
            println!("Requires filename to store to.");
            return Ok(());
        }
        if !arg_guard(rest) {
            return Ok(());
        }
        let _connection = clone_current_connection();
        // connection.dump_db_to(arg)?;
        println!("Dumped db to `{arg}`");

        Ok(())
    })
}

fn connect(args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let (arg, rest) = split_first_word(&args);
        if !arg_guard(rest) {
            return Ok(());
        }
        if current_connection().read().unwrap().is_connected() {
            println!("Already connected. Disconnect first.");
            return Ok(());
        }
        info!("Creating connection to server...");
        let mut connection = Connection::connect_to_ip(arg).await?;
        let info = connection.db().server_info()?;
        info!("Found server `{}`", info.name);
        println!("Found server `{}`", info.name);

        if !connection.is_registered() {
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

            connection = connection.register_user(&username, &password).await?;
            connection = connection.login(password).await?;
            let user_key = connection.session().user_key;
            println!("Created user {} with uuid {}.", username, user_key);
        }

        *current_connection().write().unwrap() = connection;
        println!("Connected to server.");
        Ok(())
    })
}

fn login(args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        if !arg_guard(&args) {
            println!("Extra args after command.");
            return Ok(());
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
        *current_connection().write().unwrap() = clone_current_connection().login(password).await?;
        println!("Logged in!");
        Ok(())
    })
}

fn repl_message_add(args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let connection = clone_current_connection();
        let db = connection.db().clone();
        let (arg, rest) = split_first_word(&args);
        if args.is_empty() {
            println!("Expected CHANNEL_NAME arg.");
            return Ok(());
        }

        let content = if !rest.trim().is_empty() {
            rest.to_string()
        } else {
            println!(
                "Enter message content (multiline). Send EOF or enter an empty line to confirm, ^C to cancel."
            );
            let Some(x) = multiline_prompt("msg: ", |x| x.is_empty())? else {
                println!("Cancelled. Did not send message.");
                return Ok(());
            };
            x.join("\n")
        };

        let Some(channel) = db
            .channels(Pagination::first(usize::MAX))?
            .focus
            .into_iter()
            .find(|ch| ch.get_name() == arg)
        else {
            println!("Could not find channel with name {}.", arg);
            return Ok(());
        };

        let key = connection.send_message(channel.cursor, content).await?;

        println!(
            "Created message with key {} in channel \"{}\"",
            key,
            channel.get_name()
        );
        Ok(())
    })
}
fn repl_message_sync(args: String) -> JoinHandle<Result<()>> {
    // TODO: get rid of manual syncing
    // TODO: long-term syncing ALL messages, will be a bad idea, the system ideally will have some notion of pages
    tokio::spawn(async move {
        let connection = clone_current_connection();
        // TODO: these arg checks should be handled by arg system
        if !arg_guard(&args) {
            return Ok::<(), anyhow::Error>(());
        }
        println!("Syncing messages...");
        let len = connection.download_all_messages().await?;

        println!(
            "Got {} new messages. Use `message view CHANNEL` to list them.",
            len
        );
        Ok::<(), anyhow::Error>(())
    })
}
fn repl_message_view(_args: String) -> JoinHandle<Result<()>> {
    // TODO: channels
    tokio::spawn(async move {
        let connection = clone_current_connection();
        let channels = connection.db().messages(Pagination::first(usize::MAX))?;
        for msg in channels.iter() {
            println!("Message `{}`: \"{}\"", msg.author, msg.content)
        }
        Ok(())
    })
}

fn repl_channel_sync(_args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        println!("Syncing channels...");
        let len = clone_current_connection().download_all_channels().await?;
        println!("Got {} new channels. Use `channel list` to list them.", len);
        Ok(())
    })
}
fn repl_channel_add(args: String) -> JoinHandle<Result<()>> {
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
        let data = Channel::text(name.clone(), desc);

        let key = clone_current_connection().add_channel(data).await?;
        println!("Created channel {} with uuid {}", name, key);
        Ok(())
    })
}

fn repl_channel_list(_args: String) -> JoinHandle<Result<()>> {
    tokio::spawn(async move {
        let conn = clone_current_connection();
        let channels = conn.db().channels(Pagination::first(usize::MAX))?;
        for channel in channels.iter() {
            println!(
                "Channel `{}`: \"{}\"",
                channel.get_name(),
                channel.get_description()
            )
        }
        Ok(())
    })
}
