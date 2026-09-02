//! `prot-scriber explain --stitle`: what prot-scriber makes of a sequence title, step by step.
//!
//! Everything a description goes through before it is scored happens here -- the blacklist, the
//! filter expressions, the capture-replace pairs, lower-casing, and the split into words -- and it
//! is carried out by the same code an annotation run carries it out with, reporting what it did as
//! it goes. So this is a question that can be *asked* rather than reasoned about: whether a locus
//! code survives the default filter list, what a candidate list would do to the titles a database
//! actually returns, why a whole class of hits is being discarded.

pub mod compare;
pub mod report;

use crate::cli::ExplainWhat;
use crate::default::{NON_INFORMATIVE_WORDS_REGEXS, SPLIT_DESCRIPTION_REGEX};
use crate::description::{matches_blacklist, Steps};
use crate::error::Error;
use crate::hrd::split_descriptions;
use crate::input::regex_files::{parse_regex_file, parse_regexs};
use crate::input::seq_sim_table::SeqSimTable;
use regex::Regex;
use std::io::{self, BufRead, Write};

/// Reports what the description pipeline makes of each of the given sequence titles.
///
/// # Arguments
///
/// * `what` - The titles to explain and the rule lists to explain them with.
pub fn explain_stitles(what: &ExplainWhat) -> Result<(), Error> {
    // The very lists an annotation run would use, resolved by the very code that resolves them
    // there -- including the '@name' built-ins and 'none':
    // A table that will never be read: it is here only to hold the three rule lists, resolved by
    // the very code that resolves them for a table that will be.
    let mut rules = SeqSimTable::new(String::from("explain"), String::new());
    rules.set_blacklist_regexs(&what.blacklist)?;
    rules.set_filter_regexs(&what.filter)?;
    rules.set_capture_replace_pairs(&what.capture_replace)?;
    let non_informative: Vec<Regex> = crate::assets::resolve_or_default(
        what.non_informative_words_regexs.as_deref(),
        &*NON_INFORMATIVE_WORDS_REGEXS,
        parse_regex_file,
        parse_regexs,
    )?;
    let split_regex = match &what.description_split_regex {
        Some(regex) => regex.clone(),
        None => (*SPLIT_DESCRIPTION_REGEX).clone(),
    };

    // A title given on the command line is TRACED; a database is REPORTED ON. The two are
    // different questions -- what did the rules do to this title, and what do the rules do to these
    // titles -- and a trace repeated a hundred thousand times answers neither.
    let report = if what.fasta.is_empty() && what.table.is_empty() && what.stitle.is_empty() {
        // Nothing to read: the lists are the subject. What they say about each other needs no
        // database at all, and is the one part of the report that belongs in a build.
        report::consistency(&rules, &split_regex)
    } else if what.fasta.is_empty() && what.table.is_empty() {
        let mut traced = String::new();
        for stitle in read_stitles(&what.stitle)? {
            traced.push_str(&explain_stitle(
                &stitle,
                &rules,
                &non_informative,
                &split_regex,
            ));
        }
        traced
    } else {
        rules.set_columns(&what.header, 1)?;
        rules.set_field_separator(&what.field_separator)?;
        // At most one second configuration: a candidate and a baseline answer different questions
        // -- what would this rule do, and what did my edit do -- and reporting both against one set
        // of counts would leave the reader to work out which difference is which.
        let candidate = compare::with_candidates(&rules, &what.try_rule)?;
        let baseline = compare::with_baseline(&rules, &what.baseline)?;
        if candidate.is_some() && baseline.is_some() {
            return Err(Error::Usage(String::from(
                "\n\n--try and --baseline ask different questions -- what would this rule do, and \
                 what did my edit do -- and answering both against one set of counts leaves it \
                 unclear which difference is which. Give one at a time.\n\n",
            )));
        }
        let variant = candidate.or(baseline);
        report::report(
            &rules,
            &non_informative,
            &split_regex,
            &what.fasta,
            &what.table,
            variant.as_ref(),
            what.rows,
        )?
    };

    // A report over a whole database is a thing to keep beside the rule list it is about, and a
    // LABBOOK entry cites a file rather than a scrollback.
    if what.output != "-" {
        return std::fs::write(&what.output, report.as_bytes()).map_err(|e| {
            Error::Io(format!(
                "\n\nCould not write the report to {:?}: {}\n\n",
                what.output, e
            ))
        });
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(report.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| Error::Io(format!("\n\nCould not write the explanation: {}\n\n", e)))
}

/// The sequence titles to explain: the ones given, with a single dash standing for the lines of
/// standard input, so that `cut -f 3 hits.tsv | prot-scriber explain --stitle -` puts a whole
/// search result through a candidate list of expressions.
///
/// # Arguments
///
/// * `given` - The `--stitle` arguments.
fn read_stitles(given: &[String]) -> Result<Vec<String>, Error> {
    let mut stitles: Vec<String> = vec![];
    for stitle in given {
        if stitle == "-" {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let line = line.map_err(|e| {
                    Error::Io(format!(
                        "\n\nAn error occurred reading the sequence titles from standard input: {}\n\n",
                        e
                    ))
                })?;
                if !line.trim().is_empty() {
                    stitles.push(line);
                }
            }
        } else {
            stitles.push(stitle.clone());
        }
    }
    Ok(stitles)
}

/// Renders what became of one sequence title.
///
/// # Arguments
///
/// * `stitle` - The sequence title, as a search result would carry it.
/// * `rules` - The blacklist, filter expressions and capture-replace pairs to apply.
/// * `non_informative` - The expressions that recognise a word that carries no information.
/// * `split_regex` - The expression that splits a description into words.
fn explain_stitle(
    stitle: &str,
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
) -> String {
    // Everything below reports what `hit_description` did; none of it decides anything. That is
    // the point: the order the stages are applied in is stated where they are applied, and this
    // is a rendering of the record they left.
    let mut steps = Steps::default();
    let description = rules.hit_description(stitle, Some(&mut steps));

    let mut out = format!("stitle       {}\n", stitle);

    // A blacklisted title never becomes a description at all, so there is nothing further to say
    // about it -- and saying which expression discarded it is the whole answer:
    if let Some(discarded_by) = &steps.discarded_by {
        out.push_str(&format!(
            "blacklist    discarded by {}\n\nThis hit is not used at all: no part of this title \
             reaches the scoring.\n\n",
            discarded_by
        ));
        return out;
    }
    out.push_str(&format!(
        "blacklist    kept, none of the {} expressions matched\n",
        rules.blacklist_regexs.len()
    ));

    out.push_str(&format!(
        "filter       {} of {} expressions changed it\n",
        steps.filtered.len(),
        rules.filter_regexs.len()
    ));
    for step in &steps.filtered {
        out.push_str(&format!("               {}\n", step.rule));
        out.push_str(&format!("                 -> {:?}\n", step.result));
    }
    out.push_str(&format!("lower case   {:?}\n", steps.lowered));
    out.push_str(&format!(
        "rewrite      {} of {} capture-replace pairs changed it\n",
        steps.rewritten.len(),
        rules.capture_replace_pairs.len()
    ));
    for step in &steps.rewritten {
        out.push_str(&format!(
            "               {}  ->  {:?}\n",
            step.rule,
            step.replacement.as_deref().unwrap_or("")
        ));
        out.push_str(&format!("                 -> {:?}\n", step.result));
    }
    if steps.lowered_again {
        out.push_str("lower case   again, a replacement above having put a capital back\n");
    }

    let description = match description {
        Some(description) => description,
        None => {
            out.push_str(
                "description  nothing is left of the title\n\nThis hit is not used at all: an \
                 empty description reaches no scoring.\n\n",
            );
            return out;
        }
    };
    out.push_str(&format!("description  {}\n", description));

    let words = split_descriptions(&description, split_regex);
    out.push_str(&format!("words        {}\n", words.join(", ")));
    let uninformative: Vec<&String> = words
        .iter()
        .filter(|word| matches_blacklist(word, non_informative))
        .collect();
    out.push_str(&format!(
        "             not scored: {}\n",
        if uninformative.is_empty() {
            String::from("none")
        } else {
            uninformative
                .iter()
                .map(|word| (*word).clone())
                .collect::<Vec<String>>()
                .join(", ")
        }
    ));
    out.push('\n');
    out
}
