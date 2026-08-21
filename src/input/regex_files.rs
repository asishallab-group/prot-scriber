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
