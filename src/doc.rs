//! The topics `prot-scriber doc` prints: what the options cannot say one at a time -- how to make
//! the input, which database needs which rule list, how to cluster gene families, and how to find
//! out why a description was chosen.
//!
//! Each topic is a text file in `src/doc/`, compiled in and printed exactly as it is written. Not
//! through clap's help renderer, which rewraps to the terminal: a topic holds command lines and
//! indented examples that are only right as written. So the files are written for a terminal 80
//! columns wide, which is the one width they cannot adapt to, and a test holds them to it.
//!
//! On a terminal a topic is STYLED, simply, to be easier to read: its title and every line a row
//! of '=' or '-' underlines in clap's header style; every command line -- marked `$ `, as a shell
//! shows one -- whole, in its literal style, so that commands stand apart from the text, as in
//! rustup's help; and in the listing the topic names in the literal style, as clap lists its
//! commands. Nothing inside a line is parsed: no quotes, pipes, options or verbs. The
//! styles are `cli::styles()`, the one definition the command is configured with. Anywhere else
//! -- a pipe, a file, `NO_COLOR` -- the topic is its file, byte for byte; `anstream` decides
//! which, as it does for clap.
//!
//! They are under `src/` because that is what a release is built from: a correction to a topic
//! ships with the next release, as a correction to the code does.
//!
//! A number or a name the code decides -- the score of a non-informative word, what an annotee
//! with no description is called -- is not written into a topic. The topic says `{{key}}`, and
//! `SCALARS` fills it in from the constant when the topic is printed, so the text cannot quote a
//! value the binary no longer has. Only scalars: what a shipped rule LIST contains is shown by
//! examples a test runs through the binary, never summarised.

use crate::default::{
    CENTER_AT_MEAN, MAX_MATCH_REPLACE_ITERATIONS, NON_INFORMATIVE_WORD_SCORE,
    UNKNOWN_FAMILY_DESCRIPTION, UNKNOWN_PROTEIN_DESCRIPTION,
};
use crate::error::Error;
use anstyle::Style;
use clap::builder::PossibleValue;
use clap::ValueEnum;
use std::io::Write;

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
    topic!("algorithm"),
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

    /// The whole topic, exactly as it is printed: its file, with every `{{key}}` filled in from
    /// `SCALARS`.
    pub fn text(&self) -> String {
        render(self.text)
    }
}

/// The values a topic may quote from the code, by the key it writes them as: `{{key}}`.
///
/// One table, so a renamed constant is a compile error here and nowhere else. Numbers are written
/// with `{}`, which for an `f64` never switches to exponent notation -- 0.000001 cannot become
/// 1e-6 -- and `every_scalar_reads_back_as_its_constant` holds each to the value it stands for.
fn scalars() -> [(&'static str, String); 5] {
    [
        ("non-informative-score", format!("{}", NON_INFORMATIVE_WORD_SCORE)),
        ("unknown-protein", UNKNOWN_PROTEIN_DESCRIPTION.to_string()),
        ("unknown-family", UNKNOWN_FAMILY_DESCRIPTION.to_string()),
        ("max-iterations", format!("{}", MAX_MATCH_REPLACE_ITERATIONS)),
        ("center-at-mean", format!("{}", CENTER_AT_MEAN)),
    ]
}

/// A topic's text with every `{{key}}` of `scalars` replaced by its value. A key the table does
/// not know is left as it stands, and `every_placeholder_is_a_scalar` fails on it.
///
/// # Arguments
///
/// * `text` - The topic as its file holds it.
fn render(text: &str) -> String {
    let mut rendered = text.to_string();
    for (key, value) in scalars() {
        rendered = rendered.replace(&format!("{{{{{}}}}}", key), &value);
    }
    rendered
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
    // A plain style renders as nothing at all, neither the style nor a reset, so the plain listing
    // is the styled one with no style: one copy of the listing, not two.
    listing_in(&Style::new(), &Style::new())
}

/// The listing, its opening line in `header` and the command and every topic's name in `literal`.
///
/// # Arguments
///
/// * `header` - The style of the opening line.
/// * `literal` - The style of what is typed: the command and the topics' names.
fn listing_in(header: &Style, literal: &Style) -> String {
    let width = TOPICS
        .iter()
        .map(|topic| topic.name.len())
        .max()
        .unwrap_or(0);
    let mut listed = format!(
        "{}\n\n    {} <TOPIC>\n\n",
        in_style(header, "prot-scriber's topics. Read one with"),
        in_style(literal, "prot-scriber doc")
    );
    for topic in TOPICS {
        // Padded by hand: a styled name is wider in bytes than on the screen.
        listed.push_str(&format!(
            "    {}{}  {}\n",
            in_style(literal, topic.name),
            " ".repeat(width - topic.name.len()),
            topic.title()
        ));
    }
    listed
}

/// The topics' names as prose -- 'input, databases, families and explain' -- for the pointer to
/// them in the help, which then names every topic there is and none there is not.
pub fn names() -> String {
    let names: Vec<&str> = TOPICS.iter().map(|topic| topic.name).collect();
    in_prose(&names)
}

/// Words listed as a sentence lists them: 'a', 'a and b', 'a, b and c'.
///
/// # Arguments
///
/// * `words` - What to list, in order.
pub fn in_prose(words: &[&str]) -> String {
    match words.split_last() {
        Some((last, [])) => last.to_string(),
        Some((last, rest)) => format!("{} and {}", rest.join(", "), last),
        None => String::new(),
    }
}

/// The two styles `doc` uses: the header style for headings, the literal style for the topic
/// names in the listing. The one definition, which the command is configured with too --
/// `the_command_is_styled_by_the_one_definition` says so -- so `doc` and `help` look alike.
fn clap_styles() -> (Style, Style) {
    let styles = crate::cli::styles();
    (*styles.get_header(), *styles.get_literal())
}

/// `text` in `style`, as clap writes a styled piece: the style, the text, the reset.
///
/// # Arguments
///
/// * `style` - The style.
/// * `text` - What to write in it.
fn in_style(style: &Style, text: &str) -> String {
    format!("{}{}{}", style.render(), text, style.render_reset())
}

/// Whether `line` underlines the line above it: a row of '=' or of '-', and nothing else.
///
/// # Arguments
///
/// * `line` - The line.
fn is_underline(line: &str) -> bool {
    line.len() >= 3 && (line.chars().all(|c| c == '=') || line.chars().all(|c| c == '-'))
}

/// What marks a command line: after its indentation, a line that begins with it is a command, as
/// a shell prompt shows one. The topics are ours, so every command in them is marked, on a line of
/// its own; the marker is printed with it, styled or not. Lines a trailing `\` continues it onto
/// carry no marker and belong to the command.
pub const COMMAND_MARKER: &str = "$ ";

/// Whether `line` begins a command: after its indentation, it opens with `COMMAND_MARKER`.
///
/// # Arguments
///
/// * `line` - The line.
fn opens_a_command(line: &str) -> bool {
    line.trim_start().starts_with(COMMAND_MARKER)
}

/// A topic as a terminal is shown it: every heading -- the title, and each line a row of '=' or
/// '-' underlines -- in the header style, with the underline dropped, since the style marks it;
/// and every command line, whole, in the literal style, its indentation left before the style.
/// Every other line is left as it is.
///
/// Nothing else changes: with the escape sequences taken out, this is the topic less its underline
/// rows, which `a_styled_topic_is_its_file_less_the_underlines` holds it to.
///
/// # Arguments
///
/// * `text` - The topic, rendered.
fn styled(text: &str) -> String {
    let (header, literal) = clap_styles();
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::with_capacity(text.len() + 256);
    // Whether the line before was a command line that a trailing backslash continues.
    let mut continued = false;
    for (i, line) in lines.iter().enumerate() {
        let next = lines.get(i + 1).copied().unwrap_or("");
        let heading =
            !line.is_empty() && !line.starts_with(char::is_whitespace) && is_underline(next);
        let underline = is_underline(line)
            && i > 0
            && !lines[i - 1].is_empty()
            && !lines[i - 1].starts_with(char::is_whitespace);
        if underline {
            continue;
        }
        let command = continued || opens_a_command(line);
        continued = command && line.trim_end().ends_with('\\');
        if heading {
            out.push_str(&in_style(&header, line));
        } else if command {
            let text = line.trim_start();
            out.push_str(&line[..line.len() - text.len()]);
            out.push_str(&in_style(&literal, text));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

/// The listing as a terminal is shown it: its opening line in the header style, and the command
/// and every topic's name in the literal style, as clap shows its list of commands.
fn styled_listing() -> String {
    let (header, literal) = clap_styles();
    listing_in(&header, &literal)
}

/// Writes a topic, or the listing when none is named, to standard output: styled where
/// `anstream` would pass styles through, and as written everywhere else.
///
/// # Arguments
///
/// * `topic` - Which topic to print, or `None` to list them.
pub fn print(topic: Option<Topic>) -> Result<(), Error> {
    let mut out = anstream::stdout().lock();
    // Asked of the stream, so that the choice is the one `anstream` makes for everything else it
    // writes: styles on a terminal, none in a pipe or a file or under NO_COLOR, and always under
    // CLICOLOR_FORCE. When it strips, the plain text is written -- underlines included -- since
    // stripping the styled text would lose them.
    let styles = out.current_choice() != anstream::ColorChoice::Never;
    let text = match (topic, styles) {
        (Some(topic), true) => styled(&topic.text()),
        (Some(topic), false) => topic.text(),
        (None, true) => styled_listing(),
        (None, false) => listing(),
    };
    // Flushed here, because a full disk behind a redirection must not look like success:
    out.write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| Error::Io(format!("\n\nCould not write the topic: {}\n\n", e)))
}

#[cfg(test)]
mod tests {
    use super::{listing, render, scalars, Topic, TOPICS};
    use crate::cli::{BuiltIn, Cli, DefaultList};
    use clap::{CommandFactory, ValueEnum};
    use regex::Regex;

    /// The widest a line may be. A topic is printed as written, so a longer line is broken by the
    /// terminal wherever it happens to fall -- mid-word, or in the middle of a command line that
    /// is meant to be copied.
    const COLUMNS: usize = 80;

    /// Every line of every topic, and of the listing, fits a terminal 80 columns wide -- as it is
    /// printed, with its `{{key}}`s filled in, which can make a line longer than its file's. Counted
    /// in characters, not bytes, because that is what a terminal counts: the en dashes in a
    /// citation are three bytes and one column. A TAB is refused outright, since how wide it is
    /// depends on where it stands.
    #[test]
    fn every_topic_fits_an_80_column_terminal() {
        let mut texts: Vec<(String, String)> = TOPICS
            .iter()
            .map(|topic| (format!("topic {}", topic.name), topic.text()))
            .collect();
        texts.push((String::from("the listing"), listing()));
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

    /// What a terminal shows of a styled topic fits it too: the escape sequences take no column,
    /// so each line is measured with them taken out.
    #[test]
    fn every_styled_topic_fits_an_80_column_terminal() {
        let escapes = Regex::new("\x1b\\[[0-9;]*m").unwrap();
        let mut texts: Vec<(String, String)> = TOPICS
            .iter()
            .map(|topic| (format!("topic {}", topic.name), super::styled(&topic.text())))
            .collect();
        texts.push((String::from("the listing"), super::styled_listing()));
        for (what, text) in texts {
            assert!(text.contains('\x1b'), "{} is not styled", what);
            for line in escapes.replace_all(&text, "").lines() {
                assert!(line.chars().count() <= COLUMNS, "{}: {:?}", what, line);
            }
        }
    }

    /// The command is configured with `cli::styles()`, the definition `doc` and the end of `-h`
    /// read directly, so the styles clap writes its help in and the ones they use are one thing.
    /// Read back from the built command, which is the other side.
    #[test]
    fn the_command_is_styled_by_the_one_definition() {
        let command = Cli::command();
        let configured = command.get_styles();
        assert_eq!(
            super::clap_styles(),
            (*configured.get_header(), *configured.get_literal())
        );
    }

    /// The plain listing holds no escape at all: a plain style renders as nothing, reset included,
    /// which is what lets it be the styled listing with no style.
    #[test]
    fn the_plain_listing_holds_no_escape() {
        assert!(!listing().contains('\x1b'), "{:?}", listing());
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
    /// marked with `COMMAND_MARKER` is a command -- the marker taken off -- joined with the lines a
    /// trailing `\` continues it onto. Of a command, only the programs that are `prot-scriber` --
    /// the first word, or the first word after a `|` -- are checked, and against the options of
    /// the verb they name; what Blast, Diamond, mcl, sed, awk or sort are given is theirs and is
    /// not read. Everything else is prose, in which every long option is prot-scriber's and must
    /// be declared by one of its commands. So a topic must not name another tool's long option in
    /// prose; quote it in a command instead. And a line that runs prot-scriber without the marker
    /// is an error: it would print as text, and be checked as prose.
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
            // The prose, and the commands with their continuation lines joined as a shell joins
            // them: the backslash and the line break go, and nothing takes their place. A
            // continuation line that is indented is a new word; one that is not continues the
            // word before it, which is how a quoted title too long for a line is broken.
            let mut prose = String::new();
            let mut commands: Vec<String> = vec![];
            let mut continued = false;
            for line in topic.text().lines() {
                let body = line.trim_start();
                if continued {
                    commands.last_mut().unwrap().push_str(line);
                } else if let Some(command) = body.strip_prefix(super::COMMAND_MARKER) {
                    commands.push(format!("  {}", command));
                } else {
                    let indented = line.starts_with(char::is_whitespace);
                    if indented && body.split_whitespace().next() == Some("prot-scriber") {
                        wrong.push(format!(
                            "topic {} runs prot-scriber without marking the line {:?}: {:?}",
                            topic.name,
                            super::COMMAND_MARKER,
                            line
                        ));
                    }
                    prose.push_str(line);
                    prose.push('\n');
                    continue;
                }
                continued = line.ends_with('\\');
                if continued {
                    commands.last_mut().unwrap().pop();
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

    /// Every `{{key}}` a topic writes is one `scalars` fills in, every scalar is written by some
    /// topic, and nothing that looks like a placeholder survives rendering. The first catches a
    /// misspelt key, which would otherwise be printed as it stands; the second a scalar nobody
    /// quotes any more, a table entry that would drift unnoticed.
    #[test]
    fn every_placeholder_is_a_scalar_and_every_scalar_is_used() {
        let placeholder = Regex::new(r"\{\{([^{}]*)\}\}").unwrap();
        let keys: Vec<&str> = scalars().iter().map(|(key, _)| *key).collect();
        let mut used: Vec<String> = vec![];
        for topic in TOPICS {
            for caught in placeholder.captures_iter(topic.text) {
                assert!(
                    keys.contains(&&caught[1]),
                    "topic {} writes {{{{{}}}}}, which no scalar fills in",
                    topic.name,
                    &caught[1]
                );
                used.push(caught[1].to_string());
            }
            let rendered = topic.text();
            // "{{" only: the awk program the families topic quotes closes two blocks with "}}".
            assert!(
                !rendered.contains("{{"),
                "topic {} still holds a placeholder once rendered",
                topic.name
            );
        }
        for key in keys {
            assert!(used.iter().any(|u| u == key), "no topic writes {{{{{}}}}}", key);
        }
    }

    /// A key written with one brace too few or too many -- `{non-informative-score}}`,
    /// `{{unknown-family}` -- is not a placeholder, so nothing fills it in, and it would be
    /// printed as it stands; the check above sees it only when the key happens to be used nowhere
    /// else. So every hyphenated lower-case word that touches a brace must be a well-formed
    /// `{{key}}`: the keys are all of that shape, and nothing else in the topics is -- `${name}` in
    /// a run plan has no hyphen, and the awk program's braces touch no word.
    #[test]
    fn every_brace_beside_a_key_shaped_word_makes_a_placeholder() {
        let braced = Regex::new(r"(\{*)([a-z]+(?:-[a-z]+)+)(\}*)").unwrap();
        let mut checked = 0;
        for topic in TOPICS {
            for caught in braced.captures_iter(topic.text) {
                if caught[1].is_empty() && caught[3].is_empty() {
                    continue;
                }
                checked += 1;
                assert!(
                    &caught[1] == "{{" && &caught[3] == "}}",
                    "topic {} writes {:?}, which is not a placeholder: write {{{{{}}}}}",
                    topic.name,
                    &caught[0],
                    &caught[2]
                );
            }
        }
        assert!(checked >= scalars().len(), "only {} placeholders were read", checked);
    }

    /// Each scalar reads back as the constant it stands for, so a formatting that rounded it, or
    /// wrote it in exponent notation, fails here rather than printing a different number.
    #[test]
    fn every_scalar_reads_back_as_its_constant() {
        use crate::default::{
            CENTER_AT_MEAN, MAX_MATCH_REPLACE_ITERATIONS, NON_INFORMATIVE_WORD_SCORE,
            UNKNOWN_FAMILY_DESCRIPTION, UNKNOWN_PROTEIN_DESCRIPTION,
        };
        let value = |key: &str| -> String {
            scalars().iter().find(|(k, _)| *k == key).unwrap().1.clone()
        };
        let score = value("non-informative-score");
        assert!(!score.contains('e'), "{} is in exponent notation", score);
        assert_eq!(score.parse::<f64>().unwrap(), NON_INFORMATIVE_WORD_SCORE);
        assert_eq!(value("center-at-mean").parse::<f64>().unwrap(), CENTER_AT_MEAN);
        assert_eq!(value("max-iterations").parse::<u8>().unwrap(), MAX_MATCH_REPLACE_ITERATIONS);
        assert_eq!(value("unknown-protein"), UNKNOWN_PROTEIN_DESCRIPTION);
        assert_eq!(value("unknown-family"), UNKNOWN_FAMILY_DESCRIPTION);
        assert_eq!(render("a {{center-at-mean}} b"), format!("a {} b", CENTER_AT_MEAN));
    }

    /// The lines `  <phrase>  ->  <polished>` of a section of a topic, from the line that opens
    /// with `from` to the one that opens with `to`.
    ///
    /// # Arguments
    ///
    /// * `text` - The topic.
    /// * `from` - How the section's heading begins.
    /// * `to` - How the next section's heading begins.
    fn examples_between(text: &str, from: &str, to: &str) -> Vec<(String, String)> {
        text.lines()
            .skip_while(|line| !line.starts_with(from))
            .take_while(|line| !line.starts_with(to))
            .filter(|line| line.starts_with("  "))
            .filter_map(|line| line.trim().split_once("  ->  "))
            .map(|(given, made)| (given.to_string(), made.to_string()))
            .collect()
    }

    /// The polishing examples of `doc algorithm` are what polishing does: each `<phrase>  ->
    /// <polished>` line of its step 5 is read from the topic, and the phrase is put through the
    /// built-in polish pairs by `apply_capture_replace_pairs`, the function the annotation polishes
    /// every description with (`AnnotationProcess::conclude`), with the list a run is given when
    /// it names none. A list edited so that an example no longer holds fails here, with what it
    /// makes of the phrase now.
    #[test]
    fn the_polishing_examples_are_what_polishing_does() {
        use crate::default::POLISH_CAPTURE_REPLACE_PAIRS;
        use crate::hrd::description::apply_capture_replace_pairs;
        let topic = TOPICS.iter().find(|topic| topic.name == "algorithm").unwrap();
        let examples = examples_between(&topic.text(), "Step 5", "An example");
        assert!(examples.len() >= 2, "only {} polishing examples were found", examples.len());
        for (phrase, polished) in examples {
            let mut made = phrase.clone();
            apply_capture_replace_pairs(&mut made, Some(&POLISH_CAPTURE_REPLACE_PAIRS));
            assert_eq!(made, polished, "polishing {:?}", phrase);
        }
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
