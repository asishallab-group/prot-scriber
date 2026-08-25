//! The corpus file: what a background corpus is on disk, and everything needed to use it.
//!
//! A corpus is counts of words, and counts of words are only comparable between texts that were
//! prepared the same way. `kinase` is one word or two depending on the splitting expression, and
//! a description discarded by one blacklist is counted by another. So a bare table of counts is
//! not usable data: whoever has it has to remember how it was made, and for a corpus prot-scriber
//! ships nobody can.
//!
//! The file therefore carries its own preprocessing, literally -- every regular expression written
//! out, as `plan.rs` writes out a run's -- and an annotation run given the corpus takes its
//! preprocessing from it. A digest of the sources is recorded too, so that a corpus can be traced
//! back to the release it counts.
//!
//! The shape is a TOML header, a line reading `#WORDS`, and then `word<TAB>count` sorted with the
//! commonest first. Two lines rather than one format, because the header wants to be read by
//! `toml` and the body wants to be streamed and to survive `head`, `grep` and `sort`. Nothing in
//! it is a timestamp, so building the same input with the same rules twice gives the same bytes.

use crate::corpus::Corpus;
use crate::error::Error;
use serde::{Deserialize, Serialize};

/// The line that ends the header and begins the counts.
const WORDS_SENTINEL: &str = "#WORDS";

/// A corpus together with everything said about it.
#[derive(Debug, PartialEq)]
pub struct CorpusFile {
    pub header: Header,
    pub corpus: Corpus,
}

/// What is said about a corpus, i.e. the TOML at the head of the file.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Header {
    /// The prot-scriber that built it. Preprocessing is this program's behaviour, so which one
    /// did it is part of the record.
    pub prot_scriber_version: String,
    pub corpus: Meta,
    pub preprocessing: Preprocessing,
}

/// What the corpus is of, and how big it is.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Meta {
    /// The database whose annotations these are. A word's specificity is a property of the
    /// database it was counted in, and this is what says which one that was.
    pub name: String,
    /// Occurrences counted, i.e. the denominator of every frequency.
    pub tokens: u64,
    /// Distinct words. Small is the failure mode to watch for: specificity taken from a corpus
    /// too narrow to be representative is worse than no specificity at all.
    pub types: usize,
    /// The lowest count kept. One means nothing was dropped.
    pub min_count: u64,
    /// What pruning removed, or zeroes if nothing was pruned. Recorded because a pruned corpus is
    /// missing exactly its rarest -- most specific -- words, and nothing else says so.
    pub pruned_types: usize,
    pub pruned_tokens: u64,
    /// What was counted, in the order it was counted.
    pub sources: Vec<Source>,
}

/// One input a corpus was built from.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Source {
    /// `"fasta"` or `"table"`.
    pub kind: String,
    pub path: String,
    /// The BLAKE3 hash of the bytes that were read. Absent when they came from standard input,
    /// which has no name to record and may not be the same twice.
    pub digest: Option<String>,
}

/// How the descriptions were turned into words. The counts mean nothing without it.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Preprocessing {
    pub split_regex: String,
    pub blacklist_regexs: Vec<String>,
    pub filter_regexs: Vec<String>,
    pub non_informative_words_regexs: Vec<String>,
    pub capture_replace_pairs: Vec<(String, String)>,
}

impl Preprocessing {
    /// A hash of these rules, which is what says whether two corpora are comparable.
    ///
    /// Merging corpora prepared differently, or scoring a run's words against a corpus that
    /// counted different words, gives a number that looks like a frequency and is not one. The
    /// fingerprint is what makes that a refusal rather than a silent wrong answer. It is computed
    /// rather than stored, so it cannot go stale against the rules it is a hash of.
    pub fn fingerprint(&self) -> String {
        let rendered = toml::to_string(self)
            .unwrap_or_else(|e| panic!("corpus preprocessing does not render as TOML: {}", e));
        blake3::hash(rendered.as_bytes()).to_hex().to_string()
    }
}

impl CorpusFile {
    /// The bytes of the file.
    pub fn render(&self) -> String {
        let body = toml::to_string_pretty(&self.header)
            .unwrap_or_else(|e| panic!("the corpus header does not render as TOML: {}", e));
        let mut out = format!(
            "# A prot-scriber word corpus: how often each word appears in the annotations of a\n\
             # reference database, and the rules those annotations were prepared with. Use it with\n\
             #\n\
             #     prot-scriber annotate --db-corpus {name}=<this file> ...\n\
             #\n\
             # which takes the preprocessing below from here, so that the words being scored are\n\
             # the words that were counted. Everything after the {sentinel} line is\n\
             # 'word<TAB>count', commonest first.\n\n{body}\n{sentinel}\n",
            name = self.header.corpus.name,
            sentinel = WORDS_SENTINEL,
            body = body,
        );
        for (word, count) in self.corpus.ranked() {
            out.push_str(word);
            out.push('\t');
            out.push_str(&count.to_string());
            out.push('\n');
        }
        out
    }

    /// Reads a corpus file back.
    ///
    /// # Arguments
    ///
    /// * `content` - The text of the file.
    /// * `source` - What to name in an error message.
    pub fn parse(content: &str, source: &str) -> Result<CorpusFile, Error> {
        let mut header_text = String::new();
        let mut lines = content.lines();
        let mut found_sentinel = false;
        for line in &mut lines {
            if line == WORDS_SENTINEL {
                found_sentinel = true;
                break;
            }
            header_text.push_str(line);
            header_text.push('\n');
        }
        if !found_sentinel {
            return Err(Error::MalformedData(format!(
                "\n\nThe corpus {:?} has no {:?} line, so there is nothing in it that says where \
                 its header ends and its word counts begin. Is it a corpus file at all?\n\n",
                source, WORDS_SENTINEL
            )));
        }
        let header: Header = toml::from_str(&header_text).map_err(|e| {
            Error::MalformedData(format!(
                "\n\nCannot read the header of the corpus {:?}: {}\n\n",
                source, e
            ))
        })?;

        let mut counts: Vec<(String, u64)> = vec![];
        for (offset, line) in lines.enumerate() {
            if line.is_empty() {
                continue;
            }
            let mut fields = line.split('\t');
            let word = fields.next().unwrap_or("");
            let count = fields.next().unwrap_or("");
            let count: u64 = count.parse().map_err(|_| {
                Error::MalformedData(format!(
                    "\n\nThe corpus {:?} does not have a word and a count separated by a tab on \
                     the line {:?}, which is line {} after its {:?} line.\n\n",
                    source,
                    line,
                    offset + 1,
                    WORDS_SENTINEL
                ))
            })?;
            if word.is_empty() {
                return Err(Error::MalformedData(format!(
                    "\n\nThe corpus {:?} counts an empty word, on line {} after its {:?} line.\n\n",
                    source,
                    offset + 1,
                    WORDS_SENTINEL
                )));
            }
            counts.push((word.to_string(), count));
        }

        let corpus = Corpus::of_counts(counts);
        // What the header says it holds is checked against what it holds, because a corpus is read
        // for its frequencies and a wrong denominator makes every one of them wrong quietly:
        if corpus.tokens() != header.corpus.tokens || corpus.types() != header.corpus.types {
            return Err(Error::MalformedData(format!(
                "\n\nThe corpus {:?} says it counts {} occurrences of {} words, but it holds {} \
                 occurrences of {}. It has been truncated or edited.\n\n",
                source,
                header.corpus.tokens,
                header.corpus.types,
                corpus.tokens(),
                corpus.types()
            )));
        }

        Ok(CorpusFile { header, corpus })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn a_corpus_file() -> CorpusFile {
        CorpusFile {
            header: Header {
                prot_scriber_version: String::from("0.1.6"),
                corpus: Meta {
                    name: String::from("sprot"),
                    tokens: 10,
                    types: 4,
                    min_count: 1,
                    pruned_types: 0,
                    pruned_tokens: 0,
                    sources: vec![Source {
                        kind: String::from("fasta"),
                        path: String::from("sprot.fasta"),
                        digest: Some(String::from("abc123")),
                    }],
                },
                preprocessing: Preprocessing {
                    split_regex: String::from("(\\s|-)+"),
                    blacklist_regexs: vec![String::from("(?i)^hypothetical$")],
                    filter_regexs: vec![String::from("(?i)\\sOS=.*$")],
                    non_informative_words_regexs: vec![String::from("(?i)^protein$")],
                    capture_replace_pairs: vec![(String::from("^(\\S+)$"), String::from("$1"))],
                },
            },
            corpus: Corpus::of_counts(vec![
                (String::from("kinase"), 4),
                (String::from("receptor"), 3),
                (String::from("protein"), 2),
                (String::from("phytosulfokine"), 1),
            ]),
        }
    }

    #[test]
    fn a_corpus_file_reads_back_as_what_was_written() {
        let written = a_corpus_file();
        let read = CorpusFile::parse(&written.render(), "test").unwrap();
        assert_eq!(written, read);
    }

    #[test]
    fn the_counts_are_written_commonest_first_and_alphabetically_within_a_count() {
        let mut file = a_corpus_file();
        file.corpus = Corpus::of_counts(vec![
            (String::from("beta"), 2),
            (String::from("alpha"), 2),
            (String::from("kinase"), 9),
        ]);
        file.header.corpus.tokens = 13;
        file.header.corpus.types = 3;
        let rendered = file.render();
        let words: Vec<&str> = rendered
            .split(&format!("{}\n", WORDS_SENTINEL))
            .nth(1)
            .unwrap()
            .lines()
            .collect();
        assert_eq!(vec!["kinase\t9", "alpha\t2", "beta\t2"], words);
    }

    #[test]
    fn a_file_without_the_sentinel_says_so_rather_than_reading_as_an_empty_corpus() {
        let error = CorpusFile::parse("just some text\n", "not-a-corpus.txt").unwrap_err();
        assert!(
            format!("{}", error).contains("no \"#WORDS\" line"),
            "{}",
            error
        );
    }

    #[test]
    fn a_corpus_whose_totals_do_not_match_its_counts_is_refused() {
        let rendered = a_corpus_file().render();
        // Take a word away, as a truncated download or a hand edit would:
        let truncated = rendered.strip_suffix("phytosulfokine\t1\n").unwrap();
        let error = CorpusFile::parse(truncated, "truncated.corpus").unwrap_err();
        assert!(
            format!("{}", error).contains("truncated or edited"),
            "{}",
            error
        );
    }

    #[test]
    fn a_count_that_is_not_a_number_is_refused_by_the_line_it_is_on() {
        let rendered = a_corpus_file().render().replace("kinase\t4", "kinase\tfour");
        let error = CorpusFile::parse(&rendered, "broken.corpus").unwrap_err();
        assert!(format!("{}", error).contains("line 1 after"), "{}", error);
    }

    #[test]
    fn the_fingerprint_is_of_the_rules_and_nothing_else() {
        let mut file = a_corpus_file();
        let before = file.header.preprocessing.fingerprint();
        // How much was counted is not part of what makes two corpora comparable:
        file.header.corpus.tokens = 99;
        assert_eq!(before, file.header.preprocessing.fingerprint());
        // The rules are:
        file.header
            .preprocessing
            .filter_regexs
            .push(String::from("(?i)\\sn=\\d+$"));
        assert_ne!(before, file.header.preprocessing.fingerprint());
    }
}
