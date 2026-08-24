//! Parsing of the regular expression files prot-scriber accepts on its command line, i.e. the
//! arguments `--blacklist-regexs`, `--filter-regexs` and `--capture-replace-pairs`. The regular
//! expressions parsed here replace prot-scriber's respective compiled in defaults (see
//! `crate::default`) and are applied in `crate::description`.

use crate::error::Error;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};

/// Reads in and parses a file specified by argument `path` and converts each line into an instance
/// of `Regex`. Returns a vector of the so instantiated regular expressions.
///
/// # Arguments
///
/// * `path` - A `&str` representing the path to the file containing one regular expression per
///   line.
pub fn parse_regex_file(path: &str) -> Result<Vec<Regex>, Error> {
    // Open stream to the file
    let file_path = path.to_string();
    let file = File::open(path)
        .map_err(|e| Error::opening(path, format!("No such file {:?}", path), &e))?;
    let reader = BufReader::new(file);

    // read file line by line
    let mut parsed_regexs = vec![];
    for (i, line) in reader.lines().enumerate() {
        let regex_line = line.map_err(|e| Error::reading(path, &e))?;

        match Regex::new(&regex_line) {
            Ok(regex) => {
                parsed_regexs.push(regex);
            }
            Err(e) => return Err(Error::MalformedData(format!("\n\n{:?} in file {:?} line <{:?}>. Could not parse the line into a Rust regular expression\n\n", e, file_path, i))),
        }
    }
    Ok(parsed_regexs)
}

/// Reads in and parses a file specified by argument `path` and converts each pair of lines into a
/// tupel `(fancy_regex::Regex, String)`. Returns a vector of the so instantiated tupels.
///
/// # Arguments
///
/// * `path` - A `&str` representing the path to the file containing pairs of lines. The first
///   always going to be parsed into a regular expression, the second returned as instance of
///   `String`.
pub fn parse_regex_replace_tuple_file(
    path: &str,
) -> Result<Vec<(fancy_regex::Regex, String)>, Error> {
    // Open stream to the file
    let file = File::open(path)
        .map_err(|e| Error::opening(path, format!("No such file {:?}", path), &e))?;
    let reader = BufReader::new(file);

    // Parse tuples, i.e. pairs of lines:
    let mut regex_replace_tuples: Vec<(fancy_regex::Regex, String)> = vec![];
    let mut is_regex_line = true;
    let mut regex_i: fancy_regex::Regex = fancy_regex::Regex::new("").unwrap();
    let mut n_lines = 0;
    for line in reader.lines() {
        let line_str = line.map_err(|e| Error::reading(path, &e))?;
        if is_regex_line {
            regex_i = fancy_regex::Regex::new(&line_str).map_err(|_| {
                Error::MalformedData(format!(
                    "Could not parse line {:?} as a regular expression (Rust syntax).",
                    line_str
                ))
            })?;
        } else {
            regex_replace_tuples.push((regex_i.clone(), line_str));
        }
        is_regex_line = !is_regex_line;
        n_lines += 1;
    }

    // If we have an odd number of lines, the file is malformed:
    if n_lines & 1 == 1 {
        return Err(Error::MalformedData(format!(
            "\n\n--capture-replace-pairs (-c) argument file {:?} has {:?} lines. But to construct pairs we need an even number of lines. See --help (-h) for more details.\n\n",
            path, n_lines
        )));
    }

    Ok(regex_replace_tuples)
}

#[cfg(test)]
mod tests {
    use super::{parse_regex_file, parse_regex_replace_tuple_file};
    use crate::default::{
        BLACKLIST_STITLE_REGEXS, CAPTURE_REPLACE_DESCRIPTION_PAIRS, FILTER_REGEXS,
        NON_INFORMATIVE_WORDS_REGEXS, POLISH_CAPTURE_REPLACE_PAIRS,
    };
    use pretty_assertions::assert_eq;

    /// The files under `misc/` are what the help text and the manual tell users to download when
    /// they want to start from "the default" and edit it. They are only worth that recommendation
    /// as long as they say what the binary actually does; a file that has fallen behind changes
    /// the annotation silently, because a shorter filter list is a perfectly valid filter list.
    ///
    /// Order matters as much as membership: the lists are applied as a left fold, so two
    /// expressions that both match the same description do not commute.
    #[test]
    fn the_shipped_regex_files_are_the_compiled_defaults() {
        for (path, compiled) in [
            (
                "misc/non_informative_words_regexs.txt",
                &*NON_INFORMATIVE_WORDS_REGEXS,
            ),
            ("misc/blacklist_stitle_regexs.txt", &*BLACKLIST_STITLE_REGEXS),
            ("misc/filter_stitle_regexs.txt", &*FILTER_REGEXS),
        ] {
            let parsed = parse_regex_file(path).unwrap();
            assert_eq!(
                parsed.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                compiled.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "{} has drifted away from the compiled default it claims to be",
                path
            );
        }
    }

    /// The same, for the two files holding pairs of lines. Here the replacement string is part of
    /// the payload, so a pair only matches if both of its lines do.
    #[test]
    fn the_shipped_capture_replace_files_are_the_compiled_defaults() {
        for (path, compiled) in [
            (
                "misc/capture_replace_pairs.txt",
                &*CAPTURE_REPLACE_DESCRIPTION_PAIRS,
            ),
            (
                "misc/polish_capture_replace_pairs.txt",
                &*POLISH_CAPTURE_REPLACE_PAIRS,
            ),
        ] {
            let parsed = parse_regex_replace_tuple_file(path).unwrap();
            assert_eq!(
                parsed
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
                compiled
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
                "{} has drifted away from the compiled default it claims to be",
                path
            );
        }
    }
}
