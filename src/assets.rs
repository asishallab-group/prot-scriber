//! The regular expression lists prot-scriber ships with, compiled into the binary.
//!
//! Every one of them is the file of the same name under `assets/`, taken with `include_str!`.
//! There is no second copy written out as Rust: `crate::default` parses these to build the
//! defaults, and `prot-scriber defaults` prints them, so what the binary applies, what it hands
//! you when you ask for it, and what the repository documents are all one string.

/// Words that carry no meaning of their own and are scored as such.
pub const NON_INFORMATIVE_WORDS_REGEXS: &str =
    include_str!("../assets/non_informative_words_regexs.txt");

/// Descriptions matching any of these are discarded whole.
pub const BLACKLIST_STITLE_REGEXS: &str = include_str!("../assets/blacklist_stitle_regexs.txt");

/// Substrings deleted from a description before it is scored, for descriptions from UniProtKB --
/// Swiss-Prot and TrEMBL -- whose `stitle` is `sp|ACC|ID_SPECIES <desc> OS=… OX=… GN=… PE=… SV=…`.
///
/// This is also the list a table gets when it names none. It carried no database in its name for
/// as long as it existed, which made it look like a general list rather than one written for a
/// particular shape of title -- and it is applied to every table that names no other, where it is
/// worth 0.156 precision on results that are not UniProt's.
pub const FILTER_STITLE_REGEXS_UNIPROT: &str =
    include_str!("../assets/filter_stitle_regexs_UniProt.txt");

/// The same, for descriptions from NCBI's non-redundant database, whose `stitle` has a format of
/// its own -- an identifier at the front and the source organism in brackets at the back.
pub const FILTER_STITLE_REGEXS_NCBI_NR: &str =
    include_str!("../assets/filter_stitle_regexs_NCBI_NR.txt");

/// The same, for descriptions from NCBI's RefSeq, whose `stitle` carries a `MULTISPECIES:` prefix,
/// an `isoform X1` suffix and `LOC` gene identifiers that no other database's does.
pub const FILTER_STITLE_REGEXS_REFSEQ: &str =
    include_str!("../assets/filter_stitle_regexs_RefSeq.txt");

/// The same, for descriptions from the PDB, whose `stitle` is `<id> mol:protein length:NNN <desc>`.
pub const FILTER_STITLE_REGEXS_PDB: &str = include_str!("../assets/filter_stitle_regexs_PDB.txt");

/// The same, for descriptions from the UniRef databases, whose `stitle` ends in `n=…` and begins
/// with a `UniRefNN_` cluster identifier.
pub const FILTER_STITLE_REGEXS_UNIREF: &str =
    include_str!("../assets/filter_stitle_regexs_UniRef.txt");

/// Pairs of lines rewriting a description as it is prepared for scoring.
pub const CAPTURE_REPLACE_PAIRS: &str = include_str!("../assets/capture_replace_pairs.txt");

/// Pairs of lines rewriting a finished human readable description.
pub const POLISH_CAPTURE_REPLACE_PAIRS: &str =
    include_str!("../assets/polish_capture_replace_pairs.txt");

use clap::ValueEnum;

/// prot-scriber's built-in regular expression lists, named. `clap` derives the accepted spellings
/// from the variant names, so a misspelling is answered with the full list of what there is.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultList {
    /// --blacklist-regexs (-b): descriptions matching any of these are discarded whole
    BlacklistRegexs,
    /// --filter-regexs (-l), for UniProtKB, and the list a table that names none is given
    FilterRegexsUniprot,
    /// --filter-regexs (-l), for sequence similarity search results from NCBI's NR
    FilterRegexsNcbiNr,
    /// --filter-regexs (-l), for sequence similarity search results from NCBI's RefSeq
    FilterRegexsRefseq,
    /// --filter-regexs (-l), for sequence similarity search results from the PDB
    FilterRegexsPdb,
    /// --filter-regexs (-l), for sequence similarity search results from the UniRef databases
    FilterRegexsUniref,
    /// --capture-replace-pairs (-c): pairs of lines rewriting a description before it is scored
    CaptureReplacePairs,
    /// --non-informative-words-regexs (-w): words scored as carrying no meaning of their own
    NonInformativeWordsRegexs,
    /// --polish-capture-replace-pairs (-d): pairs of lines rewriting a finished description
    PolishCaptureReplacePairs,
}

impl DefaultList {
    /// The list itself, as it is compiled in and as prot-scriber parses it.
    pub fn content(&self) -> &'static str {
        match self {
            DefaultList::BlacklistRegexs => BLACKLIST_STITLE_REGEXS,
            DefaultList::FilterRegexsUniprot => FILTER_STITLE_REGEXS_UNIPROT,
            DefaultList::FilterRegexsNcbiNr => FILTER_STITLE_REGEXS_NCBI_NR,
            DefaultList::FilterRegexsRefseq => FILTER_STITLE_REGEXS_REFSEQ,
            DefaultList::FilterRegexsPdb => FILTER_STITLE_REGEXS_PDB,
            DefaultList::FilterRegexsUniref => FILTER_STITLE_REGEXS_UNIREF,
            DefaultList::CaptureReplacePairs => CAPTURE_REPLACE_PAIRS,
            DefaultList::NonInformativeWordsRegexs => NON_INFORMATIVE_WORDS_REGEXS,
            DefaultList::PolishCaptureReplacePairs => POLISH_CAPTURE_REPLACE_PAIRS,
        }
    }

    /// The option this list is the default for, and what it does, for the bare `defaults` listing.
    pub fn what(&self) -> (&'static str, &'static str) {
        match self {
            DefaultList::BlacklistRegexs => (
                "--blacklist-regexs (-b)",
                "descriptions matching any of these are discarded whole",
            ),
            DefaultList::FilterRegexsUniprot => (
                "--filter-regexs (-l)",
                "for results from UniProtKB, and THE DEFAULT for a table naming no list",
            ),
            DefaultList::FilterRegexsNcbiNr => (
                "--filter-regexs (-l)",
                "the same, for results from NCBI's non-redundant database",
            ),
            DefaultList::FilterRegexsRefseq => (
                "--filter-regexs (-l)",
                "the same, for results from NCBI's RefSeq",
            ),
            DefaultList::FilterRegexsPdb => (
                "--filter-regexs (-l)",
                "the same, for results from the PDB",
            ),
            DefaultList::FilterRegexsUniref => (
                "--filter-regexs (-l)",
                "the same, for results from the UniRef databases",
            ),
            DefaultList::CaptureReplacePairs => (
                "--capture-replace-pairs (-c)",
                "pairs of lines rewriting a description before it is scored",
            ),
            DefaultList::NonInformativeWordsRegexs => (
                "--non-informative-words-regexs (-w)",
                "words scored as carrying no meaning of their own",
            ),
            DefaultList::PolishCaptureReplacePairs => (
                "--polish-capture-replace-pairs (-d)",
                "pairs of lines rewriting a finished description",
            ),
        }
    }

    /// The name this list is asked for by, i.e. what `clap` derived from the variant name.
    pub fn name(&self) -> String {
        self.to_possible_value()
            .expect("every list is a possible value")
            .get_name()
            .to_string()
    }
}

impl DefaultList {
    /// The list of this name, if there is one. The names are exactly those `prot-scriber defaults`
    /// prints, so `--db-filter nr=@filter-regexs-ncbi-nr` and
    /// `prot-scriber defaults filter-regexs-ncbi-nr` are the same list by construction.
    ///
    /// # Arguments
    ///
    /// * `name` - The name to look up, without the leading `@`.
    pub fn from_name(name: &str) -> Option<DefaultList> {
        DefaultList::value_variants()
            .iter()
            .copied()
            .find(|list| list.name() == name)
    }

    /// The lists that `--filter-regexs` can be given, i.e. the ones written for one database's
    /// title format. `crate::input::list_fit` compares a table's titles against all of them, so
    /// a list added here is one more candidate and needs nothing else done to it.
    pub fn filter_lists() -> [DefaultList; 5] {
        [
            DefaultList::FilterRegexsUniprot,
            DefaultList::FilterRegexsNcbiNr,
            DefaultList::FilterRegexsRefseq,
            DefaultList::FilterRegexsPdb,
            DefaultList::FilterRegexsUniref,
        ]
    }

    /// Every name, in the order `defaults` lists them, for saying what there is when one is wrong.
    pub fn all_names() -> Vec<String> {
        DefaultList::value_variants()
            .iter()
            .map(|list| list.name())
            .collect()
    }
}

/// The same as `resolve`, for an option whose absence, or the literal `"default"`, means
/// prot-scriber's own list.
///
/// `"default"` is spelled out because every other rule-list option accepts it: `--blacklist
/// default` and `--filter default` are how a per-table option says "leave this one alone". An
/// option that took a file, an `@NAME` and `none` but not `default` would be a trap rather than a
/// simplification, and `default` is what a script writes when it fills the value in from a
/// variable.
///
/// # Arguments
///
/// * `source` - What the user asked for, or `None` if they did not.
/// * `default` - prot-scriber's own list.
/// * `read_file` - Reads the list from a path.
/// * `parse` - Parses the list from text, naming the source in any error.
pub fn resolve_or_default<T: Clone>(
    source: Option<&str>,
    default: &T,
    read_file: impl Fn(&str) -> Result<T, crate::error::Error>,
    parse: impl Fn(&str, &str) -> Result<T, crate::error::Error>,
) -> Result<T, crate::error::Error> {
    match source {
        None | Some("default") => Ok(default.clone()),
        Some(source) => resolve(source, read_file, parse),
    }
}

/// The text a rule-list argument names: a built-in list written `@name`, nothing at all written
/// `none`, or the contents of a file.
///
/// A built-in can be named rather than found, which is the last thing that sent users to the
/// network: before `defaults` existed, using the NCBI-NR filter list meant downloading a copy, and
/// a copy is a thing that goes stale. `@filter-regexs-ncbi-nr` cannot.
///
/// # Arguments
///
/// * `source` - The argument value, as the user wrote it.
/// * `read_file` - How to read a path, when the source turns out to be one.
pub fn resolve<T>(
    source: &str,
    read_file: impl Fn(&str) -> Result<T, crate::error::Error>,
    parse: impl Fn(&str, &str) -> Result<T, crate::error::Error>,
) -> Result<T, crate::error::Error> {
    if source == "none" {
        return parse("", "none");
    }
    if let Some(name) = source.strip_prefix('@') {
        return match DefaultList::from_name(name) {
            Some(list) => parse(list.content(), source),
            None => Err(crate::error::Error::Usage(format!(
                "\n\nCannot run Annotation-Process, because there is no built-in list called {:?}. The built-in lists are: {}. Run 'prot-scriber defaults' to see what each of them is, or give a file instead of an '@' name.\n\n",
                name,
                DefaultList::all_names().join(", ")
            ))),
        };
    }
    read_file(source)
}
