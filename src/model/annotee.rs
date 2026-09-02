//! What is being annotated: a single query, or a whole family of them.

use crate::default::{UNKNOWN_FAMILY_DESCRIPTION, UNKNOWN_PROTEIN_DESCRIPTION};

/// What is being annotated, which decides what is written for it when nothing could be said about
/// it at all: an "unknown protein" or an "unknown sequence family".
///
/// It is not the same question as the mode of the run. A run that annotates families also
/// annotates the queries that belong to none of them, if it was asked to with
/// `--annotate-non-family-queries` (`-a`), and those are queries.
///
/// It lives in the domain model rather than in `annotation_process` because that is what it is: a
/// `Query` or a `SeqFamily`, the two things this module already names. Declared there it also made
/// a cycle, `annotation_process` holding `Vec<TraceSink>` while `trace` imported this enum from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Annotee {
    Query,
    Family,
}

impl Annotee {
    /// What to write for an annotee that could not be annotated.
    pub fn unknown(&self) -> &'static str {
        match self {
            Annotee::Query => UNKNOWN_PROTEIN_DESCRIPTION,
            Annotee::Family => UNKNOWN_FAMILY_DESCRIPTION,
        }
    }
}
