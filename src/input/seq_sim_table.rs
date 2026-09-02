//! Parsing of the tabular sequence similarity search results (Blast or Diamond) prot-scriber is
//! given with its `--seq-sim-table` arguments, and `SeqSimTable`, the complete description of one
//! such input.

use crate::default::{
    BLACKLIST_STITLE_REGEXS, CAPTURE_REPLACE_DESCRIPTION_PAIRS, FILTER_REGEXS,
    SEQ_SIM_TABLE_COLUMNS, SSSR_TABLE_FIELD_SEPARATOR,
};
use crate::description::{filter_stitle, first_blacklist_match, Steps};
use crate::input::list_fit::ListFit;
use crate::error::Error;
use crate::input::regex_files::{
    parse_pair_file, parse_pairs, parse_rule_file, parse_rules, PairList, RuleList,
};
use crate::model::query::Query;
use std::borrow::Cow;
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
/// Which of a table's three rule lists an expression belongs to.
///
/// Declared here, with the lists it names, rather than beside its first caller: `append_rule` below
/// takes one, and `explain::compare` -- which was where it used to live -- takes a `SeqSimTable`.
/// The two modules referring to each other was a cycle, and an inline `crate::explain::compare::`
/// path rather than a `use`, so no grep of the imports showed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Blacklist,
    Filter,
    CaptureReplace,
}

impl Stage {
    /// The stage of this name, or a usage error naming the three there are.
    ///
    /// # Arguments
    ///
    /// * `name` - The stage as the user wrote it.
    /// * `whole` - The whole argument, for the error message.
    pub fn parse(name: &str, whole: &str) -> Result<Stage, Error> {
        match name {
            "blacklist" => Ok(Stage::Blacklist),
            "filter" => Ok(Stage::Filter),
            "capture-replace" => Ok(Stage::CaptureReplace),
            _ => Err(Error::Usage(format!(
                "\n\nCannot read {:?}: {:?} is not a stage. The stages a rule can belong to are \
                 'blacklist', 'filter' and 'capture-replace'.\n\n",
                whole, name
            ))),
        }
    }
}

/// Which column of a table holds what, once a header specification has been checked.
///
/// The check is a question about the ARGUMENT -- does this list of column names include the three
/// prot-scriber needs -- and it is answerable before any file is opened, so it is asked by the
/// value parser behind `--db-header` and `--header`. What arrives here has already passed.
///
/// The knowledge of WHICH columns a table must have, and that Diamond spells two of them
/// differently, stays here with the table; only the reporting moved. That is what lets the message
/// name no flag: there are two flags for this one setting and clap names whichever was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The column index in which to find the `qacc`.
    pub qacc_col: usize,
    /// The column index in which to find the `sacc`.
    pub sacc_col: usize,
    /// The column index in which to find the `stitle`.
    pub stitle_col: usize,
    /// How many columns were named, and therefore how many fields a row must split into.
    pub columns: usize,
}

impl Header {
    /// The columns a table has if its header says nothing: `default::SEQ_SIM_TABLE_COLUMNS`.
    pub fn compiled_in() -> Header {
        Header {
            qacc_col: *(*SEQ_SIM_TABLE_COLUMNS).get("qacc").unwrap(),
            sacc_col: *(*SEQ_SIM_TABLE_COLUMNS).get("sacc").unwrap(),
            stitle_col: *(*SEQ_SIM_TABLE_COLUMNS).get("stitle").unwrap(),
            columns: (*SEQ_SIM_TABLE_COLUMNS).len(),
        }
    }

    /// The columns a header specification names, or why it will not do.
    ///
    /// The error names no option, because two of them carry this: clap says which was written.
    ///
    /// # Arguments
    ///
    /// * `spec` - The column names in order, space separated, or "default".
    pub fn parse(spec: &str) -> Result<Header, String> {
        if spec.trim().eq_ignore_ascii_case("default") {
            return Ok(Header::compiled_in());
        }
        // The NAMES, in order, before they become a map: two of them can canonicalise to one key
        // (`qacc` and `qseqid`), and it is the count of columns the user declared -- not the count
        // of distinct ones -- that a row has to match.
        let names: Vec<&str> = spec.trim().split(' ').filter(|x| !x.is_empty()).collect();
        let columns: HashMap<String, usize> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (canonical_column_name(name).to_string(), i))
            .collect();
        for required in ["qacc", "sacc", "stitle"] {
            if !columns.contains_key(required) {
                return Err(format!(
                    "does not name the required column {:?}{}",
                    required,
                    match required {
                        "qacc" => " (or 'qseqid', as Diamond calls it)",
                        "sacc" => " (or 'sseqid', as Diamond calls it)",
                        _ => "",
                    }
                ));
            }
        }
        Ok(Header {
            qacc_col: *columns.get("qacc").unwrap(),
            sacc_col: *columns.get("sacc").unwrap(),
            stitle_col: *columns.get("stitle").unwrap(),
            columns: names.len(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SeqSimTable {
    /// What the user calls this table on the command line: the name given as `--db NAME=PATH`, or
    /// the file's own name when it was declared as a bare path. Every per-table `--db-*` option
    /// finds its table by this, rather than by the position it was written in.
    pub name: String,
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
    /// How many columns the header names, and therefore how many fields a row of this table must
    /// split into.
    ///
    /// The three indices above say where to look; this says what the table was declared to BE, and
    /// only the two together can catch a column that is in the wrong place rather than missing.
    /// `-f 6 qseqid sseqid evalue stitle` reaches `stitle_col` = 2 perfectly happily and finds an
    /// e-value there.
    pub columns: usize,
    /// The regular expressions used to identify to be discarded descriptions (`stitle`).
    pub blacklist_regexs: RuleList,
    /// The regular expressions used to identify to be deleted matching sub-strings in the
    /// descriptions (`stitle`).
    pub filter_regexs: RuleList,
    /// Tuples pairing a regular expression and a capture-group replacement string, applied
    /// iteratively to the descriptions to prepare them for splitting into words (see
    /// `crate::hrd::split_descriptions` for details).
    pub capture_replace_pairs: PairList,
}

impl SeqSimTable {
    /// Returns the argument `path` as an input table to be parsed with prot-scriber's compiled in
    /// defaults for all of its settings. Use the `set_*` functions to override those the user
    /// provided a command line argument for.
    ///
    /// # Arguments
    ///
    /// * `name` - What the user calls this table on the command line.
    /// * `path` - The path to the tabular sequence similarity search result file to parse.
    pub fn new(name: String, path: String) -> SeqSimTable {
        SeqSimTable {
            name,
            path,
            field_separator: SSSR_TABLE_FIELD_SEPARATOR,
            qacc_col: Header::compiled_in().qacc_col,
            sacc_col: Header::compiled_in().sacc_col,
            stitle_col: Header::compiled_in().stitle_col,
            columns: Header::compiled_in().columns,
            blacklist_regexs: (*BLACKLIST_STITLE_REGEXS).clone(),
            filter_regexs: (*FILTER_REGEXS).clone(),
            capture_replace_pairs: (*CAPTURE_REPLACE_DESCRIPTION_PAIRS).clone(),
        }
    }

    /// Which column holds what, from a header specification already checked.
    ///
    /// Infallible, and that is the point: whether a header names the three required columns is a
    /// question about the argument, not about the data, and it is answered by the value parser
    /// behind `--db-header` before any file is opened.
    ///
    /// # Arguments
    ///
    /// * `header` - The columns, already checked.
    pub fn set_header(&mut self, header: &Header) {
        self.qacc_col = header.qacc_col;
        self.sacc_col = header.sacc_col;
        self.stitle_col = header.stitle_col;
        self.columns = header.columns;
    }

    /// Adds one candidate expression to the end of the list it belongs to.
    ///
    /// For `explain --try`, which measures a rule without writing it anywhere. The candidate is
    /// named `--try` in the report rather than given a file and a line it does not have.
    ///
    /// # Arguments
    ///
    /// * `stage` - Which list it belongs to.
    /// * `expression` - The candidate, as the user wrote it.
    /// * `origin` - What to call it in the report, since it has no file and no line. The caller's
    ///   word: it is the name of the option the candidate arrived on, and this module does not
    ///   know the command line.
    pub fn append_rule(
        &mut self,
        stage: Stage,
        expression: &str,
        origin: &str,
    ) -> Result<(), Error> {
        match stage {
            Stage::Blacklist => self.blacklist_regexs.push_rule(expression, origin),
            Stage::Filter => self.filter_regexs.push_rule(expression, origin),
            Stage::CaptureReplace => self.capture_replace_pairs.push_pair(expression, origin),
        }
    }

    /// The character rows of this table split on.
    ///
    /// Infallible, and that is the point: whether a separator is ONE character is a question about
    /// the argument, not about the data, and it is now answered by the value parser behind
    /// `--db-sep` before any file is opened. A table cannot be handed an invalid one.
    ///
    /// # Arguments
    ///
    /// * `separator` - The character, already validated.
    pub fn set_field_separator(&mut self, separator: char) {
        self.field_separator = separator;
    }

    /// Which of prot-scriber's own filter lists this table is prepared with, so that reading it
    /// can say when the titles want a different one.
    ///
    /// `None` when the list is the user's own file, or `none`, or a replayed run plan's -- none of
    /// which prot-scriber is in a position to second-guess. See `crate::input::list_fit`.
    ///
    /// Asked of the list rather than remembered beside it. `RuleList` already records where every
    /// expression came from, and `parse_rules` drops the leading `@`, so a list reached as
    /// `@filter-regexs-pdb` and the same list reached as the default answer identically. Kept as a
    /// second field it was a fact stored twice, and the copy that plan replay did not set was the
    /// one that made a replayed run warn about a list it had never been given.
    pub fn filter_list_name(&self) -> Option<String> {
        self.filter_regexs
            .origin(0)
            .and_then(|origin| crate::assets::DefaultList::from_name(&origin.list))
            .map(|list| list.name())
    }

    /// The description one hit contributes to the annotation of its query, or `None` when it
    /// contributes none: the blacklist discarded its title, or the expressions left nothing of it.
    ///
    /// This is the whole of what happens to a sequence title, and it is here so that it is said
    /// once. It used to be said twice -- here, and again in `prot-scriber explain --stitle`, which
    /// exists to report it -- and the two had already come apart over the lower-casing that
    /// follows the capture-replace pairs. An account of what prot-scriber does is worth having
    /// only if it is produced by the code that does it.
    ///
    /// # Arguments
    ///
    /// * `&self` - This table, for the rules it is parsed with.
    /// * `stitle` - The sequence title as the search result carries it.
    /// * `steps` - Where to record what happened, or `None` to do the work and say nothing.
    pub fn hit_description(&self, stitle: &str, mut steps: Option<&mut Steps>) -> Option<String> {
        let (discarded_by, blacklist_checked) =
            first_blacklist_match(stitle, &self.blacklist_regexs);
        if let Some(steps) = steps.as_deref_mut() {
            steps.blacklist_checked = blacklist_checked;
        }
        if let Some(discarded_by) = discarded_by {
            if let Some(steps) = steps.as_deref_mut() {
                steps.discarded_by = Some(discarded_by);
            }
            return None;
        }
        let description = filter_stitle(
            stitle,
            &self.filter_regexs,
            Some(&self.capture_replace_pairs),
            steps.as_deref_mut(),
        );
        if description.is_empty() {
            if let Some(steps) = steps {
                steps.emptied = true;
            }
            return None;
        }
        Some(description)
    }

    /// Parses a `--db-blacklist` command line argument, i.e. reads the regular
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
            self.blacklist_regexs = crate::assets::resolve(blacklist_regexs_arg, parse_rule_file, parse_rules)?;
        }
        Ok(())
    }

    /// Parses a `--db-filter` command line argument, i.e. reads the regular expressions
    /// used to identify to be deleted sub-strings of the descriptions (`stitle`) from the argument
    /// file. Keeps `default::FILTER_REGEXS` if the argument equals `"default"` (case insensitive).
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of SeqSimTable.
    /// * `filter_regexs_arg` - The passed command line argument.
    pub fn set_filter_regexs(&mut self, filter_regexs_arg: &str) -> Result<(), Error> {
        let source = filter_regexs_arg.trim();
        if !source.eq_ignore_ascii_case("default") {
            self.filter_regexs = crate::assets::resolve(source, parse_rule_file, parse_rules)?;
        }
        Ok(())
    }

    /// Parses a `--db-capture-replace` command line argument, i.e. reads the pairs of
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
            self.capture_replace_pairs =
                crate::assets::resolve(capture_replace_pairs_arg, parse_pair_file, parse_pairs)?;
        }
        Ok(())
    }
}

/// The Blast name of a column, given either the Blast or the Diamond spelling of it.
///
/// Diamond calls them `qseqid` and `sseqid`, and a Diamond user's own `-f 6 qseqid sseqid stitle`
/// is the obvious thing to paste into `--header`. It used to be refused, and the help apologised
/// for it -- "even if you ran Diamond, please provide 'qacc'" -- for no reason except that the map
/// was built from the words as typed.
///
/// # Arguments
///
/// * `column` - A column name as the user wrote it.
fn canonical_column_name(column: &str) -> &str {
    match column {
        "qseqid" => "qacc",
        "sseqid" => "sacc",
        other => other,
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
    /// `digest` is the BLAKE3 hash of everything the thread read, taken as the bytes went past so
    /// that recording *which* data was annotated costs no second pass over the file.
    TableRead {
        path: String,
        records: usize,
        digest: String,
    },
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
    // Lines whose bytes are not valid UTF-8, and the first of them. A hit is data a sequence
    // similarity search was run to obtain, and BLAST and DIAMOND titles do carry latin-1 bytes, so
    // such a line is decoded with the offending characters replaced rather than dropped or treated
    // as a failure. What must not happen is that it goes unmentioned, hence the count.
    let mut undecodable_lines: usize = 0;
    let mut first_undecodable_line: usize = 0;
    let mut raw: Vec<u8> = Vec::new();
    // Whether the filter list fits these titles, decided from the first few thousand of them. See
    // `crate::input::list_fit`; it costs nothing on a table of any size, because the sample is.
    let mut fit = ListFit::new(table.filter_list_name());
    let mut lines = lines;
    let mut digest = blake3::Hasher::new();
    for line_number in 0.. {
        raw.clear();
        match lines.read_until(b'\n', &mut raw) {
            Ok(0) => break,
            Ok(_) => {
                digest.update(&raw);
                let decoded = String::from_utf8_lossy(&raw);
                if matches!(decoded, Cow::Owned(_)) {
                    undecodable_lines += 1;
                    if first_undecodable_line == 0 {
                        first_undecodable_line = line_number + 1;
                    }
                }
                let line: &str = &decoded;
                // THE LINE ENDING ONLY, not the whitespace. A trailing TAB is the last field, and
                // an empty last field is a hit with no description -- 15.2 % of the GenPept rows in
                // this project's benchmark, because many CDS features have no /product. Trimming
                // the row before splitting it loses that field, and the row then looks one column
                // short to the check below. The three values are trimmed where they are read
                // instead, which is what the trim was ever wanted for.
                let row: &str = line.trim_end_matches(['\r', '\n']);
                let cols: Vec<&str> = row.split(table.field_separator).collect();
                // THE HEADER HAS TO FIT THE TABLE. A row that splits into a different number of
                // fields than the header names is the table not being the table the arguments
                // describe -- and it is not enough to check that the three required indices exist,
                // because extra columns in FRONT of the description leave them all present and
                // pointing at the wrong things. `-f 6 qseqid sseqid evalue stitle` under the
                // three-column default read every e-value as a description and exited 0.
                if cols.len() != table.columns {
                    let _ = transmitter.send(ParseMessage::Failed(Error::MalformedData(format!(
                        "\n\nCannot parse file {:?} of table {:?}, because line {} splits into {} field(s) using the field-separator {:?}, while the header names {}. The header has to fit the table: name every column it has, in order, e.g. 'qacc sacc evalue stitle'. A column prot-scriber does not read still has to be named, because a name is what puts the description in the right place -- unnamed, an extra column in front of it silently shifts everything after it. If the table is not TAB separated, the separator given for it is wrong too.\n\n",
                        table.path,
                        table.name,
                        line_number + 1,
                        cols.len(),
                        table.field_separator,
                        table.columns
                    ))));
                    return;
                }
                // A line that has no field where one of the three required columns should be
                // means the table is not the table the arguments describe -- most often because
                // the field separator is not the one the table actually uses, in which
                // case every line collapses into a single field. There is nothing to salvage
                // from the rest of the file, so report it and stop:
                let (qacc, sacc, stitle) = match (
                    cols.get(table.qacc_col),
                    cols.get(table.sacc_col),
                    cols.get(table.stitle_col),
                ) {
                    (Some(qacc), Some(sacc), Some(stitle)) => {
                        (qacc.trim(), sacc.trim(), stitle.trim())
                    }
                    _ => {
                        let _ = transmitter.send(ParseMessage::Failed(Error::MalformedData(format!(
                            "\n\nCannot parse file {:?}, because line {} splits into {} field(s) using the field-separator {:?}, which is too few to hold the required columns 'qacc', 'sacc' and 'stitle'. Either that separator is not the one this table uses, or the header given for it does not describe this table.\n\n",
                            table.path,
                            line_number + 1,
                            cols.len(),
                            table.field_separator
                        ))));
                        return;
                    }
                };
                records += 1;
                if fit.wants() {
                    fit.observe(stitle, &table.filter_regexs);
                }

                if qacc != last_qacc && !last_qacc.is_empty() {
                    transmitter
                        .send(ParseMessage::Query(last_qacc, curr_query))
                        .unwrap();
                    curr_query = Query::new();
                }

                if !curr_query.hits.contains_key(sacc) {
                    if let Some(desc) = table.hit_description(stitle, None) {
                        curr_query.hits.insert(sacc.to_string(), desc);
                    }
                }

                last_qacc = qacc.to_string();
            }
            Err(e) => {
                // Not "continue anyway": a read error does not advance the reader, so asking for
                // the next line returns the same error for ever. A directory given as an input
                // table reaches exactly that, because opening one succeeds and only reading it
                // fails. Report it and stop, as every other file this program reads already does.
                let _ = transmitter.send(ParseMessage::Failed(Error::reading(&table.path, &e)));
                return;
            }
        }
    }

    if let Some(warning) = fit.report(&format!("table {:?}", table.name)) {
        eprint!("{}", warning);
    }

    if undecodable_lines > 0 {
        eprintln!(
            "\nWarning: {} line(s) of {:?} are not valid UTF-8, the first at line {}. Those \
             characters were replaced, so the hits were kept but their descriptions may differ \
             from what the search wrote. Convert the table first to avoid this, e.g. with\n  \
             iconv -f latin1 -t utf8 {:?} > converted.tsv\n",
            undecodable_lines, table.path, first_undecodable_line, table.path
        );
    }

    // Send the last parsed query. NOT `&& !curr_query.hits.is_empty()`: the send at the row where
    // the query identifier changes does not test that, so adding it here made the last query of a
    // table the one case where losing every hit to the blacklist lost the row as well, instead of
    // producing the "unknown protein" the caller expects and `-x` exists to remove. An empty
    // `last_qacc` still stops this, and that is the real guard -- it means the table held no rows.
    if !last_qacc.is_empty() {
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
            digest: digest.finalize().to_hex().to_string(),
        })
        .unwrap();
}

/// The output is wrapped in a Result to allow matching on errors Returns an Iterator to the Reader
/// of the lines of the file.
///
/// # Arguments
///
/// * `filename` The path to the file to open a `BufReader` for.
fn read_lines<P>(filename: P) -> io::Result<io::BufReader<File>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file))
}

#[cfg(test)]
mod tests {
    use super::{Header, SeqSimTable};
    use crate::default::SSSR_TABLE_FIELD_SEPARATOR;
    use pretty_assertions::assert_eq;

    #[test]
    fn a_header_must_name_the_three_required_columns() {
        for (spec, missing) in [
            ("qacc sacc", "stitle"),
            ("qacc stitle", "sacc"),
            ("sacc stitle", "qacc"),
        ] {
            let refused = Header::parse(spec).unwrap_err();
            assert!(
                refused.contains(missing),
                "{:?} was refused without saying which column is missing: {}",
                spec,
                refused
            );
            // The message must name no option: two of them carry a header, and clap says which
            // one was written. A message naming one sends half its readers to the wrong flag.
            assert!(
                !refused.contains("--"),
                "{:?}'s message names an option, which is clap's to say: {}",
                spec,
                refused
            );
        }
    }

    #[test]
    fn diamonds_own_column_names_are_understood() {
        let diamond = Header::parse("qseqid sseqid stitle").unwrap();
        let blast = Header::parse("qacc sacc stitle").unwrap();
        assert_eq!(diamond, blast);
    }

    #[test]
    fn a_header_counts_the_columns_it_names_not_the_ones_it_needs() {
        // `-f 6 qseqid sseqid evalue stitle` is four columns, and it is the COUNT that catches a
        // row of the wrong shape -- the three indices alone are all present and all wrong.
        let four = Header::parse("qseqid sseqid evalue stitle").unwrap();
        assert_eq!(four.columns, 4);
        assert_eq!(four.stitle_col, 3);
        assert_eq!(Header::parse("default").unwrap(), Header::compiled_in());
    }

    #[test]
    fn a_table_takes_the_settings_it_is_given() {
        let mut table = SeqSimTable::new("hits".to_string(), "hits.tsv".to_string());
        assert_eq!(table.field_separator, SSSR_TABLE_FIELD_SEPARATOR);
        table.set_field_separator('@');
        assert_eq!(table.field_separator, '@');
        table.set_header(&Header::parse("qseqid sseqid evalue stitle").unwrap());
        assert_eq!(table.stitle_col, 3);
        assert_eq!(table.columns, 4);
    }
}
