//! Does the filter list fit the titles it is being applied to?
//!
//! A filter list is written for one database's title shape. Applied to another's it deletes almost
//! nothing, whatever it fails to strip becomes words, and the run succeeds -- so the only evidence
//! is in the descriptions, which is exactly where nobody looks. Measured over 1,215 gene families,
//! preparing RefSeq, GenPept and PDB hits with UniProtKB's list instead of their own costs 0.156
//! precision and 0.104 F1. Recall barely moves: nothing is lost, junk is added, precision pays.
//!
//! WHAT IS REPORTED IS A COMPARISON, NOT A DIAGNOSIS. How many characters the list in use deletes
//! per title, against how many the best of prot-scriber's own would -- both measured on the user's
//! own titles. Saying instead "these look like PDB titles" would be a guess, and there are at least
//! four ways for that guess to be wrong: older NCBI titles carry `gi|…|ref|…|` and match UniProt's
//! accession rule; a table concatenated from two databases has no single right answer; a user's own
//! list may drop a rule on purpose; and a database prot-scriber ships no list for would be told to
//! use one that is also wrong. A measurement of the user's own data cannot be wrong in that way.
//!
//! THE FIRST TOKEN OF A TITLE IS NEUTRALISED BEFORE MEASURING, and that is not a detail. Three of
//! the five lists begin by deleting the leading accession, `^\s*\S+\s+`, which deletes the first
//! word of ANY title whatever its shape. Counting it makes those three look better on every table
//! that has a first word, which is every table -- and it produced a warning on a title with no
//! accession in it at all, recommending a list that would have eaten a real word. A rule that
//! fires on everything is evidence about nothing, so the head is replaced by a constant and what
//! is measured is the shape that follows it.
//!
//! THE THRESHOLDS ARE WHERE THEY ARE BECAUSE OF WHAT WAS MEASURED, on 20,000 real titles per
//! database, with the head neutralised:
//!
//!   titles       uniprot     pdb   refseq  ncbi-nr
//!   PDB              0.4    26.3      2.3      2.3     <- 65x, the case that costs the most
//!   RefSeq           3.3     4.7     30.8     29.2     <- 9x against UniProt's list, and only
//!   GenPept          0.5     2.4      2.5      2.4        5 % between RefSeq's and NR's
//!   no accession     0.0     2.0      2.0      2.0     <- must stay silent
//!
//! A factor of two admits the mismatches (65x, 9x) and rejects the pair that does not need telling
//! apart (1.05x). A floor of five characters rejects the last row, where nothing of substance is
//! being deleted by anyone.
//!
//! WHAT THIS MISSES, stated because a silent miss is easy to mistake for a clean bill of health:
//! GenPept, whose titles in the data measured carry no shape beyond the leading accession -- 2.5
//! characters against 0.5, under the floor. Preparing GenPept hits with the wrong list still costs
//! precision; this will not tell you. Missing a case is silent and harmless, while a warning that
//! fires on a correct configuration is noise, and noise is what stops warnings being read.
//!
//! Counting the fraction of titles a list CHANGES does not work and was measured not to: it is
//! 100 % in eight of nine list/table pairs. Only how MUCH is deleted discriminates.

use crate::input::assets::DefaultList;
use crate::input::regex_files::parse_regexs;
use lazy_static::lazy_static;
use regex::Regex;

/// How many titles to look at. The separation being five- to twentyfold, this is far more than is
/// needed to see it, and it is a fixed cost: a table ten times as large pays exactly the same.
const SAMPLE: usize = 10_000;

/// How much better a shipped list must be before it is worth saying anything.
const FACTOR: f64 = 2.0;

/// And how much it must be deleting at all, in characters per title. A factor needs something to
/// be a factor of: without this, a list deleting two characters where another deletes none is
/// infinitely better and says so.
const FLOOR: f64 = 5.0;

lazy_static! {
    /// prot-scriber's own filter lists, compiled once and shared. About 60 kB of resident memory
    /// per expression, so ~4.6 MB for the four a run does not already hold -- and nothing at all
    /// until a table is actually read, because this is only touched from `ListFit::observe`.
    static ref SHIPPED: Vec<(String, Vec<Regex>)> = DefaultList::filter_lists()
        .iter()
        .map(|list| {
            (
                list.name(),
                parse_regexs(list.content(), &list.name())
                    .unwrap_or_else(|e| panic!("built-in list {:?} does not parse: {}", list.name(), e)),
            )
        })
        .collect();
}

/// The title with its first whitespace-delimited token replaced by a single character, which is
/// what makes the leading-accession rule worth the same to every list that has it. See the module
/// comment: without this the check fires on titles that simply begin with a word.
fn head_neutralised(title: &str) -> String {
    match title.trim_start().split_once(char::is_whitespace) {
        Some((_, rest)) => format!("X {}", rest),
        None => String::from("X"),
    }
}

/// How many characters a list deletes from a title, applying its expressions in order exactly as
/// `filter_stitle` does. Only the length is wanted, so nothing is kept.
fn deleted(title: &str, regexs: &[Regex]) -> usize {
    let mut description = title.to_string();
    for regex in regexs {
        description = regex.replace_all(&description, "").to_string();
    }
    title.len().saturating_sub(description.len())
}

/// One table's running comparison between the filter list it was given and the ones prot-scriber
/// ships.
pub struct ListFit {
    /// How the user named the list in use, for saying it back to them; `None` when the check does
    /// not apply -- `none` was asked for, or the list is the user's own file, which prot-scriber
    /// is in no position to second-guess.
    in_use: Option<String>,
    titles: usize,
    by_list_in_use: usize,
    by_shipped: Vec<usize>,
}

impl ListFit {
    /// A check for a table whose filter list was named, or `disabled` for one where it was not.
    ///
    /// # Arguments
    ///
    /// * `in_use` - The name of the built-in list the table is being prepared with, without the
    ///   leading `@`, or `None` to do nothing.
    pub fn new(in_use: Option<String>) -> ListFit {
        ListFit {
            in_use,
            titles: 0,
            by_list_in_use: 0,
            by_shipped: vec![0; SHIPPED.len()],
        }
    }

    /// Is another title still wanted? Asked before `observe` so that a table of millions of rows
    /// costs this at most `SAMPLE` times.
    pub fn wants(&self) -> bool {
        self.in_use.is_some() && self.titles < SAMPLE
    }

    /// Count one title against every list.
    ///
    /// # Arguments
    ///
    /// * `title` - The sequence title as the search result carries it, before anything.
    /// * `in_use` - The expressions the table is really being prepared with.
    pub fn observe(&mut self, title: &str, in_use: &[Regex]) {
        let title = head_neutralised(title);
        self.titles += 1;
        self.by_list_in_use += deleted(&title, in_use);
        for (index, (_, regexs)) in SHIPPED.iter().enumerate() {
            self.by_shipped[index] += deleted(&title, regexs);
        }
    }

    /// What to tell the user, or `None` when the list in use is as good as anything shipped.
    ///
    /// # Arguments
    ///
    /// * `subject` - What is being prepared, named as the user would recognise it -- a table of an
    ///   annotation run, or the database being reported on. Spelt out by the caller because a
    ///   report can be made from a FASTA, and calling that a table would be a small lie in a message
    ///   whose whole purpose is to be believed.
    pub fn report(&self, subject: &str) -> Option<String> {
        let in_use = self.in_use.as_ref()?;
        if self.titles == 0 {
            return None;
        }
        let (index, &best_total) = self
            .by_shipped
            .iter()
            .enumerate()
            .max_by_key(|(_, total)| **total)?;
        let (best_name, _) = &SHIPPED[index];
        if best_name == in_use {
            return None;
        }
        let per_title = |total: usize| total as f64 / self.titles as f64;
        let (mine, best) = (per_title(self.by_list_in_use), per_title(best_total));
        if mine * FACTOR > best || best < FLOOR {
            return None;
        }
        // The message says the TOTALS, which are what was counted. The thresholds above are
        // averages because they have to be comparable between tables of different sizes, but an
        // average rounded for printing says '0.0 characters' where the truth is 0.04 -- a claim
        // that nothing was deleted, from a number that only means very little was.
        Some(format!(
            "\nWarning: the filter expressions given to {} may not be the ones its titles \
             need. Across its first {} title(s) they delete {} character(s) in all, while \
             prot-scriber's own '@{}' would delete {}. A list is written for one database's \
             title format and deletes almost nothing from another's, and what it fails to delete \
             is counted as words -- which costs precision without costing recall, so nothing \
             fails and only the words themselves show it. Check what the titles look like with \
             'prot-scriber explain --stitle', and 'prot-scriber defaults' lists what ships. This \
             is a comparison of lists on your own titles and not a claim about which database \
             they came from; if '@{}' is right for them, nothing here needs changing.\n",
            subject, self.titles, self.by_list_in_use, best_name, best_total, in_use
        ))
    }
}
