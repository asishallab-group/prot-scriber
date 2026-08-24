//! Parsing of the regular expression files prot-scriber accepts on its command line, i.e. the
//! arguments `--blacklist-regexs`, `--filter-regexs` and `--capture-replace-pairs`. The regular
//! expressions parsed here replace prot-scriber's respective compiled in defaults (see
//! `crate::default`) and are applied in `crate::description`.

use crate::error::Error;
use regex::Regex;
use std::fs::File;
use std::io::Read;

/// Reads the whole of the file at `path` into memory and parses it with `parse_regexs`.
///
/// # Arguments
///
/// * `path` - A `&str` representing the path to the file containing one regular expression per
///   line.
pub fn parse_regex_file(path: &str) -> Result<Vec<Regex>, Error> {
    parse_regexs(&slurp(path)?, path)
}

/// Converts each line of `content` into an instance of `Regex` and returns a vector of the so
/// instantiated regular expressions.
///
/// This is where prot-scriber's own default lists come from, too: they are the files under
/// `assets/`, compiled into the binary with `include_str!` and parsed right here. There is no
/// second copy of them written out as Rust literals, so a built-in list and the file that
/// documents it cannot say different things.
///
/// # Arguments
///
/// * `content` - The text to parse, one regular expression per line.
/// * `source` - What to name in an error message; a file path, or the built-in list's name.
pub fn parse_regexs(content: &str, source: &str) -> Result<Vec<Regex>, Error> {
    let mut parsed_regexs = vec![];
    for (i, regex_line) in content.lines().enumerate() {
        match Regex::new(regex_line) {
            Ok(regex) => {
                parsed_regexs.push(regex);
            }
            Err(e) => return Err(Error::MalformedData(format!("\n\n{:?} in file {:?} line <{:?}>. Could not parse the line into a Rust regular expression\n\n", e, source, i))),
        }
    }
    Ok(parsed_regexs)
}

/// Reads the file at `path` in full, distinguishing a file that is not there from one that cannot
/// be read.
///
/// The whole file is taken at once on purpose. These files hold a few dozen short lines, and
/// reading them line by line is how `-b <directory>` used to spin for ever: on Linux opening a
/// directory succeeds and only the read fails, and a line iterator hands back the same error
/// without ever advancing.
fn slurp(path: &str) -> Result<String, Error> {
    let mut file = File::open(path)
        .map_err(|e| Error::opening(path, format!("No such file {:?}", path), &e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| Error::reading(path, &e))?;
    Ok(content)
}

/// Reads the whole of the file at `path` into memory and parses it with
/// `parse_regex_replace_tuples`.
///
/// # Arguments
///
/// * `path` - A `&str` representing the path to the file containing pairs of lines.
pub fn parse_regex_replace_tuple_file(
    path: &str,
) -> Result<Vec<(fancy_regex::Regex, String)>, Error> {
    parse_regex_replace_tuples(&slurp(path)?, path)
}

/// Converts each pair of lines of `content` into a tuple `(fancy_regex::Regex, String)` and
/// returns a vector of the so instantiated tuples. The first line of a pair is parsed into a
/// regular expression, the second is the replacement it is applied with.
///
/// # Arguments
///
/// * `content` - The text to parse, in pairs of lines.
/// * `source` - What to name in an error message; a file path, or the built-in list's name.
pub fn parse_regex_replace_tuples(
    content: &str,
    source: &str,
) -> Result<Vec<(fancy_regex::Regex, String)>, Error> {
    // Parse tuples, i.e. pairs of lines:
    let mut regex_replace_tuples: Vec<(fancy_regex::Regex, String)> = vec![];
    let mut is_regex_line = true;
    let mut regex_i: fancy_regex::Regex = fancy_regex::Regex::new("").unwrap();
    let mut n_lines = 0;
    for line_str in content.lines() {
        if is_regex_line {
            regex_i = fancy_regex::Regex::new(line_str).map_err(|_| {
                Error::MalformedData(format!(
                    "Could not parse line {:?} as a regular expression (Rust syntax).",
                    line_str
                ))
            })?;
        } else {
            regex_replace_tuples.push((regex_i.clone(), line_str.to_string()));
        }
        is_regex_line = !is_regex_line;
        n_lines += 1;
    }

    // If we have an odd number of lines, the file is malformed:
    if n_lines & 1 == 1 {
        return Err(Error::MalformedData(format!(
            "\n\n--capture-replace-pairs (-c) argument file {:?} has {:?} lines. But to construct pairs we need an even number of lines. See --help (-h) for more details.\n\n",
            source, n_lines
        )));
    }

    Ok(regex_replace_tuples)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_regex_file, parse_regex_replace_tuple_file, parse_regex_replace_tuples, parse_regexs,
    };
    use crate::default::{
        BLACKLIST_STITLE_REGEXS, CAPTURE_REPLACE_DESCRIPTION_PAIRS, FILTER_REGEXS,
        NON_INFORMATIVE_WORDS_REGEXS, POLISH_CAPTURE_REPLACE_PAIRS,
    };
    use pretty_assertions::assert_eq;

    /// Forces every built-in list, which is what parses them, and pins how many expressions each
    /// one holds. The counts are here so that an `assets/` file losing lines -- to a bad merge, or
    /// to an editor that stops at the first blank line -- fails a test instead of quietly
    /// annotating with less filtering than it should.
    #[test]
    fn the_built_in_lists_parse() {
        assert_eq!(NON_INFORMATIVE_WORDS_REGEXS.len(), 10);
        assert_eq!(BLACKLIST_STITLE_REGEXS.len(), 11);
        assert_eq!(FILTER_REGEXS.len(), 25);
        assert_eq!(CAPTURE_REPLACE_DESCRIPTION_PAIRS.len(), 5);
        assert_eq!(POLISH_CAPTURE_REPLACE_PAIRS.len(), 1);
    }

    /// No built-in list repeats an expression. A repeat cannot change what prot-scriber produces
    /// -- the lists are folded with `replace_all`, so the second application of an expression
    /// finds nothing the first one left -- but it is a defect in a list that is published as
    /// documentation, and it costs a pass over every description of every hit.
    #[test]
    fn no_built_in_list_repeats_an_expression() {
        for (name, list) in [
            (
                "non_informative_words_regexs",
                &*NON_INFORMATIVE_WORDS_REGEXS,
            ),
            ("blacklist_stitle_regexs", &*BLACKLIST_STITLE_REGEXS),
            ("filter_stitle_regexs", &*FILTER_REGEXS),
        ] {
            let mut seen = std::collections::HashSet::new();
            for regex in list.iter() {
                assert!(
                    seen.insert(regex.as_str()),
                    "{} holds {:?} more than once",
                    name,
                    regex.as_str()
                );
            }
        }
    }

    /// A user who passes one of the shipped files back on the command line must get exactly the
    /// default. The two reach the parser by different routes -- one through `include_str!`, the
    /// other read off disk -- and this is what says the routes agree.
    #[test]
    fn passing_a_shipped_file_back_reproduces_the_default() {
        for (path, compiled) in [
            (
                "assets/non_informative_words_regexs.txt",
                &*NON_INFORMATIVE_WORDS_REGEXS,
            ),
            (
                "assets/blacklist_stitle_regexs.txt",
                &*BLACKLIST_STITLE_REGEXS,
            ),
            ("assets/filter_stitle_regexs.txt", &*FILTER_REGEXS),
        ] {
            assert_eq!(
                parse_regex_file(path)
                    .unwrap()
                    .iter()
                    .map(|r| r.as_str())
                    .collect::<Vec<_>>(),
                compiled.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "{} does not parse to the default it is compiled in as",
                path
            );
        }
    }

    /// The same for the two files holding pairs of lines, where the replacement string is part of
    /// the payload -- including a replacement that is the empty line.
    #[test]
    fn passing_a_shipped_pair_file_back_reproduces_the_default() {
        for (path, compiled) in [
            (
                "assets/capture_replace_pairs.txt",
                &*CAPTURE_REPLACE_DESCRIPTION_PAIRS,
            ),
            (
                "assets/polish_capture_replace_pairs.txt",
                &*POLISH_CAPTURE_REPLACE_PAIRS,
            ),
        ] {
            assert_eq!(
                parse_regex_replace_tuple_file(path)
                    .unwrap()
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
                compiled
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
                "{} does not parse to the default it is compiled in as",
                path
            );
        }
    }

    #[test]
    fn a_list_that_does_not_parse_is_malformed_data_and_names_its_source() {
        let e = parse_regexs("\\b\\w+\\b\n(unclosed\n", "my_regexs.txt").unwrap_err();
        assert_eq!(e.exit_code(), crate::error::EXIT_MALFORMED_INPUT);
        let message = format!("{}", e);
        assert!(message.contains("my_regexs.txt"), "{}", message);
        // The offending line is the second one, reported zero-based as it always has been:
        assert!(message.contains("line <1>"), "{}", message);
    }

    #[test]
    fn an_odd_number_of_lines_cannot_make_pairs() {
        let e = parse_regex_replace_tuples("\\s+\n \n\\d+\n", "my_pairs.txt").unwrap_err();
        assert_eq!(e.exit_code(), crate::error::EXIT_MALFORMED_INPUT);
        assert!(format!("{}", e).contains("my_pairs.txt"));
    }

    /// An empty list is a legitimate way of saying "do not filter at all", so it is not an error.
    #[test]
    fn an_empty_list_is_empty_rather_than_an_error() {
        assert!(parse_regexs("", "empty.txt").unwrap().is_empty());
        assert!(parse_regex_replace_tuples("", "empty.txt")
            .unwrap()
            .is_empty());
    }

    /// A file that is not there and a file that cannot be read are told apart, and neither is
    /// reported as a bug in prot-scriber. A path that simply does not exist is a mistake in the
    /// command line and earns the usage status; a path that exists but will not yield its contents
    /// is not something the user can correct by retyping.
    #[test]
    fn a_missing_file_and_an_unreadable_one_are_told_apart() {
        let missing = parse_regex_file("no/such/list.txt").unwrap_err();
        assert_eq!(missing.exit_code(), crate::error::EXIT_USAGE_ERROR);
        assert!(format!("{}", missing).contains("No such file"));

        // A directory opens successfully on Linux and only fails on the read. Reading it line by
        // line used to spin for ever; the whole file is taken at once now.
        let directory = parse_regex_file("assets").unwrap_err();
        assert_eq!(directory.exit_code(), crate::error::EXIT_IO_ERROR);
    }
}
