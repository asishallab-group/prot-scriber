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

/// Substrings deleted from a description before it is scored.
pub const FILTER_STITLE_REGEXS: &str = include_str!("../assets/filter_stitle_regexs.txt");

/// The same, for descriptions from NCBI's non-redundant database, whose `stitle` has a format of
/// its own -- an identifier at the front and the source organism in brackets at the back.
pub const FILTER_STITLE_REGEXS_NCBI_NR: &str =
    include_str!("../assets/filter_stitle_regexs_NCBI_NR.txt");

/// The same, for descriptions from the UniRef databases, whose `stitle` ends in `n=…` and begins
/// with a `UniRefNN_` cluster identifier.
pub const FILTER_STITLE_REGEXS_UNIREF: &str =
    include_str!("../assets/filter_stitle_regexs_UniRef.txt");

/// Pairs of lines rewriting a description as it is prepared for scoring.
pub const CAPTURE_REPLACE_PAIRS: &str = include_str!("../assets/capture_replace_pairs.txt");

/// Pairs of lines rewriting a finished human readable description.
pub const POLISH_CAPTURE_REPLACE_PAIRS: &str =
    include_str!("../assets/polish_capture_replace_pairs.txt");
