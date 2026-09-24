//! The topics `prot-scriber doc` prints: what the options cannot say one at a time -- how to make
//! the input, which database needs which rule list, how to cluster gene families, and how to find
//! out why a description was chosen.
//!
//! Each topic is a text file in `src/doc/`, compiled in and printed exactly as it is written. Not
//! through clap's help renderer, which rewraps to the terminal: a topic holds command lines and
//! indented examples that are only right as written. So the files are written for a terminal 80
//! columns wide, which is the one width they cannot adapt to, and a test holds them to it.
//!
//! They are under `src/` because that is what a release is built from: a correction to a topic
//! ships with the next release, as a correction to the code does.

use crate::error::Error;
use clap::builder::PossibleValue;
use clap::ValueEnum;
use std::io::{self, Write};

/// One topic: the name `prot-scriber doc` is asked for it by, and its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Topic {
    name: &'static str,
    text: &'static str,
}

/// A topic named after its file in `src/doc/`, so that the name it is asked for by and the file it
/// is read from are one word, written once.
macro_rules! topic {
    ($name:literal) => {
        Topic {
            name: $name,
            text: include_str!(concat!("doc/", $name, ".txt")),
        }
    };
}

/// Every topic, in the order `doc` lists them. The one place a topic is registered: the listing,
/// the parser's possible values and its "a similar value exists" all read this.
const TOPICS: &[Topic] = &[
    topic!("input"),
    topic!("databases"),
    topic!("families"),
    topic!("explain"),
];

impl Topic {
    /// The first line of the topic, which is its title and what the listing shows beside its name.
    /// There is no second, hand-written summary to fall out of step with the text.
    pub fn title(&self) -> &'static str {
        self.text.lines().next().unwrap_or_default()
    }

    /// The whole topic, exactly as it is printed.
    pub fn text(&self) -> &'static str {
        self.text
    }
}

impl ValueEnum for Topic {
    fn value_variants<'a>() -> &'a [Self] {
        TOPICS
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(PossibleValue::new(self.name).help(self.title()))
    }
}

/// What `prot-scriber doc` prints without a topic: every topic's name beside its title, in the
/// shape `prot-scriber defaults` lists the built-in rules in.
pub fn listing() -> String {
    let width = TOPICS
        .iter()
        .map(|topic| topic.name.len())
        .max()
        .unwrap_or(0);
    let mut listed =
        String::from("prot-scriber's topics. Read one with\n\n    prot-scriber doc <TOPIC>\n\n");
    for topic in TOPICS {
        listed.push_str(&format!(
            "    {:width$}  {}\n",
            topic.name,
            topic.title(),
            width = width
        ));
    }
    listed
}

/// The topics' names as prose -- 'input, databases, families and explain' -- for the pointer to
/// them in the help, which then names every topic there is and none there is not.
pub fn names() -> String {
    let names: Vec<&str> = TOPICS.iter().map(|topic| topic.name).collect();
    match names.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {}", rest.join(", "), last),
        None => String::new(),
    }
}

/// Writes a topic, or the listing when none is named, to standard output.
///
/// # Arguments
///
/// * `topic` - Which topic to print, or `None` to list them.
pub fn print(topic: Option<Topic>) -> Result<(), Error> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let text = match topic {
        Some(topic) => topic.text().to_string(),
        None => listing(),
    };
    // Flushed here, because a full disk behind a redirection must not look like success:
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| Error::Io(format!("\n\nCould not write the topic: {}\n\n", e)))
}

#[cfg(test)]
mod tests {
    use super::{listing, Topic, TOPICS};
    use crate::cli::{BuiltIn, Cli, DefaultList};
    use clap::{CommandFactory, ValueEnum};
    use regex::Regex;

    /// The widest a line may be. A topic is printed as written, so a longer line is broken by the
    /// terminal wherever it happens to fall -- mid-word, or in the middle of a command line that
    /// is meant to be copied.
    const COLUMNS: usize = 80;

    /// Every line of every topic, and of the listing, fits a terminal 80 columns wide. Counted in
    /// characters, not bytes, because that is what a terminal counts: the en dashes in a citation
    /// are three bytes and one column. A TAB is refused outright, since how wide it is depends on
    /// where it stands.
    #[test]
    fn every_topic_fits_an_80_column_terminal() {
        let mut texts: Vec<(String, &str)> = TOPICS
            .iter()
            .map(|topic| (format!("topic {}", topic.name), topic.text))
            .collect();
        let listed = listing();
        texts.push((String::from("the listing"), &listed));
        for (what, text) in texts {
            for (number, line) in text.lines().enumerate() {
                assert!(
                    !line.contains('\t'),
                    "{}, line {}, holds a TAB, whose width depends on where it stands:\n{}",
                    what,
                    number + 1,
                    line
                );
                assert!(
                    line.chars().count() <= COLUMNS,
                    "{}, line {}, is {} columns wide:\n{}",
                    what,
                    number + 1,
                    line.chars().count(),
                    line
                );
            }
        }
    }

    /// A topic's first line is its title, which the listing and the possible values show, so it
    /// has to be one: something there, and the text of the topic after it.
    #[test]
    fn every_topic_opens_with_a_title() {
        for topic in TOPICS {
            assert!(
                !topic.title().trim().is_empty(),
                "topic {} has no title",
                topic.name
            );
            assert!(
                topic.text.ends_with('\n'),
                "topic {} does not end its last line",
                topic.name
            );
            assert!(
                topic.text.lines().count() > 3,
                "topic {} is its title and nothing else",
                topic.name
            );
        }
    }

    /// The long options, and the short ones, that a command declares, aliases included.
    ///
    /// # Arguments
    ///
    /// * `command` - The command whose declarations to read.
    fn declared(command: &clap::Command) -> (Vec<String>, Vec<char>) {
        let mut longs = vec![];
        let mut shorts = vec![];
        for argument in command.get_arguments() {
            longs.extend(argument.get_long().map(|long| format!("--{}", long)));
            longs.extend(
                argument
                    .get_visible_aliases()
                    .unwrap_or_default()
                    .iter()
                    .map(|alias| format!("--{}", alias)),
            );
            shorts.extend(argument.get_short());
        }
        (longs, shorts)
    }

    /// A command line split the way a shell splits it, as far as a topic needs: words separated by
    /// white space, quotes holding white space and `|` together, and `|` outside quotes a word of
    /// its own, since it is where one program's arguments end.
    ///
    /// # Arguments
    ///
    /// * `line` - The command line.
    fn shell_words(line: &str) -> Vec<String> {
        let mut words = vec![];
        let mut word = String::new();
        let mut quote: Option<char> = None;
        for c in line.chars() {
            match (quote, c) {
                (Some(open), c) if c == open => quote = None,
                (Some(_), c) => word.push(c),
                (None, '\'') | (None, '"') => quote = Some(c),
                (None, '|') => {
                    words.extend((!word.is_empty()).then(|| std::mem::take(&mut word)));
                    words.push(String::from("|"));
                }
                (None, c) if c.is_whitespace() => {
                    words.extend((!word.is_empty()).then(|| std::mem::take(&mut word)));
                }
                (None, c) => word.push(c),
            }
        }
        words.extend((!word.is_empty()).then_some(word));
        words
    }

    /// Every option, `@NAME` list, `defaults NAME`, `doc TOPIC` and `help COMMAND` a topic names
    /// exists.
    ///
    /// What counts as existing is read from the DECLARATIONS -- `Cli::command()`, the lists' and
    /// topics' own enumerations -- and never from help prose, which is how Diamond's quiet flag
    /// and mcl's abc flag once counted as prot-scriber's options: the text they stood in was part
    /// of the help the known options were read from.
    ///
    /// THE CONVENTION THIS RELIES ON, since topics quote other tools' command lines too: a line
    /// indented by white space is a command, joined with the lines a trailing `\` continues it
    /// onto. Of a command, only the programs that are `prot-scriber` -- the first word, or the
    /// first word after a `|` -- are checked, and against the options of the verb they name; what
    /// Blast, Diamond, mcl, sed, awk or sort are given is theirs and is not read. Everything else
    /// is prose, in which every long option is prot-scriber's and must be declared by one of its
    /// commands. So a topic must not name another tool's long option in prose; quote it in a
    /// command instead.
    ///
    /// Where this stops: an option is checked for existing, not for belonging to the verb a
    /// sentence is about, and a value is checked only where it is an `@NAME`, a `defaults` name, a
    /// topic or a command.
    #[test]
    fn every_option_list_and_name_a_topic_gives_exists() {
        let top = Cli::command();
        let verbs: Vec<String> = top
            .get_subcommands()
            .map(|verb| verb.get_name().to_string())
            .collect();
        let all_longs: Vec<String> = std::iter::once(&top)
            .chain(top.get_subcommands())
            .flat_map(|command| declared(command).0)
            .collect();
        let lists: Vec<String> = DefaultList::value_variants()
            .iter()
            .map(|list| list.to_possible_value().unwrap().get_name().to_string())
            .collect();
        let built_ins: Vec<String> = BuiltIn::value_variants()
            .iter()
            .map(|rule| rule.name())
            .collect();
        let topics: Vec<&str> = TOPICS.iter().map(|topic| topic.name).collect();

        let option = Regex::new(r"(^|[^A-Za-z0-9-])(--[a-z][a-z0-9-]*)").unwrap();
        let list = Regex::new(r"(^|[^A-Za-z0-9])@([a-z][a-z0-9-]*)").unwrap();
        let asked = Regex::new(r"prot-scriber\s+(defaults|doc|help)\s+([a-z][a-z0-9-]*)").unwrap();

        let mut wrong: Vec<String> = vec![];
        let (mut checked_in_prose, mut checked_in_commands) = (0, 0);
        for topic in TOPICS {
            // The prose, and the commands with their continuation lines joined.
            let mut prose = String::new();
            let mut commands: Vec<String> = vec![];
            let mut continued = false;
            for line in topic.text.lines() {
                if continued {
                    commands.last_mut().unwrap().push_str(line);
                } else if line.starts_with(char::is_whitespace) {
                    commands.push(line.to_string());
                } else {
                    prose.push_str(line);
                    prose.push('\n');
                }
                continued = line.ends_with('\\');
                if continued {
                    let command = commands.last_mut().unwrap();
                    command.pop();
                    command.push(' ');
                }
            }

            let named = |what: &str, known: &[String], name: &str, wrong: &mut Vec<String>| {
                if !known.iter().any(|k| k == name) {
                    wrong.push(format!(
                        "topic {} names {} {}, which does not exist",
                        topic.name, what, name
                    ));
                }
            };
            for caught in option.captures_iter(&prose) {
                checked_in_prose += 1;
                named("the option", &all_longs, &caught[2], &mut wrong);
            }
            for caught in list.captures_iter(&prose) {
                named("the list", &lists, &caught[2], &mut wrong);
            }
            let names_for = |verb: &str| -> Vec<String> {
                match verb {
                    "defaults" => built_ins.clone(),
                    "doc" => topics.iter().map(|t| t.to_string()).collect(),
                    _ => verbs.clone(),
                }
            };
            for caught in
                asked.captures_iter(&prose.split_whitespace().collect::<Vec<_>>().join(" "))
            {
                named(
                    &format!("'{}'", &caught[1]),
                    &names_for(&caught[1]),
                    &caught[2],
                    &mut wrong,
                );
            }

            for command in commands {
                let words = shell_words(&command);
                for program in words.split(|word| word == "|") {
                    if program.first().map(String::as_str) != Some("prot-scriber") {
                        continue; // another tool's command line, whose options are its own
                    }
                    let mut arguments = &program[1..];
                    let mut verb = &top;
                    if let Some(named_verb) =
                        arguments.first().and_then(|word| top.find_subcommand(word))
                    {
                        verb = named_verb;
                        if let Some(name) = arguments.get(1) {
                            if ["defaults", "doc", "help"].contains(&verb.get_name()) {
                                named(
                                    &format!("'{}'", verb.get_name()),
                                    &names_for(verb.get_name()),
                                    name,
                                    &mut wrong,
                                );
                            }
                        }
                        arguments = &arguments[1..];
                    }
                    let (longs, shorts) = declared(verb);
                    for word in arguments {
                        let flag = word.split('=').next().unwrap_or_default();
                        if flag.starts_with("--") {
                            checked_in_commands += 1;
                            named(
                                &format!("for '{}' the option", verb.get_name()),
                                &longs,
                                flag,
                                &mut wrong,
                            );
                        } else if flag.len() == 2 && flag.starts_with('-') && flag != "--" {
                            checked_in_commands += 1;
                            let short = flag.chars().nth(1).unwrap();
                            if !shorts.contains(&short) {
                                wrong.push(format!(
                                    "topic {} gives '{}' the option {}, which it does not declare",
                                    topic.name,
                                    verb.get_name(),
                                    flag
                                ));
                            }
                        }
                        if let Some(caught) = list.captures(word) {
                            named("the list", &lists, &caught[2], &mut wrong);
                        }
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        // A check that found nothing to check has checked nothing; these are what the topics hold
        // today, give or take, and neither is near zero.
        assert!(
            checked_in_prose > 20,
            "only {} options were read from prose",
            checked_in_prose
        );
        assert!(
            checked_in_commands > 20,
            "only {} options were read from commands",
            checked_in_commands
        );
    }

    /// The listing and the parser read one table: every name the parser accepts is listed, beside
    /// the title its topic opens with.
    #[test]
    fn the_listing_names_every_topic_the_parser_accepts() {
        let listed = listing();
        for topic in Topic::value_variants() {
            let name = topic.to_possible_value().unwrap().get_name().to_string();
            assert!(
                listed.lines().any(|line| {
                    line.split_whitespace().next() == Some(name.as_str())
                        && line.ends_with(topic.title())
                }),
                "{} is not listed beside its title:\n{}",
                name,
                listed
            );
        }
    }
}
