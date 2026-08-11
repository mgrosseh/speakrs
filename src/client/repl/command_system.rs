use anyhow::Result;
use linefeed::{Completion, Prompter, Terminal};
use std::{
    fmt::Display,
};
use tokio::task::JoinHandle;
use tracing::warn;

use super::{split_first_word, ReplConnection, current_connection};

// TODO: maybe generalize
fn fetch_all_channel_names() -> Result<Vec<String>> {
    let connection = current_connection().read().unwrap();
    if !connection.has() {
        return Ok(vec![]);
    }
    let db = connection.db();
    Ok(db
        .channels()?
        .range(..)
        .map(|result| result.map(|(_, v)| v.get_name().to_owned()))
        .collect::<Result<Vec<String>>>()?)
}

pub(super) trait CommandTreeCompletion<'a> {
    fn get_completion(&self, word: &str) -> Option<Completion>;
    fn matches(&self, word: &str) -> bool;
    fn find_match(&self, word: &str) -> Option<CommandTreeEither<'a>>;
    fn add_completions_after(&self, compls: &mut Vec<Completion>, word: &str);
}
#[derive(Clone)]
pub(super) enum CommandTreePart<'a> {
    Members(&'a [CommandTreeMember<'a>]),
    Arguments(&'a [CommandTreeArgument<'a>]),
}
#[derive(Clone)]
pub(super) enum CommandTreeEither<'a> {
    Member(&'a CommandTreeMember<'a>),
    Argument(&'a CommandTreeArgument<'a>),
}
impl<'a> CommandTreeEither<'a> {
    fn traverse_to(self, text: &str) -> Option<CommandTreeEither<'a>> {
        match self {
            CommandTreeEither::Member(member) => match member.children {
                CommandTreePart::Members(_) => {
                    let (word, rest) = split_first_word(text);
                    if word.is_empty() {
                        return Some(self);
                    }
                    if let Some(next) = self.find_match(word) {
                        next.traverse_to(rest)
                    } else {
                        None
                    }
                }
                CommandTreePart::Arguments(children) => {
                    let (mut word, mut rest) = split_first_word(text);
                    for i in 0..children.len() {
                        if word.is_empty() {
                            return Some(CommandTreeEither::Argument(&children[i]));
                        }

                        (word, rest) = split_first_word(rest);
                    }

                    None
                }
            },
            CommandTreeEither::Argument(_) => {
                if text.trim().is_empty() {
                    Some(self)
                } else {
                    None
                }
            }
        }
    }
    fn traverse_to_member(self, text: &str) -> Option<(&'a CommandTreeMember<'a>, String)> {
        match self {
            CommandTreeEither::Argument(_) => return None,
            CommandTreeEither::Member(member) => {
                if text.trim().is_empty() || !member.is_group() {
                    return Some((member, text.to_owned()));
                }
            }
        }
        let (word, rest) = split_first_word(text);

        if let Some(next) = self.find_match(word) {
            next.traverse_to_member(rest)
        } else {
            None
        }
    }
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
    fn add_completions_after(&self, mut compls: &mut Vec<Completion>, word: &str) {
        match self {
            CommandTreeEither::Member(member) => member.add_completions_after(&mut compls, word),
            CommandTreeEither::Argument(argument) => {
                argument.add_completions_after(&mut compls, word)
            }
        }
    }
}
#[derive(Clone)]
pub(super) struct CommandTreeMember<'a> {
    name: &'a str,
    desc: &'a str,
    pub binding: Option<BindingFn>,
    children: CommandTreePart<'a>,
}
pub(super) type BindingFn = fn(ReplConnection, String) -> JoinHandle<Result<()>>;

impl<'a> CommandTreeMember<'a> {
    pub const fn binding(
        name: &'a str,
        desc: &'a str,
        args: &'a [CommandTreeArgument<'a>],
        binding: BindingFn,
    ) -> Self {
        let children = CommandTreePart::Arguments(args);
        CommandTreeMember {
            desc,
            name,
            binding: Some(binding),
            children,
        }
    }
    pub const fn single(name: &'a str, desc: &'a str, args: &'a [CommandTreeArgument<'a>]) -> Self {
        let children = CommandTreePart::Arguments(args);
        CommandTreeMember {
            desc,
            name,
            binding: None,
            children,
        }
    }
    pub const fn group(name: &'a str, desc: &'a str, children: &'a [CommandTreeMember<'a>]) -> Self {
        let children = CommandTreePart::Members(children);
        CommandTreeMember {
            desc,
            name,
            binding: None,
            children,
        }
    }

    pub fn is_group(&self) -> bool {
        match self.children {
            CommandTreePart::Members(_) => true,
            CommandTreePart::Arguments(_) => false,
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
                let fill_remove = depth + command.len() + 1; // +1 since below we add a space in write!
                if fill_remove <= fill {
                    command.push_str(&" ".repeat(fill - fill_remove));
                }
                write!(f, "{prefix}{command} {}", self.desc)?;
            }
        }
        Ok(())
    }
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
    fn add_completions_after(&self, compls: &mut Vec<Completion>, word: &str) {
        match self.children {
            CommandTreePart::Arguments(arguments) => {
                for arg in arguments {
                    if let Some(x) = arg.get_completion(word) {
                        compls.push(x);
                    }
                }
            }
            CommandTreePart::Members(members) => {
                for child in members {
                    if let Some(x) = child.get_completion(word) {
                        compls.push(x);
                    }
                }
            }
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum ArgumentType {
    ChannelName,
    String,
    Int,
    IpAddress,
}
impl ArgumentType {
    fn matches(self, _word: &str) -> bool {
        true // TODO: could depending on type include a quick test of sorts to categorize this argument roughly, full test might be too expensive
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
            ArgumentType::IpAddress => (), // TODO: maybe expand LOCALHOST or other neat shortcuts into ip_addresses
        }
    }
}
#[derive(Clone)]
pub(super) enum CommandTreeArgument<'a> {
    Required(ArgumentType, &'a str),
    #[allow(unused)]
    RequiredMany(ArgumentType, &'a str),
    Optional(ArgumentType, &'a str),
    OptionalMany(ArgumentType, &'a str),
    WithDefault(ArgumentType, &'a str, &'a str),
}
impl<'a> CommandTreeArgument<'a> {
    fn help(&self) -> String {
        match self {
            CommandTreeArgument::Required(_, s) => format!("{s}"),
            CommandTreeArgument::RequiredMany(_, s) => format!("{s}..."),
            CommandTreeArgument::Optional(_, s) => format!("[{s}]"),
            CommandTreeArgument::OptionalMany(_, s) => format!("[{s}...]"),
            CommandTreeArgument::WithDefault(_, s, d) => format!("[{s}={d}]"),
        }
    }
    fn get_name(&self) -> &'a str {
        match self {
            CommandTreeArgument::Required(_, name) => name,
            CommandTreeArgument::RequiredMany(_, name) => name,
            CommandTreeArgument::Optional(_, name) => name,
            CommandTreeArgument::OptionalMany(_, name) => name,
            CommandTreeArgument::WithDefault(_, name, _) => name,
        }
    }
    fn get_argument_type(&self) -> ArgumentType {
        match self {
            CommandTreeArgument::Required(arg_type, _) => *arg_type,
            CommandTreeArgument::RequiredMany(arg_type, _) => *arg_type,
            CommandTreeArgument::Optional(arg_type, _) => *arg_type,
            CommandTreeArgument::OptionalMany(arg_type, _) => *arg_type,
            CommandTreeArgument::WithDefault(arg_type, _, _) => *arg_type,
        }
    }
}
impl<'a> CommandTreeCompletion<'a> for CommandTreeArgument<'a> {
    fn get_completion(&self, _word: &str) -> Option<Completion> {
        println!("|{}|", self.get_name());
        None // TODO
    }

    fn matches(&self, word: &str) -> bool {
        self.get_argument_type().matches(word)
    }

    fn find_match(&self, _word: &str) -> Option<CommandTreeEither<'a>> {
        None // since we have no children, always None
    }

    fn add_completions_after(&self, compls: &mut Vec<Completion>, word: &str) {
        // TODO: I want to show the name of completion here if possible and none
        self.get_argument_type().add_completions(compls, word);
    }
}

#[derive(Clone)]
pub(super) struct CommandTree<'a>(pub &'a [CommandTreeMember<'a>]);
impl<'a> CommandTree<'a> {
    fn help_fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for member in self.0 {
            member.help_fmt(f, 0, 35)?;
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
    fn traverse_to(&self, first: &str, rest: &str) -> Option<CommandTreeEither<'a>> {
        let node = self.find_match(first);
        if node.is_none() {
            return None;
        }
        node.unwrap().traverse_to(rest)
    }
    pub fn traverse_to_member(&self, text: &str) -> Option<(&'a CommandTreeMember<'a>, String)> {
        if text.trim().is_empty() {
            return None;
        }

        let (word, rest) = split_first_word(text);

        if let Some(next) = self.find_match(word) {
            next.traverse_to_member(rest)
        } else {
            None
        }
    }
}
impl Display for CommandTree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.help_fmt(f)
    }
}

impl<Term: Terminal> linefeed::Completer<Term> for &'_ CommandTree<'_> {
    fn complete(
        &self,
        word: &str,
        prompter: &Prompter<Term>,
        start: usize,
        _end: usize,
    ) -> Option<Vec<Completion>> {
        let line = prompter.buffer();

        // TODO: with backtracking or recursion we can add many compls by traversing multiple branches that match instead of only the first

        let (first, rest) = split_first_word(&line[..start]);

        let mut compls = Vec::new();
        if first.is_empty() {
            for child in self.0 {
                if let Some(x) = child.get_completion(word) {
                    compls.push(x);
                }
            }
            return Some(compls);
        } else {
            let node = self.traverse_to(first, rest);
            if node.is_none() {
                return None;
            }
            node.unwrap().add_completions_after(&mut compls, word);
            return Some(compls);
        }
    }
}


#[cfg(test)]
mod test {
    use super::*;
    fn a_test_binding(_connection: ReplConnection, args: String) -> JoinHandle<Result<()>> {
        tokio::spawn(async move {
            let (word, _) = split_first_word(&args);
            println!("{}", word);
            Ok(())
        })
    }
    #[tokio::test]
    async fn test_binding() {
        let x = &CommandTree(&[CommandTreeMember::binding(
            "test",
            "Open this help.",
            &[],
            a_test_binding,
        )]);
        let binding = x.0[0].binding.unwrap();
        let connection = ReplConnection::empty();
        let args = "het the tri".to_owned();
        binding(connection, args).await.unwrap().unwrap();
    }

    static TEST_COMMANDS: &CommandTree<'static> = &CommandTree(&[
        CommandTreeMember::single("help", "Testing tests", &[]),
        CommandTreeMember::group(
            "test_group",
            "Testing tests",
            &[
                CommandTreeMember::single("test", "Testing tests", &[]),
                CommandTreeMember::single("group", "Testing tests", &[]),
            ],
        ),
    ]);
    #[test]
    fn test_traverse() {
        let text = "test_group test some args ignored";
        let (m, rest) = TEST_COMMANDS.traverse_to_member(text).unwrap();
        assert_eq!(m.name, "test");
        assert_eq!(rest, "some args ignored");
    }
}
