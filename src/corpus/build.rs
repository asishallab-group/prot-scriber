//! `prot-scriber corpus`: building, combining and inspecting a background word corpus.
//!
//! What a word is worth today is decided among the hits of the one protein being annotated: a word
//! that most of them share is what they agree the protein is. That finds a consensus, and it is
//! what prot-scriber is for -- but it cannot tell a word that says something from a word every
//! annotation in the database carries. `domain`, `containing` and `family` are on no list of
//! non-informative words, and among a few dozen hit descriptions they are as common as `kinase`.
//! `domain containing protein` is the commonest thing prot-scriber has to say.
//!
//! Telling those apart takes a second, larger sample: the annotations of the whole reference
//! database. That is what a corpus is, and this is what builds one.
//!
//! It is built from the reference FASTA rather than from a search result, and the difference is
//! not a technicality. A search result holds only the sequences that got a hit, which is a sample
//! biased towards whatever the query proteome happens to resemble -- exactly the bias a background
//! is supposed to correct for. A table is accepted all the same, because a FASTA is not always at
//! hand, and it is counted once per subject sequence rather than once per row so that a subject
//! every query hits does not count for a hundred.

use crate::assets;
use crate::cli::{CorpusBuild, CorpusMerge, CorpusShow};
use crate::corpus::file::{CorpusFile, Header, Meta, Preprocessing, Source};
use crate::corpus::Corpus;
use crate::default::{NON_INFORMATIVE_WORDS_REGEXS, SPLIT_DESCRIPTION_REGEX};
use crate::error::Error;
use crate::hrd::split_descriptions;
use crate::input::regex_files::{parse_regex_file, parse_regexs};
use crate::input::seq_sim_table::SeqSimTable;
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

/// Counts the words of one or more reference databases and writes the corpus.
///
/// # Arguments
///
/// * `what` - What to count and how to prepare it.
pub fn build(what: &CorpusBuild) -> Result<(), Error> {
    if what.fasta.is_empty() && what.table.is_empty() {
        return Err(Error::Usage(String::from(
            "\n\nThere is nothing to count: 'prot-scriber corpus build' needs at least one \
             --fasta or one --table. A corpus is best counted from the reference database's own \
             FASTA, which is the file the search was run against.\n\n",
        )));
    }

    // A table that will never be parsed as a table: it holds the rule lists, resolved by the very
    // code that resolves them for a table that will be, so that a corpus is prepared exactly as
    // the annotation run preparing its own descriptions is.
    let mut rules = SeqSimTable::new(what.name.clone(), String::new());
    rules.set_blacklist_regexs(&what.blacklist)?;
    rules.set_filter_regexs(&what.filter)?;
    rules.set_capture_replace_pairs(&what.capture_replace)?;
    rules.set_columns(&what.header, 1)?;
    rules.set_field_separator(&what.field_separator)?;
    let non_informative: Vec<Regex> = assets::resolve_or_default(
        what.non_informative_words_regexs.as_deref(),
        &*NON_INFORMATIVE_WORDS_REGEXS,
        parse_regex_file,
        parse_regexs,
    )?;
    let split_regex = match &what.description_split_regex {
        Some(regex) => regex.clone(),
        None => (*SPLIT_DESCRIPTION_REGEX).clone(),
    };

    let mut corpus = Corpus::default();
    let mut sources: Vec<Source> = vec![];
    for path in &what.fasta {
        sources.push(count_fasta(
            path,
            &rules,
            &non_informative,
            &split_regex,
            &mut corpus,
        )?);
    }
    // One set of seen accessions for every table, not one per table: a reference sequence's
    // description is one description however many of the searches found it.
    let mut seen: HashSet<String> = HashSet::new();
    for path in &what.table {
        sources.push(count_table(
            path,
            &rules,
            &non_informative,
            &split_regex,
            &mut corpus,
            &mut seen,
        )?);
    }

    if corpus.is_empty() {
        return Err(Error::EmptyResult(format!(
            "\n\nNot one word was counted from {}. Either the descriptions were all discarded by \
             the blacklist, or the input is not what it was said to be -- 'prot-scriber explain \
             --stitle' on one line of it says which.\n\n",
            named(&sources)
        )));
    }

    let (pruned_types, pruned_tokens) = if what.min_count > 1 {
        corpus.prune(what.min_count)
    } else {
        (0, 0)
    };

    let file = CorpusFile {
        header: Header {
            prot_scriber_version: String::from(env!("CARGO_PKG_VERSION")),
            corpus: Meta {
                name: what.name.clone(),
                tokens: corpus.tokens(),
                types: corpus.types(),
                min_count: what.min_count,
                pruned_types,
                pruned_tokens,
                sources,
            },
            preprocessing: Preprocessing {
                split_regex: split_regex.as_str().to_string(),
                blacklist_regexs: strings(&rules.blacklist_regexs),
                filter_regexs: strings(&rules.filter_regexs),
                non_informative_words_regexs: strings(&non_informative),
                capture_replace_pairs: rules
                    .capture_replace_pairs
                    .iter()
                    .map(|(regex, replace)| (regex.as_str().to_string(), replace.clone()))
                    .collect(),
            },
        },
        corpus,
    };
    write(&file.render(), &what.output)
}

/// Adds corpora together, refusing any two that were not prepared the same way.
///
/// # Arguments
///
/// * `what` - The corpora to add and where to put the result.
pub fn merge(what: &CorpusMerge) -> Result<(), Error> {
    let mut merged: Option<(&String, CorpusFile)> = None;
    for path in &what.corpora {
        let next = read(path)?;
        match &mut merged {
            None => merged = Some((path, next)),
            Some((first, into)) => {
                if into.header.preprocessing != next.header.preprocessing {
                    return Err(Error::Usage(format!(
                        "\n\nThe corpora {:?} and {:?} were not prepared the same way -- their \
                         preprocessing fingerprints are {} and {} -- so their counts are counts of \
                         different things and adding them would give a frequency that is not one. \
                         Build them again with the same rules, or use them separately.\n\n",
                        first,
                        path,
                        &into.header.preprocessing.fingerprint()[..16],
                        &next.header.preprocessing.fingerprint()[..16],
                    )));
                }
                into.corpus.merge(&next.corpus);
                into.header.corpus.sources.extend(next.header.corpus.sources);
                into.header.corpus.pruned_types += next.header.corpus.pruned_types;
                into.header.corpus.pruned_tokens += next.header.corpus.pruned_tokens;
                into.header.corpus.min_count =
                    into.header.corpus.min_count.max(next.header.corpus.min_count);
            }
        }
    }
    let (_, mut merged) = merged.ok_or_else(|| {
        Error::Usage(String::from(
            "\n\nThere is nothing to merge: 'prot-scriber corpus merge' needs the corpora to \
             add, and at least two of them to be worth doing.\n\n",
        ))
    })?;
    merged.header.corpus.name = what.name.clone();
    merged.header.prot_scriber_version = String::from(env!("CARGO_PKG_VERSION"));
    merged.header.corpus.tokens = merged.corpus.tokens();
    merged.header.corpus.types = merged.corpus.types();
    write(&merged.render(), &what.output)
}

/// Reports what a corpus holds, without printing the whole of it.
///
/// # Arguments
///
/// * `what` - The corpus to describe, and how many of its commonest words to show.
pub fn show(what: &CorpusShow) -> Result<(), Error> {
    let file = read(&what.corpus)?;
    let meta = &file.header.corpus;
    let mut out = format!(
        "corpus        {}\nbuilt by      prot-scriber {}\nfingerprint   {}\nwords         \
         {} distinct, {} occurrences\n",
        meta.name,
        file.header.prot_scriber_version,
        &file.header.preprocessing.fingerprint()[..16],
        meta.types,
        meta.tokens,
    );
    if meta.min_count > 1 {
        out.push_str(&format!(
            "pruned        every word seen fewer than {} times: {} words, {} occurrences\n              Those are the rarest words there were, which is to say the most\n              specific ones, and a word missing from a corpus counts as unseen.\n",
            meta.min_count, meta.pruned_types, meta.pruned_tokens
        ));
    }
    for source in &meta.sources {
        out.push_str(&format!(
            "source        {} {} {}\n",
            source.kind,
            source.path,
            source.digest.as_deref().unwrap_or("(not hashed)")
        ));
    }
    if meta.types < SMALL_CORPUS {
        out.push_str(&format!(
            "\nWarning: {} distinct words is small for a background corpus. What a background is \
             for is to say which words are common in the database as a whole, and a sample this \
             narrow says instead which words are common in the sample -- which can rank the \
             boilerplate above the words that mean something, i.e. the wrong way round.\n",
            meta.types
        ));
    }
    out.push_str(&format!("\ncommonest {} words:\n", what.words));
    for (word, count) in file.corpus.ranked().iter().take(what.words) {
        out.push_str(&format!("{:>12}  {}\n", count, word));
    }
    write(&out, "-")
}

/// Below this many distinct words a corpus is more likely to mislead than to help; see `show`.
const SMALL_CORPUS: usize = 10_000;

/// Counts the words of every description in a FASTA file, and reports what was read.
///
/// A FASTA header is a sequence title: everything after the `>` is what a search puts in the
/// `stitle` column, so it goes through the very same rules.
fn count_fasta(
    path: &str,
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
    corpus: &mut Corpus,
) -> Result<Source, Error> {
    let mut digest = blake3::Hasher::new();
    for_each_line(path, &mut digest, |line| {
        if let Some(stitle) = line.strip_prefix('>') {
            observe(stitle, rules, non_informative, split_regex, corpus);
        }
    })?;
    Ok(source("fasta", path, digest))
}

/// Counts the words of every distinct subject sequence's description in a search result table.
///
/// Once per subject, not once per row: the same reference sequence is hit by many queries, and
/// counting each of those hits would make the corpus a record of what this query set matched
/// rather than of what the database says.
///
/// # Arguments
///
/// * `seen` - The subject accessions already counted, carried across every table of one build so
///   that tables which overlap -- as the results of several searches against one database do --
///   do not count what they share twice.
fn count_table(
    path: &str,
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
    corpus: &mut Corpus,
    seen: &mut HashSet<String>,
) -> Result<Source, Error> {
    let mut digest = blake3::Hasher::new();
    let mut short_line: Option<(usize, usize)> = None;
    for_each_line(path, &mut digest, |line| {
        let fields: Vec<&str> = line.trim().split(rules.field_separator).collect();
        match (fields.get(rules.sacc_col), fields.get(rules.stitle_col)) {
            (Some(sacc), Some(stitle)) => {
                if seen.insert((*sacc).to_string()) {
                    observe(stitle, rules, non_informative, split_regex, corpus);
                }
            }
            _ => {
                if short_line.is_none() {
                    short_line = Some((seen.len(), fields.len()));
                }
            }
        }
    })?;
    if let Some((_, fields)) = short_line {
        return Err(Error::MalformedData(format!(
            "\n\nCannot count the table {:?}, because one of its lines splits into {} field(s) \
             using the field separator {:?}, which is too few to hold the 'sacc' and 'stitle' \
             columns. Please check the --header and --field-separator arguments.\n\n",
            path, fields, rules.field_separator
        )));
    }
    Ok(source("table", path, digest))
}

/// Puts one sequence title through the rules and counts the words it leaves.
fn observe(
    stitle: &str,
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
    corpus: &mut Corpus,
) {
    if let Some(description) = rules.hit_description(stitle, None) {
        corpus.observe_description(&split_descriptions(&description, split_regex), non_informative);
    }
}

/// Reads a file, or standard input for `-`, a line at a time, hashing the bytes as they go by.
///
/// A line at a time because a reference FASTA is tens of gigabytes and the whole point of a
/// corpus is that it is small: what is held is the counts, never the input.
fn for_each_line(
    path: &str,
    digest: &mut blake3::Hasher,
    mut visit: impl FnMut(&str),
) -> Result<(), Error> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path).map_err(|e| {
            Error::opening(path, format!("No such file {:?}", path), &e)
        })?))
    };
    let mut reader = reader;
    let mut raw: Vec<u8> = Vec::new();
    loop {
        raw.clear();
        match reader.read_until(b'\n', &mut raw) {
            Ok(0) => break,
            Ok(_) => {
                digest.update(&raw);
                let decoded = String::from_utf8_lossy(&raw);
                visit(decoded.trim_end_matches(['\n', '\r']));
            }
            Err(e) => return Err(Error::reading(path, &e)),
        }
    }
    Ok(())
}

/// What was read, for the record. Standard input is not hashed: there is no path to record it
/// against and no way to read it a second time.
fn source(kind: &str, path: &str, digest: blake3::Hasher) -> Source {
    Source {
        kind: String::from(kind),
        path: String::from(path),
        digest: if path == "-" {
            None
        } else {
            Some(digest.finalize().to_hex().to_string())
        },
    }
}

/// Reads a corpus file from a path, or from standard input for `-`.
pub fn read(path: &str) -> Result<CorpusFile, Error> {
    let content = if path == "-" {
        let mut content = String::new();
        io::Read::read_to_string(&mut io::stdin(), &mut content)
            .map_err(|e| Error::Io(format!("\n\nCould not read the corpus: {}\n\n", e)))?;
        content
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| Error::opening(path, format!("No such corpus {:?}", path), &e))?
    };
    CorpusFile::parse(&content, path)
}

/// The BLAKE3 hash of a corpus file, which is what a run plan records so that a replay can tell
/// whether the corpus it is given is the corpus that was used.
///
/// # Arguments
///
/// * `path` - The corpus file. `None` for standard input, which cannot be read a second time.
pub fn digest(path: &str) -> Result<Option<String>, Error> {
    if path == "-" {
        return Ok(None);
    }
    let bytes = std::fs::read(path)
        .map_err(|e| Error::opening(path, format!("No such corpus {:?}", path), &e))?;
    Ok(Some(blake3::hash(&bytes).to_hex().to_string()))
}

/// Writes to a path, or to standard output for `-`.
fn write(content: &str, path: &str) -> Result<(), Error> {
    if path == "-" {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        out.write_all(content.as_bytes())
            .and_then(|()| out.flush())
            .map_err(|e| Error::Io(format!("\n\nCould not write the corpus: {}\n\n", e)))
    } else {
        std::fs::write(path, content)
            .map_err(|e| Error::Io(format!("\n\nCould not write {:?}: {}\n\n", path, e)))
    }
}

/// The source text of each of a list of regular expressions.
fn strings(regexs: &[Regex]) -> Vec<String> {
    regexs.iter().map(|r| r.as_str().to_string()).collect()
}

/// The inputs, named, for an error message.
fn named(sources: &[Source]) -> String {
    sources
        .iter()
        .map(|source| format!("{:?}", source.path))
        .collect::<Vec<String>>()
        .join(", ")
}
