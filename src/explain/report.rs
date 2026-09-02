//! `prot-scriber explain --fasta` / `--table`: what a whole database's titles make of the rules.
//!
//! `explain --stitle` traces ONE title, and that is the wrong shape for maintaining a rule list.
//! The questions a list raises are about a database -- which of my twenty-seven expressions never
//! fires, how much of this database does that one delete, is the list I was given the list these
//! titles want -- and none of them can be answered one title at a time.
//!
//! WHAT THIS CAN SAY THAT A TRACE CANNOT. `Steps` records only the expressions that CHANGED
//! something, which is right for one title: a list of twenty-six that did nothing is not an
//! explanation. Over a database it is exactly backwards, because the interesting rule is the one
//! that never appears. So this counts by rule, not by title, and prints every expression of every
//! list including the ones whose count is zero. That is the number nothing in prot-scriber could
//! produce before, and the one that removed two dead PDB expressions in August 2026 -- on evidence
//! gathered by hand, outside the program.
//!
//! CHECKED AND MATCHED ARE TWO COLUMNS, and the difference is the blacklist's. Its scan stops at
//! the first match, so an expression below the one that matched was never offered the title at all;
//! "0 of 81,806" for a rule that saw four hundred would be a lie of just the kind this report
//! exists to prevent. The filter expressions and the capture-replace pairs are applied
//! unconditionally, so for them the two columns differ only where a stage was never reached.

use crate::description::{matches_blacklist, Steps};
use crate::error::Error;
use crate::hrd::split_descriptions;
use crate::input::lines::{for_each_line, thousands};
use crate::input::seq_sim_table::SeqSimTable;
use regex::Regex;
use std::collections::HashMap;
use std::collections::HashSet;

/// How many distinct words to learn before giving up on learning new ones.
///
/// The word table is the one thing here whose size follows the input rather than the rules, and
/// nr has hundreds of millions of titles. A cap is a worse answer than the corpus's `--min-count`,
/// which could drop the rare words after counting them; this drops them by refusing to start. It
/// is declared rather than silent, and the report says when it was reached -- because the singleton
/// columns below are evidence about exactly the rare words a cap throws away.
const MAX_TYPES: usize = 5_000_000;

/// The share of descriptions above which a word is a property of the FORMAT rather than of the
/// database.
///
/// No word of the language is in nine descriptions out of ten. `protein`, `domain`, `containing`
/// and `family` are the commonest things prot-scriber has to say and sit far below this;
/// `mol`, `length`, `ox=` and `pe=` sit at 100 %. The line is what keeps the MANUAL's own
/// readability invariant from being reported as a defect.
const FORMAT_SHARE: f64 = 0.90;

lazy_static! {
    /// A run of four or more digits, which is what tells an identifier from a gene name.
    ///
    /// The project's own discriminator, written into `assets/blacklist_stitle_regexs.txt` and
    /// `assets/capture_replace_pairs.txt`: `At3g47570`, `ZYRO0A01628g` and `KLMA_20055` have such a
    /// run and `TP53`, `IL6`, `SH3`, `SLC25A24` and `C18orf32` do not. Reporting by shape rather
    /// than by this would flag the second group, which is the negative ground truth.
    static ref IDENTIFIER_SHAPED: Regex = Regex::new(r"\d{4,}").unwrap();
}

/// What one word did across the input.
#[derive(Debug, Default, Clone)]
struct WordStat {
    /// How many descriptions held it, which is what a share is taken over.
    descriptions: u64,
    /// How many times it was seen, counting repeats within a description.
    occurrences: u64,
    /// How many descriptions were nothing BUT this word. That is the difference between a code
    /// that wants a blacklist rule and one that wants a capture-replace pair -- rules 9 and 10 of
    /// the August record, and the distinction nothing else computes.
    alone: u64,
    /// Whether the non-informative expressions recognise it. Counted anyway, and marked: that is
    /// the whole reason a manufactured `20055` can appear here at all.
    non_informative: bool,
    /// One title it came from, so a row can be recognised without going back to the data.
    sample: String,
}

/// How often each expression of one list was checked and how often it matched.
#[derive(Debug, Default)]
struct Tally {
    checked: Vec<u64>,
    matched: Vec<u64>,
}

impl Tally {
    fn of(len: usize) -> Tally {
        Tally {
            checked: vec![0; len],
            matched: vec![0; len],
        }
    }
}

/// Everything one pass over the input counted.
#[derive(Debug, Default)]
struct Counts {
    titles: u64,
    discarded: u64,
    emptied: u64,
    described: u64,
    blacklist: Tally,
    filter: Tally,
    pairs: Tally,
    /// Every word of every description, non-informative ones included and marked.
    words: HashMap<String, WordStat>,
    /// Whether the type cap was reached, so the report can say so rather than quietly under-count.
    capped: bool,
    /// The compound tokens the split took apart, by shape.
    shapes: HashMap<String, TokenShape>,
    /// The characters standing in descriptions that the split does not separate on.
    chars: HashMap<char, CharStat>,
}

/// What the split made of the compound tokens of one shape.
#[derive(Debug, Default, Clone)]
struct TokenShape {
    /// How many tokens of this shape were taken apart.
    tokens: u64,
    /// How many words they were cut into, in total.
    words: u64,
    /// How many of those words are nothing but digits. THIS is what the section is ranked by: a
    /// bare number is worth a fixed 1e-06 and joins whatever phrase it is next to, so a shape that
    /// manufactures them is manufacturing decisions.
    bare_numbers: u64,
    /// One token of this shape, and the title it stood in.
    sample: String,
    sample_title: String,
}

/// A character that stands in finished descriptions and is not one the split separates on.
#[derive(Debug, Default, Clone)]
struct CharStat {
    occurrences: u64,
    descriptions: u64,
    sample: String,
}

/// Where every expression of every list stands, so that a recorded step can be attributed to the
/// rule it came from. Keyed by `list:line`, which is unique by construction.
#[derive(Debug, Default)]
struct Slots {
    blacklist: HashMap<String, usize>,
    filter: HashMap<String, usize>,
    pairs: HashMap<String, usize>,
}

/// What was read, and what it hashed to.
struct Read {
    kind: &'static str,
    path: String,
    titles: u64,
    digest: String,
}

/// Reports what a whole set of titles makes of the rule lists.
///
/// # Arguments
///
/// * `rules` - The blacklist, filter expressions and capture-replace pairs, resolved as an
///   annotation run resolves them.
/// * `fasta` - Reference FASTA paths; every `>` line is a title. `-` is standard input.
/// * `table` - Search result table paths, counted once per subject sequence.
pub fn report(
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
    fasta: &[String],
    table: &[String],
) -> Result<String, Error> {
    let mut counts = Counts {
        blacklist: Tally::of(rules.blacklist_regexs.len()),
        filter: Tally::of(rules.filter_regexs.len()),
        pairs: Tally::of(rules.capture_replace_pairs.len()),
        ..Counts::default()
    };
    let mut slots = Slots::default();
    for i in 0..rules.blacklist_regexs.len() {
        if let Some(origin) = rules.blacklist_regexs.origin(i) {
            slots.blacklist.insert(origin.to_string(), i);
        }
    }
    for i in 0..rules.filter_regexs.len() {
        if let Some(origin) = rules.filter_regexs.origin(i) {
            slots.filter.insert(origin.to_string(), i);
        }
    }
    for i in 0..rules.capture_replace_pairs.len() {
        if let Some(origin) = rules.capture_replace_pairs.origin(i) {
            slots.pairs.insert(origin.to_string(), i);
        }
    }

    let mut reads: Vec<Read> = vec![];
    for path in fasta {
        let before = counts.titles;
        let mut digest = blake3::Hasher::new();
        for_each_line(path, &mut digest, |line| {
            if let Some(stitle) = line.strip_prefix('>') {
                observe(
                    stitle,
                    rules,
                    non_informative,
                    split_regex,
                    &mut counts,
                    &slots,
                );
            }
        })?;
        reads.push(Read {
            kind: "fasta",
            path: path.clone(),
            titles: counts.titles - before,
            digest: digest.finalize().to_hex().to_string(),
        });
    }
    // ONE set of seen accessions across every table, as `corpus build` does it: a reference
    // sequence's description is one description however many searches found it, and counting it
    // once per row would make this a report about the query set.
    let mut seen: HashSet<String> = HashSet::new();
    for path in table {
        let before = counts.titles;
        let mut digest = blake3::Hasher::new();
        let mut short: Option<usize> = None;
        for_each_line(path, &mut digest, |line| {
            let fields: Vec<&str> = line.split(rules.field_separator).collect();
            match (fields.get(rules.sacc_col), fields.get(rules.stitle_col)) {
                (Some(sacc), Some(stitle)) => {
                    if seen.insert((*sacc).to_string()) {
                        observe(
                            stitle,
                            rules,
                            non_informative,
                            split_regex,
                            &mut counts,
                            &slots,
                        );
                    }
                }
                _ => {
                    if short.is_none() {
                        short = Some(fields.len());
                    }
                }
            }
        })?;
        if let Some(fields) = short {
            return Err(Error::MalformedData(format!(
                "\n\nCannot read the table {:?}, because one of its lines splits into {} field(s) \
                 using the field separator {:?}, which is too few to hold the 'sacc' and 'stitle' \
                 columns. Check --header and --field-separator.\n\n",
                path, fields, rules.field_separator
            )));
        }
        reads.push(Read {
            kind: "table",
            path: path.clone(),
            titles: counts.titles - before,
            digest: digest.finalize().to_hex().to_string(),
        });
    }

    Ok(render(rules, &counts, &reads, seen.len()))
}

/// Puts one title through the very code an annotation run puts it through, and counts what happened.
fn observe(
    stitle: &str,
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
    counts: &mut Counts,
    slots: &Slots,
) {
    let mut steps = Steps::default();
    let description = rules.hit_description(stitle, Some(&mut steps));
    counts.titles += 1;

    // The blacklist is the only list whose scan stops early, so it is the only one whose `checked`
    // has to be recorded rather than derived.
    for i in 0..steps.blacklist_checked.min(counts.blacklist.checked.len()) {
        counts.blacklist.checked[i] += 1;
    }
    if let Some(rule) = &steps.discarded_by {
        counts.discarded += 1;
        if let Some(i) = slot(&slots.blacklist, rule) {
            counts.blacklist.matched[i] += 1;
        }
        return;
    }

    // Everything below was reached, so every one of its expressions was applied.
    for checked in counts.filter.checked.iter_mut() {
        *checked += 1;
    }
    for step in &steps.filtered {
        if let Some(i) = slot(&slots.filter, &step.rule) {
            counts.filter.matched[i] += 1;
        }
    }
    for checked in counts.pairs.checked.iter_mut() {
        *checked += 1;
    }
    for step in &steps.rewritten {
        if let Some(i) = slot(&slots.pairs, &step.rule) {
            counts.pairs.matched[i] += 1;
        }
    }

    let description = match description {
        Some(description) => {
            counts.described += 1;
            description
        }
        None => {
            counts.emptied += 1;
            return;
        }
    };

    // THE WORDS, non-informative ones INCLUDED. `Corpus::observe_description` skips them, which is
    // right when counting evidence for scoring and exactly wrong when looking for what a rule list
    // missed: `20055` and `22` are non-informative by the time anyone could see them, and they are
    // the artefacts worth seeing.
    // WHAT THE SPLIT TOOK APART. A whitespace token is what the title actually holds; anything the
    // split cuts into more than one word is a word the title never had. `KLMA_20055` is not in any
    // title -- `klma` and `20055` are what prot-scriber makes of it, and the bare number then joins
    // whatever phrase it is beside.
    for token in description.split_whitespace() {
        let made = split_descriptions(token, split_regex);
        if made.len() < 2 {
            continue;
        }
        let shape = token_shape(token);
        let stat = counts.shapes.entry(shape).or_insert_with(|| TokenShape {
            sample: token.to_string(),
            sample_title: stitle.to_string(),
            ..TokenShape::default()
        });
        stat.tokens += 1;
        stat.words += made.len() as u64;
        stat.bare_numbers += made
            .iter()
            .filter(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()))
            .count() as u64;
    }

    // AND WHAT IT DID NOT SEPARATE. A character that is neither part of a word nor a separator
    // holds two words together: `ox=1736528` is one word because `=` is in neither class.
    let mut seen_chars: HashSet<char> = HashSet::new();
    for c in description.chars() {
        if c.is_alphanumeric() || split_regex.is_match(&c.to_string()) {
            continue;
        }
        let stat = counts.chars.entry(c).or_insert_with(|| CharStat {
            sample: stitle.to_string(),
            ..CharStat::default()
        });
        stat.occurrences += 1;
        if seen_chars.insert(c) {
            stat.descriptions += 1;
        }
    }

    let words = split_descriptions(&description, split_regex);
    let alone = words.len() == 1;
    let mut seen_here: HashSet<&String> = HashSet::new();
    for word in &words {
        let known = counts.words.contains_key(word.as_str());
        if !known && counts.words.len() >= MAX_TYPES {
            counts.capped = true;
            continue;
        }
        let stat = counts.words.entry(word.clone()).or_insert_with(|| WordStat {
            non_informative: matches_blacklist(word, non_informative),
            sample: stitle.to_string(),
            ..WordStat::default()
        });
        stat.occurrences += 1;
        if seen_here.insert(word) {
            stat.descriptions += 1;
        }
        if alone {
            stat.alone += 1;
        }
    }
}

/// Which expression of a list a recorded step belongs to.
fn slot(slots: &HashMap<String, usize>, rule: &crate::description::Rule) -> Option<usize> {
    rule.origin.as_ref().and_then(|origin| slots.get(origin)).copied()
}

/// The report itself.
fn render(rules: &SeqSimTable, counts: &Counts, reads: &[Read], subjects: usize) -> String {
    let mut out = format!("# prot-scriber {}\n\n", env!("CARGO_PKG_VERSION"));

    out.push_str("input\n");
    for read in reads {
        out.push_str(&format!(
            "  {:<7} {}\n            {} title(s)   blake3 {}\n",
            read.kind,
            read.path,
            thousands(read.titles),
            &read.digest[..16]
        ));
    }
    if subjects > 0 {
        out.push_str(&format!(
            "            counted once per subject sequence, not once per row \
             ({} distinct subjects)\n",
            thousands(subjects as u64)
        ));
    }

    out.push_str("\nlists\n");
    out.push_str(&format!(
        "  {:<17} {:<30} {} expressions\n",
        "blacklist",
        list_name(rules.blacklist_regexs.origin(0).map(|o| o.list.to_string())),
        rules.blacklist_regexs.len()
    ));
    out.push_str(&format!(
        "  {:<17} {:<30} {} expressions\n",
        "filter",
        list_name(rules.filter_regexs.origin(0).map(|o| o.list.to_string())),
        rules.filter_regexs.len()
    ));
    out.push_str(&format!(
        "  {:<17} {:<30} {} pairs\n",
        "capture-replace",
        list_name(
            rules
                .capture_replace_pairs
                .origin(0)
                .map(|o| o.list.to_string())
        ),
        rules.capture_replace_pairs.len()
    ));

    out.push_str("\nstages\n");
    let percent = |n: u64| {
        if counts.titles == 0 {
            0.0
        } else {
            100.0 * n as f64 / counts.titles as f64
        }
    };
    for (label, n) in [
        ("read", counts.titles),
        ("discarded by the blacklist", counts.discarded),
        ("nothing left after the rules", counts.emptied),
        ("became a description", counts.described),
    ] {
        out.push_str(&format!(
            "  {:<32} {:>12} {:>7.1} %\n",
            label,
            thousands(n),
            percent(n)
        ));
    }

    let distinct = counts.words.len();
    let once = counts
        .words
        .values()
        .filter(|stat| stat.occurrences == 1)
        .count();
    let not_scored: u64 = counts
        .words
        .values()
        .filter(|stat| stat.non_informative)
        .map(|stat| stat.occurrences)
        .sum();
    let occurrences: u64 = counts.words.values().map(|stat| stat.occurrences).sum();
    out.push_str(&format!(
        "  {:<32} {:>12}           {} distinct, {} seen once\n",
        "words",
        thousands(occurrences),
        thousands(distinct as u64),
        thousands(once as u64)
    ));
    out.push_str(&format!(
        "  {:<32} {:>12} {:>7.1} %   counted here, and kept in the description\n",
        "of them not scored",
        thousands(not_scored),
        if occurrences == 0 {
            0.0
        } else {
            100.0 * not_scored as f64 / occurrences as f64
        }
    ));
    if counts.capped {
        out.push_str(&format!(
            "\n  ! stopped learning new words after {} distinct; the counts below are for the \
             words already known.\n",
            thousands(MAX_TYPES as u64)
        ));
    }

    out.push_str(&format_words(counts));
    out.push_str(&taken_apart(counts));
    out.push_str(&not_separated(counts));
    out.push_str(&identifier_shaped(counts));
    out.push_str(&never_fired(rules, counts));
    out
}

/// Words in nearly every description, which is what a database's title FORMAT looks like.
fn format_words(counts: &Counts) -> String {
    if counts.described == 0 {
        return String::new();
    }
    let mut rows: Vec<(f64, &String, &WordStat)> = counts
        .words
        .iter()
        .map(|(word, stat)| {
            (
                stat.descriptions as f64 / counts.described as f64,
                word,
                stat,
            )
        })
        .filter(|(share, _, _)| *share >= FORMAT_SHARE)
        .collect();
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap().then(a.1.cmp(b.1)));

    let mut out = format!(
        "\nWORDS IN NEARLY EVERY DESCRIPTION   {} at or above {:.0} %\n",
        rows.len(),
        100.0 * FORMAT_SHARE
    );
    out.push_str(
        "  A word this database's title FORMAT carries, rather than one this database SAYS.\n  \
         'protein', 'domain' and 'family' are common because proteins are, and sit far below\n  \
         this line; 'mol', 'length' and a UniProt tag sit at 100 %.\n",
    );
    if rows.is_empty() {
        out.push_str("\n  none.\n");
        return out;
    }
    out.push('\n');
    for (share, word, stat) in rows {
        out.push_str(&format!(
            "  {:<24} {:>7.1} %  of descriptions{}\n      {}\n",
            word,
            100.0 * share,
            marker(stat),
            stat.sample
        ));
    }
    out
}

/// The shape of one token, for grouping tokens that are all different and all the same problem.
///
/// A run of letters is `a`, a run of one to three digits is `9`, and a run of FOUR OR MORE is
/// `9999` -- kept distinct because that is the project's own discriminator between an identifier
/// and a gene name, and folding all digit runs together would put `At3g47570` and `SLC25A24` in one
/// row. Every other character stands for itself, because which character did the cutting is the
/// whole answer: `a_9999` says the underscore, `9.9a` says the full stop.
///
/// # Arguments
///
/// * `token` - One whitespace-delimited token of a description.
fn token_shape(token: &str) -> String {
    let mut out = String::new();
    let mut chars = token.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            let mut run = 1;
            while chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                chars.next();
                run += 1;
            }
            out.push_str(if run >= 4 { "9999" } else { "9" });
        } else if c.is_alphabetic() {
            if !out.ends_with('a') {
                out.push('a');
            }
            while chars.peek().is_some_and(|n| n.is_alphabetic()) {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The compound tokens the split took apart, and the bare numbers it made of them.
fn taken_apart(counts: &Counts) -> String {
    let mut rows: Vec<(&String, &TokenShape)> = counts.shapes.iter().collect();
    // Ranked by bare numbers made, NOT by how often the shape occurs: a bare number is worth a
    // fixed 1e-06 and joins whatever phrase it stands beside, so a shape that manufactures them is
    // manufacturing decisions. Ties fall back to how many tokens were cut.
    rows.sort_by(|a, b| {
        b.1.bare_numbers
            .cmp(&a.1.bare_numbers)
            .then(b.1.tokens.cmp(&a.1.tokens))
            .then(a.0.cmp(b.0))
    });

    let mut out = format!(
        "\nWHAT THE SPLIT TOOK APART   {} shape(s) of compound token\n",
        rows.len()
    );
    out.push_str(
        "  A word the title never held. The split cuts a compound token, and what comes out is\n\
         \x20 not what the database wrote: KLMA_20055 is in no title -- klma and 20055 are what\n\
         \x20 prot-scriber made of it. Ranked by the BARE NUMBERS made, because one of those is\n\
         \x20 worth a fixed 1e-06 and joins whatever phrase it stands beside, which is how\n\
         \x20 `aga2p 20055` beat `aga2p` by exactly that margin.\n\
         \x20 Shapes, not tokens: every locus tag is different and each occurs once.\n",
    );
    if rows.is_empty() {
        out.push_str("\n  none -- the split cut no token into more than one word.\n");
        return out;
    }
    out.push('\n');
    for (shape, stat) in rows.iter().take(25) {
        out.push_str(&format!(
            "  {:<20} {:>10} token(s) -> {:>8} word(s), {:>8} bare number(s)\n      {}   in   {}\n",
            shape,
            thousands(stat.tokens),
            thousands(stat.words),
            thousands(stat.bare_numbers),
            stat.sample,
            stat.sample_title
        ));
    }
    if rows.len() > 25 {
        out.push_str(&format!(
            "  ... and {} more, not shown.\n",
            thousands(rows.len() as u64 - 25)
        ));
    }
    out
}

/// Characters that stand in descriptions and are neither part of a word nor separators.
fn not_separated(counts: &Counts) -> String {
    let mut rows: Vec<(&char, &CharStat)> = counts.chars.iter().collect();
    rows.sort_by(|a, b| b.1.occurrences.cmp(&a.1.occurrences).then(a.0.cmp(b.0)));

    let mut out = format!(
        "\nCHARACTERS THE SPLIT DOES NOT SEPARATE ON   {} distinct\n",
        rows.len()
    );
    out.push_str(
        "  Each of these holds two words together. `ox=1736528` is ONE word because `=` is\n\
         \x20 neither part of a word nor a separator; `+` and `[` were counted as parts of words\n\
         \x20 until the split class gained them. Some belong: `~` is the sentinel the pairs join\n\
         \x20 a domain accession with, and a hyphen inside a chemical name is doing its job.\n",
    );
    if rows.is_empty() {
        out.push_str("\n  none.\n");
        return out;
    }
    out.push('\n');
    for (c, stat) in rows.iter().take(25) {
        out.push_str(&format!(
            "  {:<6} {:>12} occurrence(s) in {:>12} description(s)\n      {}\n",
            format!("{:?}", c),
            thousands(stat.occurrences),
            thousands(stat.descriptions),
            stat.sample
        ));
    }
    out
}

/// Words shaped like an identifier, and whether each stands alone or inside a description.
///
/// The alone/inside split is the whole of the difference between the two rules the August record
/// numbers 9 and 10: a code that IS the description wants a blacklist rule, and a code that is only
/// part of one cannot be reached by a blacklist at all and wants a capture-replace pair.
fn identifier_shaped(counts: &Counts) -> String {
    let mut rows: Vec<(&String, &WordStat)> = counts
        .words
        .iter()
        .filter(|(word, _)| IDENTIFIER_SHAPED.is_match(word))
        .collect();
    rows.sort_by(|a, b| b.1.occurrences.cmp(&a.1.occurrences).then(a.0.cmp(b.0)));
    let once = rows
        .iter()
        .filter(|(_, stat)| stat.occurrences == 1)
        .count();

    let mut out = format!(
        "\nWORDS SHAPED LIKE AN IDENTIFIER   {} distinct, {} seen exactly once\n",
        rows.len(),
        once
    );
    out.push_str(
        "  A run of four or more digits, which is what tells a code from a gene name: At3g47570\n  \
         and ZYRO0A01628g have one, TP53, IL6, SH3 and C18orf32 do not. Nearly all of them being\n  \
         seen once is what an identifier looks like.\n  \
         ALONE means the whole description was this word, which a blacklist rule can reach.\n  \
         INSIDE means it was part of a longer one, which only a capture-replace pair can.\n",
    );
    if rows.is_empty() {
        out.push_str("\n  none.\n");
        return out;
    }
    out.push('\n');
    for (word, stat) in rows.iter().take(25) {
        out.push_str(&format!(
            "  {:<24} {:>10} seen   {:>8} alone   {:>8} inside{}\n      {}\n",
            word,
            thousands(stat.occurrences),
            thousands(stat.alone),
            thousands(stat.descriptions - stat.alone.min(stat.descriptions)),
            marker(stat),
            stat.sample
        ));
    }
    if rows.len() > 25 {
        out.push_str(&format!(
            "  ... and {} more, not shown.\n",
            thousands(rows.len() as u64 - 25)
        ));
    }
    out
}

/// The section this whole verb was wanted for: the expressions that never fired.
fn never_fired(rules: &SeqSimTable, counts: &Counts) -> String {
    let mut rows: Vec<String> = vec![];
    let mut total = 0usize;
    for (label, list, tally) in [
        (
            "blacklist",
            RuleNames::Rules(&rules.blacklist_regexs),
            &counts.blacklist,
        ),
        (
            "filter",
            RuleNames::Rules(&rules.filter_regexs),
            &counts.filter,
        ),
        (
            "capture-replace",
            RuleNames::Pairs(&rules.capture_replace_pairs),
            &counts.pairs,
        ),
    ] {
        for i in 0..list.len() {
            total += 1;
            if tally.matched.get(i).copied().unwrap_or(0) > 0 {
                continue;
            }
            rows.push(format!(
                "  {:<15}  {:<28}  checked {:>12}   {}\n",
                label,
                list.origin(i).unwrap_or_else(|| String::from("?")),
                thousands(tally.checked.get(i).copied().unwrap_or(0)),
                list.expression(i)
            ));
        }
    }

    let mut out = format!(
        "\nRULES THAT NEVER FIRED   {} of {} expressions\n",
        rows.len(),
        total
    );
    out.push_str(
        "  A rule that matched nothing here either does not belong to these titles, or is\n  \
         pre-empted by one above it. CHECKED says which: a blacklist expression below the one\n  \
         that matched was never offered the title at all.\n",
    );
    if rows.is_empty() {
        out.push_str("\n  none -- every expression of every list fired at least once.\n");
    } else {
        out.push('\n');
        for row in rows {
            out.push_str(&row);
        }
    }
    out
}

/// The two shapes of list, so that one loop can walk either.
enum RuleNames<'a> {
    Rules(&'a crate::input::regex_files::RuleList),
    Pairs(&'a crate::input::regex_files::PairList),
}

impl RuleNames<'_> {
    fn len(&self) -> usize {
        match self {
            RuleNames::Rules(list) => list.len(),
            RuleNames::Pairs(list) => list.len(),
        }
    }

    fn origin(&self, i: usize) -> Option<String> {
        match self {
            RuleNames::Rules(list) => list.origin(i).map(|o| o.to_string()),
            RuleNames::Pairs(list) => list.origin(i).map(|o| o.to_string()),
        }
    }

    fn expression(&self, i: usize) -> String {
        match self {
            RuleNames::Rules(list) => list[i].as_str().to_string(),
            RuleNames::Pairs(list) => list[i].0.as_str().to_string(),
        }
    }
}

/// The note that a word carries no score, or nothing at all -- never a run of blanks, because
/// trailing whitespace in a report is noise in every diff of it.
fn marker(stat: &WordStat) -> &'static str {
    if stat.non_informative {
        "   not scored"
    } else {
        ""
    }
}

/// What to call a list whose expressions know where they came from, or `none` when it is empty.
fn list_name(name: Option<String>) -> String {
    name.unwrap_or_else(|| String::from("none"))
}
