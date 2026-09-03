//! Default values and global constants are kept in this module.
//!
//! The regular expression lists are not written out here as Rust literals. They are the files
//! under `assets/`, compiled into the binary with `include_str!` and parsed by exactly the code
//! that parses a list a user passes on the command line. There is therefore only one copy of each
//! list: the file the documentation points at *is* what the binary applies, and the two cannot
//! drift apart -- which is what they had done, every `misc/` file having last been touched in 2022
//! while these lists kept being extended until 2024.
use crate::input::assets;
use crate::input::assets::DefaultList;
use crate::input::regex_files::{
    parse_pairs, parse_regexs, parse_rules, PairList, RuleList,
};
use regex::Regex;
use std::collections::HashMap;

/// The path that stands for a standard stream rather than a file: standard INPUT wherever
/// something is read (`--db`, `--seq-families`, `explain --fasta`, `--table` and `--stitle`) and
/// standard OUTPUT wherever something is written (`--output`, `--explain-out`).
///
/// A single dash is what every unix tool that has this at all uses for it, and it cannot collide
/// with a real file name: a shell expands `-` to itself, and a path meant literally can still be
/// written as `./-`. It lived with the output-table writer, named `STDOUT_PATH`, while six other
/// places wrote the dash out as a literal, three of them to recognise standard INPUT -- for which
/// that name would have read as a mistake. Here it is one constant, in the module whose first line
/// says it keeps the global ones.
pub const STREAM_PATH: &str = "-";

/// Parses one of the built-in regular expression lists shipped in `assets/`.
///
/// A list that does not parse is a broken build rather than anything the user did, so this panics
/// -- which `main`'s hook reports as the bug it is. The `assets/` files are covered by tests, so
/// it cannot happen without CI saying so first.
fn builtin_regexs(content: &str, name: &str) -> Vec<Regex> {
    parse_regexs(content, name)
        .unwrap_or_else(|e| panic!("built-in list {:?} does not parse: {}", name, e))
}

/// The same, keeping the list and line each expression came from.
///
/// The list is named the way `prot-scriber defaults` names it, not by its path under `assets/`:
/// that is the name the user can reach it by, and `assets/` is not on their disk at all.
fn builtin_rules(content: &str, list: DefaultList) -> RuleList {
    let name = list.name();
    parse_rules(content, &name)
        .unwrap_or_else(|e| panic!("built-in list {:?} does not parse: {}", name, e))
}

/// The same as `builtin_rules`, for the built-in lists that hold pairs of lines.
fn builtin_pairs(content: &str, list: DefaultList) -> PairList {
    let name = list.name();
    parse_pairs(content, &name)
        .unwrap_or_else(|e| panic!("built-in list {:?} does not parse: {}", name, e))
}

/// The score assigned to non informative words:
pub const NON_INFORMATIVE_WORD_SCORE : f64 = 0.000001;

/// The default argument `AnnotationProcess.center_iic_at_quantile` to be used for centering
/// the inverse information content values of words. The literal 50.0 indicates centering at
/// the mean and not actually a quantile:
pub const CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE : f64 = 50.0;

/// The maximum number of applying one tuple of regular expression and match-group replacing in
/// generate_hrd_associated_funcs::split_descriptions( ..., `replace_regexs`) (see above
/// `REPLACE_REGEXS_DESCRIPTION`):
pub const MAX_MATCH_REPLACE_ITERATIONS: u8 = u8::MAX;

/// Default sequence similarity search result table field separator:
pub const SSSR_TABLE_FIELD_SEPARATOR: char = '\t';

/// The default short description to be used for queries for which no reasonable description
/// can be generated
pub const UNKNOWN_PROTEIN_DESCRIPTION: &str = "unknown protein";

/// The default short description to be used for sequence families for which no reasonable
/// description can be generated
pub const UNKNOWN_FAMILY_DESCRIPTION: &str = "unknown sequence family";


/// The default character used to split gene-family-identifiers from the set of genes the
/// respective family is comprised of:
pub const SPLIT_GENE_FAMILY_ID_FROM_GENE_SET: &str = "\t";

lazy_static! {

    /// The default Blacklist of regular expressions used to check for non-informative words
    /// in the description to be excluded from scoring. If ANY of these expression matches
    /// the word is considered as non-informative
    pub static ref NON_INFORMATIVE_WORDS_REGEXS: Vec<Regex> = builtin_regexs(
        assets::NON_INFORMATIVE_WORDS_REGEXS,
        "assets/non_informative_words_regexs.txt"
    );

    /// The default Blacklist of regular expressions used to filter out Hit title (`stitle`) fields
    /// if they match ANY of these expressions.
    pub static ref BLACKLIST_STITLE_REGEXS: RuleList = builtin_rules(
        assets::BLACKLIST_STITLE_REGEXS,
        DefaultList::BlacklistRegexs
    );

    /// The default regular expressions used to filter a Hit title (`stitle`) and retain the short
    /// human readable description.
    pub static ref FILTER_REGEXS: RuleList = builtin_rules(
        assets::FILTER_STITLE_REGEXS_UNIPROT,
        DefaultList::FilterRegexsUniprot
    );

    /// The default header definition of sequence similarity search result tables, i.e. mapping
    /// column names to their factual position in the to be parsed table.
    pub static ref SEQ_SIM_TABLE_COLUMNS: HashMap<String, usize> = {
        let mut h = HashMap::new();
        // Default header is 'qacc sacc stitle', and a table given no --db-header must have
        // exactly those three columns. A column prot-scriber does not read still has to be
        // named: unnamed, it is indistinguishable from one of these three being somewhere
        // else, which is how `-f 6 qseqid sseqid evalue stitle` came to have every e-value
        // annotated as a description.
        h.insert("qacc".to_string(), 0);
        h.insert("sacc".to_string(), 1);
        h.insert("stitle".to_string(), 2);
        h
    };

    /// A Hit's description is split into words using this default regular expression.
    ///
    /// Every character in the class is one that cannot be part of a word, so a run of them is
    /// where one word ends and the next begins. The brackets, the braces, the angle brackets and
    /// the arithmetic signs are there because an enzyme's cofactor and reaction sense are written
    /// with them -- 'superoxide dismutase [Cu-Zn]', 'alcohol dehydrogenase [NAD(P)+]' -- and a
    /// word that keeps the bracket it touched is a different word from the same word without it.
    ///
    /// The tilde is NOT in the class, and that is deliberate: it is the sentinel the
    /// capture-replace pairs join a domain accession with, so `duf~4228` has to stay one word.
    /// It never occurs in an annotation -- none in the whole of Swiss-Prot, none in six million
    /// lines of search results -- and the polish pairs take it back out of a finished
    /// description.
    /// The default regular expression splitting a gene family's list of gene identifiers.
    ///
    /// Compiled here rather than kept as a string, so that the one place a bad one can be written
    /// -- the command line, and a run plan -- is the one place it is reported from.
    pub static ref SPLIT_GENE_FAMILY_GENES_REGEX: Regex = Regex::new(r"(\s*,\s*|\s+)").unwrap();

    pub static ref SPLIT_DESCRIPTION_REGEX: Regex = Regex::new(r"([()\[\]{}<>+*^_\-/|\\;,':.\s]+)").unwrap();

    /// The default vector of regular expressions _with_ match-groups to be used to split
    /// descriptions (parsed `stitle`) into separate words by replacing the matched region with
    /// the first and second captures:
    pub static ref CAPTURE_REPLACE_DESCRIPTION_PAIRS: PairList = builtin_pairs(
        assets::CAPTURE_REPLACE_PAIRS,
        DefaultList::CaptureReplacePairs
    );

    /// The default vector of regular expressions _with_ match-groups to be used to post-process
    /// ("polish") assigned human readable descriptions before using them as final output:
    pub static ref POLISH_CAPTURE_REPLACE_PAIRS: PairList = builtin_pairs(
        assets::POLISH_CAPTURE_REPLACE_PAIRS,
        DefaultList::PolishCaptureReplacePairs
    );
}
