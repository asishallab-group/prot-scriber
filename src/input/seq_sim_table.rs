//! Parsing of the tabular sequence similarity search results (Blast or Diamond) prot-scriber is
//! given with its `--seq-sim-table` arguments, and `SeqSimTable`, the complete description of one
//! such input.

use crate::default::{
    BLACKLIST_STITLE_REGEXS, CAPTURE_REPLACE_DESCRIPTION_PAIRS, FILTER_REGEXS,
    SEQ_SIM_TABLE_COLUMNS, SSSR_TABLE_FIELD_SEPARATOR,
};
use crate::description::{filter_stitle, matches_blacklist};
use crate::error::Error;
use crate::input::regex_files::{parse_regex_file, parse_regex_replace_tuple_file};
use crate::model::query::Query;
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::sync::mpsc::Sender;

/// Everything prot-scriber needs in order to parse one input sequence similarity search result
/// table: where it is, and the five settings the user may give once per table on the command line.
/// A setting the user did not give is resolved to prot-scriber's compiled in default when this is
/// constructed, so a parsing thread receives a complete and self contained description of its
/// table. Note that this is what keeps a table from ever being parsed with another table's
/// settings: before, the five settings lived in five vectors parallel to the table paths, and were
/// found by shared index.
#[derive(Debug, Clone)]
pub struct SeqSimTable {
    /// The path to the tabular sequence similarity search result file to parse.
    pub path: String,
    /// The separator to use to split a line into an array of columns.
    pub field_separator: char,
    /// The column index in which to find the `qacc`.
    pub qacc_col: usize,
    /// The column index in which to find the `sacc`.
    pub sacc_col: usize,
    /// The column index in which to find the `stitle`.
    pub stitle_col: usize,
    /// The regular expressions used to identify to be discarded descriptions (`stitle`).
    pub blacklist_regexs: Vec<Regex>,
    /// The regular expressions used to identify to be deleted matching sub-strings in the
    /// descriptions (`stitle`).
    pub filter_regexs: Vec<Regex>,
    /// Tuples pairing a regular expression and a capture-group replacement string, applied
    /// iteratively to the descriptions to prepare them for splitting into words (see
    /// `crate::hrd::split_descriptions` for details).
    pub capture_replace_pairs: Vec<(fancy_regex::Regex, String)>,
}

impl SeqSimTable {
    /// Returns the argument `path` as an input table to be parsed with prot-scriber's compiled in
    /// defaults for all of its settings. Use the `set_*` functions to override those the user
    /// provided a command line argument for.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the tabular sequence similarity search result file to parse.
    pub fn new(path: String) -> SeqSimTable {
        SeqSimTable {
            path,
            field_separator: SSSR_TABLE_FIELD_SEPARATOR,
            qacc_col: *(*SEQ_SIM_TABLE_COLUMNS).get("qacc").unwrap(),
            sacc_col: *(*SEQ_SIM_TABLE_COLUMNS).get("sacc").unwrap(),
            stitle_col: *(*SEQ_SIM_TABLE_COLUMNS).get("stitle").unwrap(),
            blacklist_regexs: (*BLACKLIST_STITLE_REGEXS).clone(),
            filter_regexs: (*FILTER_REGEXS).clone(),
            capture_replace_pairs: (*CAPTURE_REPLACE_DESCRIPTION_PAIRS).clone(),
        }
    }

    /// Parses a `--header` (`-e`) command line argument into the column indices of `qacc`, `sacc`
    /// and `stitle`, and stores them. Uses `default::SEQ_SIM_TABLE_COLUMNS` if the argument equals
    /// `"default"` (case insensitive). Fails if any of the three required columns is absent.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of SeqSimTable.
    /// * `header_arg` - The passed header argument.
    /// * `arg_number` - The one based position of `header_arg` among the `--header` arguments,
    ///   used only to point the user at the offending one.
    pub fn set_columns(&mut self, header_arg: &str, arg_number: usize) -> Result<(), Error> {
        let columns: HashMap<String, usize> = if header_arg.trim().to_lowercase() == "default" {
            (*SEQ_SIM_TABLE_COLUMNS).clone()
        } else {
            header_arg
                .trim()
                .split(' ')
                .filter(|x| !x.is_empty())
                .enumerate()
                .map(|(i, col_name)| (col_name.to_string(), i))
                .collect()
        };
        for required in ["qacc", "sacc", "stitle"] {
            if !columns.contains_key(required) {
                return Err(Error::Usage(format!(
                    "\n\nCannot run Annotation-Process, because --header (-e) argument number {} does not contain required column {:?}!\n\n",
                    arg_number, required
                )));
            }
        }
        self.qacc_col = *columns.get("qacc").unwrap();
        self.sacc_col = *columns.get("sacc").unwrap();
        self.stitle_col = *columns.get("stitle").unwrap();
        Ok(())
    }

    /// Parses a `--field-separator` (`-p`) command line argument into the `char` used to split a
    /// row of this table into fields. Keeps `default::SSSR_TABLE_FIELD_SEPARATOR` if the argument
    /// equals `"default"` (case insensitive).
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of SeqSimTable.
    /// * `field_separator_arg` - The passed field-separator argument.
    pub fn set_field_separator(&mut self, field_separator_arg: &str) -> Result<(), Error> {
        if field_separator_arg.trim().to_lowercase() != "default" {
            self.field_separator = field_separator_arg.chars().next().ok_or_else(|| {
                Error::Usage(String::from(
                    "\n\nCannot run Annotation-Process, because a --field-separator (-p) argument is the empty string. Please provide the character that separates the fields of the respective input table, or 'default' for the '<TAB>' character.\n\n",
                ))
            })?;
        }
        Ok(())
    }

    /// Parses a `--blacklist-regexs` (`-b`) command line argument, i.e. reads the regular
    /// expressions used to identify to be discarded descriptions (`stitle`) from the argument
    /// file. Keeps `default::BLACKLIST_STITLE_REGEXS` if the argument equals `"default"` (case
    /// insensitive).
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of SeqSimTable.
    /// * `blacklist_regexs_arg` - The passed command line argument.
    pub fn set_blacklist_regexs(&mut self, blacklist_regexs_arg: &str) -> Result<(), Error> {
        if blacklist_regexs_arg.trim().to_lowercase() != "default" {
            self.blacklist_regexs = parse_regex_file(blacklist_regexs_arg)?;
        }
        Ok(())
    }

    /// Parses a `--filter-regexs` (`-l`) command line argument, i.e. reads the regular expressions
    /// used to identify to be deleted sub-strings of the descriptions (`stitle`) from the argument
    /// file. Keeps `default::FILTER_REGEXS` if the argument equals `"default"` (case insensitive).
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of SeqSimTable.
    /// * `filter_regexs_arg` - The passed command line argument.
    pub fn set_filter_regexs(&mut self, filter_regexs_arg: &str) -> Result<(), Error> {
        if filter_regexs_arg.trim().to_lowercase() != "default" {
            self.filter_regexs = parse_regex_file(filter_regexs_arg)?;
        }
        Ok(())
    }

    /// Parses a `--capture-replace-pairs` (`-c`) command line argument, i.e. reads the pairs of
    /// regular expression and capture-group replacement string from the argument file. Keeps
    /// `default::CAPTURE_REPLACE_DESCRIPTION_PAIRS` if the argument equals `"default"` (case
    /// insensitive).
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of SeqSimTable.
    /// * `capture_replace_pairs_arg` - The passed command line argument.
    pub fn set_capture_replace_pairs(&mut self, capture_replace_pairs_arg: &str) -> Result<(), Error> {
        if capture_replace_pairs_arg.trim().to_lowercase() != "default" {
            self.capture_replace_pairs = parse_regex_replace_tuple_file(capture_replace_pairs_arg)?;
        }
        Ok(())
    }
}

/// What a parsing thread sends back to the `AnnotationProcess` that started it. A parsing failure
/// has to travel this way rather than end the thread: a thread that dies takes its diagnosis with
/// it, and the run it was working for goes on to report success having annotated nothing.
#[derive(Debug)]
pub enum ParseMessage {
    /// A query whose hits have all been read, under its identifier.
    Query(String, Query),
    /// This table has been read to its end, and `records` of its lines held the three required
    /// columns. The count is reported even -- especially -- when it is zero: a table that yields
    /// no record was not the table the command line described, and no later stage of the run can
    /// tell that apart from a table whose hits were all uninformative.
    TableRead { path: String, records: usize },
    /// The one reason this table could not be parsed, after which the thread sends nothing more.
    Failed(Error),
}

/// Finds a tabular file (`table.path`) and parses it in a stream approach, i.e. line by line.
/// Every time an instance of Query is successfully and completely parsed it is send using the
/// argument `transmitter` to the respective registered receiver.
///
/// # Arguments
///
/// * `table` - The input sequence similarity search result table to parse, and the settings to
///   parse it with.
/// * `transmitter: Sender<ParseMessage>` - Used to send instances of `Query`, the number of
///   records this table held, or the failure that ended the parsing, to any receiver.
pub fn parse_table(table: &SeqSimTable, transmitter: Sender<ParseMessage>) {
    let lines = match read_lines(&table.path) {
        Ok(lines) => lines,
        Err(e) => {
            let _ = transmitter.send(ParseMessage::Failed(Error::opening(
                &table.path,
                format!("An error occurred reading file {:?}", table.path),
                &e,
            )));
            return;
        }
    };
    let mut records: usize = 0;
    let mut last_qacc = String::new();
    let mut curr_query = Query::new();
    for (line_number, line_rslt) in lines.enumerate() {
        match line_rslt {
            Ok(line) => {
                let cols: Vec<&str> = line.trim().split(table.field_separator).collect();
                // A line that has no field where one of the three required columns should be
                // means the table is not the table the arguments describe -- most often because
                // the --field-separator (-p) is not the one the table actually uses, in which
                // case every line collapses into a single field. There is nothing to salvage
                // from the rest of the file, so report it and stop:
                let (qacc, sacc, stitle) = match (
                    cols.get(table.qacc_col),
                    cols.get(table.sacc_col),
                    cols.get(table.stitle_col),
                ) {
                    (Some(qacc), Some(sacc), Some(stitle)) => (*qacc, *sacc, *stitle),
                    _ => {
                        let _ = transmitter.send(ParseMessage::Failed(Error::MalformedData(format!(
                            "\n\nCannot parse file {:?}, because line {} splits into {} field(s) using the field-separator {:?}, which is too few to hold the required columns 'qacc', 'sacc' and 'stitle'. Please check the --field-separator (-p) and --header (-e) arguments given for this table.\n\n",
                            table.path,
                            line_number + 1,
                            cols.len(),
                            table.field_separator
                        ))));
                        return;
                    }
                };
                records += 1;

                if qacc != last_qacc && !last_qacc.is_empty() {
                    transmitter
                        .send(ParseMessage::Query(last_qacc, curr_query))
                        .unwrap();
                    curr_query = Query::new();
                }

                if !curr_query.hits.contains_key(sacc)
                    && !matches_blacklist(stitle, &table.blacklist_regexs)
                {
                    let desc = filter_stitle(
                        stitle,
                        &table.filter_regexs,
                        Some(&table.capture_replace_pairs),
                    )
                    .trim()
                    .to_lowercase();
                    if !desc.is_empty() {
                        curr_query.hits.insert(sacc.to_string(), desc);
                    }
                }

                last_qacc = qacc.to_string();
            }
            Err(e) => {
                eprintln!(
                    "\nAn error occurred while parsing {:?}:\n{:?}\nContinuing anyway!\n",
                    table.path, e
                );
            }
        }
    }

    // Send last parsed query:
    if !curr_query.hits.is_empty() && !last_qacc.is_empty() {
        transmitter
            .send(ParseMessage::Query(last_qacc, curr_query))
            .unwrap();
    }

    // And say how much of this table was read, which is the only report that still arrives when
    // there was nothing in it:
    transmitter
        .send(ParseMessage::TableRead {
            path: table.path.clone(),
            records,
        })
        .unwrap();
}

/// The output is wrapped in a Result to allow matching on errors Returns an Iterator to the Reader
/// of the lines of the file.
///
/// # Arguments
///
/// * `filename` The path to the file to open a `BufReader` for.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
