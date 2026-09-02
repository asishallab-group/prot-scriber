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

use crate::description::{matches_any_regex, Steps};
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
/// nr has hundreds of millions of titles. A cap is a worse answer than counting everything and
/// dropping the rare words afterwards, which is what the deleted corpus verb could do; this drops
/// them by refusing to start. It
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
    /// Titles it came from, so a row can be recognised without going back to the data. Several,
    /// because one is enough to recognise a WORD and not enough to recognise a CLASS: three locus
    /// tags from three genomes say more than one does.
    samples: Vec<String>,
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
    /// What each capture-replace pair made and destroyed, by its index in the list.
    pair_effects: HashMap<usize, PairEffect>,
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
    /// Tokens of this shape and the titles they stood in, paired.
    samples: Vec<(String, String)>,
}

/// What one capture-replace pair did to the words of the descriptions it fired on.
#[derive(Debug, Default, Clone)]
struct PairEffect {
    /// How many descriptions it changed.
    fired: u64,
    /// Words that were there before it fired and gone after. THE SIDE THAT HAS NEVER BEEN
    /// VISIBLE: a pair that eats `cd5` and leaves `cd` has destroyed a name, and nothing said so.
    destroyed: HashMap<String, u64>,
    /// Words that were not there before it fired and are there after.
    made: HashMap<String, u64>,
}

/// How many distinct made-or-destroyed words to remember per pair. A pair that fires on a whole
/// database can touch millions; the head of each list is what a reader acts on.
const MAX_PAIR_WORDS: usize = 10_000;

/// A character that stands in finished descriptions and is not one the split separates on.
#[derive(Debug, Default, Clone)]
struct CharStat {
    occurrences: u64,
    descriptions: u64,
    samples: Vec<String>,
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
    /// `None` for a stream, which has no path to record a hash against and cannot be read twice.
    digest: Option<String>,
}

/// Where the titles come from. Three kinds, and a report may take any mixture of them.
pub struct Inputs<'a> {
    /// Reference database FASTA paths; every `>` line is a title.
    pub fasta: &'a [String],
    /// Search result tables, counted once per subject sequence.
    pub table: &'a [String],
    /// Titles already read from a stream by `--stitle -`.
    pub piped: &'a [String],
}

/// How much of each section to print.
pub struct Limits {
    /// Rows per section. Truncation always says what it hid.
    pub rows: usize,
    /// Titles shown under each row.
    pub samples: usize,
    /// Whether to write the section-keyed table instead of the report.
    pub tsv: bool,
}

/// What the rule lists say about each other, before a byte of data is read.
///
/// Every other section of this report is a count over titles. This one is a property of the lists
/// themselves, which is what lets it run in CI: a rule that cannot fire is a rule that cannot fire
/// whatever database it is pointed at.
///
/// # Arguments
///
/// * `rules` - The blacklist, filter expressions and capture-replace pairs, resolved.
/// * `split_regex` - The expression that splits a description into words.
pub fn consistency(rules: &SeqSimTable, split_regex: &Regex) -> String {
    let mut rows: Vec<String> = vec![];

    // AN EXPRESSION IN BOTH THE BLACKLIST AND A FILTER LIST. They are applied to the same raw
    // title and the blacklist goes first, discarding the hit whole -- so the filter copy is not
    // merely redundant, it is unreachable. It is also a category confusion: one list decides
    // whether a hit is worth anything, the other decides which of its words to keep.
    let mut black: HashMap<&str, String> = HashMap::new();
    for (i, regex) in rules.blacklist_regexs.iter().enumerate() {
        let origin = rules
            .blacklist_regexs
            .origin(i)
            .map(|origin| origin.to_string())
            .unwrap_or_else(|| String::from("?"));
        black.insert(regex.as_str(), origin);
    }
    for (i, regex) in rules.filter_regexs.iter().enumerate() {
        if let Some(shadowing) = black.get(regex.as_str()) {
            rows.push(format!(
                "  {:<28} can never fire: the same expression is {}, and the blacklist\n\
                 \x20                              is applied first, to the same title.\n      {}\n",
                rules
                    .filter_regexs
                    .origin(i)
                    .map(|origin| origin.to_string())
                    .unwrap_or_else(|| String::from("?")),
                shadowing,
                regex.as_str()
            ));
        }
    }

    // A REPLACEMENT THAT WRITES A CHARACTER THE SPLIT SEPARATES ON. A pair that joins two things
    // with a character the split then cuts at has done nothing at all -- which is how every DUF
    // family came to collapse into the single word `duf`, the sentinel having been in the split
    // class and the pair's own comment saying the opposite.
    for (i, (regex, replacement)) in rules.capture_replace_pairs.iter().enumerate() {
        if let Some(c) = separator_between_groups(replacement, split_regex) {
            rows.push(format!(
                "  {:<28} writes {:?}, which the split expression separates on, so what it\n\
                 \x20                              joins is taken apart again.\n      {}  ->  {:?}\n",
                rules
                    .capture_replace_pairs
                    .origin(i)
                    .map(|origin| origin.to_string())
                    .unwrap_or_else(|| String::from("?")),
                c,
                regex.as_str(),
                replacement
            ));
        }
    }

    let mut out = format!("\nCONSISTENCY   {} finding(s), read from the lists alone\n", rows.len());
    out.push_str(
        "  What the lists say about each other, with no database involved. A rule that cannot\n\
         \x20 fire cannot fire whatever it is pointed at, so this is the one check that belongs\n\
         \x20 in a build rather than in a run.\n",
    );
    if rows.is_empty() {
        out.push_str("\n  none.\n");
        return out;
    }
    out.push('\n');
    for row in rows {
        out.push_str(&row);
    }
    out
}

/// A separator character written BETWEEN two capture groups, which is a join the split undoes.
///
/// Only between two groups. A pair whose replacement ENDS in a space is separating on purpose --
/// `$first ` is how the gene-name pair cuts a number off a name -- and flagging that would be a
/// warning on a correct configuration, which is what stops warnings being read. What is wrong is a
/// pair that puts two captures together with a character the split then cuts at: the join has done
/// nothing, and that is how every DUF family came to collapse into the single word `duf`.
///
/// # Arguments
///
/// * `replacement` - The replacement half of a capture-replace pair.
/// * `split_regex` - The expression that splits a description into words.
fn separator_between_groups(replacement: &str, split_regex: &Regex) -> Option<char> {
    let mut chars = replacement.char_indices().peekable();
    let mut last_group_end: Option<usize> = None;
    let mut pending: Option<(usize, char)> = None;
    while let Some((i, c)) = chars.next() {
        if c == '$' {
            // `$name`, `$1` or `${name}` -- a reference to what the expression captured.
            let mut end = i + c.len_utf8();
            if chars.peek().map(|(_, n)| *n) == Some('{') {
                for (j, n) in chars.by_ref() {
                    end = j + n.len_utf8();
                    if n == '}' {
                        break;
                    }
                }
            } else {
                while let Some((j, n)) = chars.peek() {
                    if n.is_alphanumeric() || *n == '_' {
                        end = j + n.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
            }
            // A separator seen since the previous group now sits between two of them.
            if last_group_end.is_some() {
                if let Some((_, sep)) = pending {
                    return Some(sep);
                }
            }
            last_group_end = Some(end);
            pending = None;
        } else if split_regex.is_match(&c.to_string()) {
            pending = Some((i, c));
        }
    }
    None
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
    inputs: &Inputs,
    variant: Option<&crate::explain::compare::Variant>,
    limits: &Limits,
) -> Result<String, Error> {
    let (rows, samples) = (limits.rows, limits.samples);
    let mut difference = crate::explain::compare::Difference::default();
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
    // Titles read from a stream: already in memory, because `--stitle -` collected them. They are
    // a database like any other here, and counted once each rather than once per subject, there
    // being no subject accession beside them.
    if !inputs.piped.is_empty() {
        let before = counts.titles;
        for stitle in inputs.piped {
            observe(
                stitle,
                rules,
                non_informative,
                split_regex,
                &mut counts,
                &slots,
                samples,
            );
            if let Some(variant) = variant {
                crate::explain::compare::observe(
                    stitle,
                    rules,
                    variant,
                    split_regex,
                    &mut difference,
                );
            }
        }
        reads.push(Read {
            kind: "stream",
            path: String::from("-"),
            titles: counts.titles - before,
            digest: None,
        });
    }
    for path in inputs.fasta {
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
                    samples,
                );
                if let Some(variant) = variant {
                    crate::explain::compare::observe(
                        stitle,
                        rules,
                        variant,
                        split_regex,
                        &mut difference,
                    );
                }
            }
        })?;
        reads.push(Read {
            kind: "fasta",
            path: path.clone(),
            titles: counts.titles - before,
            digest: Some(digest.finalize().to_hex().to_string()),
        });
    }
    // ONE set of seen accessions across every table, as `corpus build` does it: a reference
    // sequence's description is one description however many searches found it, and counting it
    // once per row would make this a report about the query set.
    let mut seen: HashSet<String> = HashSet::new();
    for path in inputs.table {
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
                    samples,
                        );
                        if let Some(variant) = variant {
                            crate::explain::compare::observe(
                                stitle,
                                rules,
                                variant,
                                split_regex,
                                &mut difference,
                            );
                        }
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
            digest: Some(digest.finalize().to_hex().to_string()),
        });
    }

    // A machine gets every row and no prose; a person gets the sections, ranked and truncated.
    // The counts are the same counts either way -- this is the last step, not a second pass.
    if limits.tsv {
        return Ok(render_tsv(rules, &counts, &reads));
    }
    let mut out = render(rules, split_regex, &counts, &reads, seen.len(), rows);
    if let Some(variant) = variant {
        // The titles that must not be damaged are put through both configurations too, and they
        // are not part of the user's data: they are prot-scriber's own memory of what correct
        // output looks like.
        crate::explain::compare::check_known_good(rules, variant, &mut difference);
        out.push_str(&crate::explain::compare::render(variant, &difference, rows));
    }
    Ok(out)
}

/// Keeps up to `wanted` distinct examples, in the order they were first seen.
///
/// The first few, not the last: a report read from the top wants the examples that came with the
/// evidence, and keeping the newest would make the same input give different rows depending on the
/// order the files were given in.
fn remember(kept: &mut Vec<String>, sample: String, wanted: usize) {
    if kept.len() < wanted && !kept.contains(&sample) {
        kept.push(sample);
    }
}

/// The same, for a token paired with the title it stood in.
fn remember_pair(kept: &mut Vec<(String, String)>, sample: (String, String), wanted: usize) {
    if kept.len() < wanted && !kept.iter().any(|held| held.0 == sample.0) {
        kept.push(sample);
    }
}

/// Puts one title through the very code an annotation run puts it through, and counts what happened.
fn observe(
    stitle: &str,
    rules: &SeqSimTable,
    non_informative: &[Regex],
    split_regex: &Regex,
    counts: &mut Counts,
    slots: &Slots,
    samples: usize,
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
    // WHAT EACH PAIR MADE AND DESTROYED. The text a pair was given is the text the pair before it
    // produced -- and the first was given the lower-cased description -- so the two word sets are
    // both to hand without recording anything further. Only pairs that CHANGED something are
    // recorded, and a pair that changed nothing has an empty difference anyway.
    let mut before_text: &str = &steps.lowered;
    for step in &steps.rewritten {
        if let Some(i) = slot(&slots.pairs, &step.rule) {
            counts.pairs.matched[i] += 1;
            let before: HashSet<String> = split_descriptions(before_text, split_regex)
                .into_iter()
                .collect();
            let after: HashSet<String> = split_descriptions(&step.result, split_regex)
                .into_iter()
                .collect();
            let effect = counts.pair_effects.entry(i).or_default();
            effect.fired += 1;
            for word in before.difference(&after) {
                if effect.destroyed.len() < MAX_PAIR_WORDS || effect.destroyed.contains_key(word) {
                    *effect.destroyed.entry(word.clone()).or_insert(0) += 1;
                }
            }
            for word in after.difference(&before) {
                if effect.made.len() < MAX_PAIR_WORDS || effect.made.contains_key(word) {
                    *effect.made.entry(word.clone()).or_insert(0) += 1;
                }
            }
        }
        before_text = &step.result;
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
        let stat = counts.shapes.entry(shape).or_default();
        remember_pair(
            &mut stat.samples,
            (token.to_string(), stitle.to_string()),
            samples,
        );
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
        let stat = counts.chars.entry(c).or_default();
        remember(&mut stat.samples, stitle.to_string(), samples);
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
            non_informative: matches_any_regex(word, non_informative),
            ..WordStat::default()
        });
        remember(&mut stat.samples, stitle.to_string(), samples);
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

/// The same counts, section-keyed, for something other than a person to read.
///
/// Every row carries its section as the first field, which is how samtools and bcftools `stats` are
/// read: `grep ^WORD | cut -f 2-` is a table and nothing has to understand the layout.
///
/// UNTRUNCATED AND UNRANKED, deliberately. Ranking and a row limit exist because a person reads
/// from the top and stops; a pipeline wants all of it, and quietly handing it the top twenty-five
/// would be the worst of both. `--rows` and `--sample` shape the report a person reads, not this,
/// and the header says so.
///
/// The consistency findings are not here: each is a sentence about two rules and why one cannot
/// fire, and pretending that into columns would lose the half that matters. `--format report` is
/// where they belong.
fn render_tsv(rules: &SeqSimTable, counts: &Counts, reads: &[Read]) -> String {
    let mut out = format!(
        "# prot-scriber {}\n\
         # section-keyed: every row's first field is its section. Untruncated and unranked --\n\
         # --rows and --sample shape the report a person reads, not this.\n",
        env!("CARGO_PKG_VERSION")
    );

    out.push_str("INPUT\tkind\tpath\ttitles\tdigest\n");
    for read in reads {
        out.push_str(&format!(
            "INPUT\t{}\t{}\t{}\t{}\n",
            read.kind,
            read.path,
            read.titles,
            read.digest.as_deref().unwrap_or("")
        ));
    }

    out.push_str("STAGE\twhat\ttitles\n");
    for (label, n) in [
        ("read", counts.titles),
        ("discarded", counts.discarded),
        ("emptied", counts.emptied),
        ("described", counts.described),
    ] {
        out.push_str(&format!("STAGE\t{}\t{}\n", label, n));
    }

    out.push_str("RULE\tstage\torigin\tchecked\tmatched\texpression\n");
    for (stage, list, tally) in [
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
            out.push_str(&format!(
                "RULE\t{}\t{}\t{}\t{}\t{}\n",
                stage,
                list.origin(i).unwrap_or_default(),
                tally.checked.get(i).copied().unwrap_or(0),
                tally.matched.get(i).copied().unwrap_or(0),
                list.expression(i)
            ));
        }
    }

    out.push_str("WORD\tword\toccurrences\tdescriptions\talone\tnot_scored\n");
    let mut words: Vec<(&String, &WordStat)> = counts.words.iter().collect();
    words.sort_by(|a, b| a.0.cmp(b.0));
    for (word, stat) in words {
        out.push_str(&format!(
            "WORD\t{}\t{}\t{}\t{}\t{}\n",
            word, stat.occurrences, stat.descriptions, stat.alone, stat.non_informative
        ));
    }

    out.push_str("SHAPE\tshape\ttokens\twords\tbare_numbers\n");
    let mut shapes: Vec<(&String, &TokenShape)> = counts.shapes.iter().collect();
    shapes.sort_by(|a, b| a.0.cmp(b.0));
    for (shape, stat) in shapes {
        out.push_str(&format!(
            "SHAPE\t{}\t{}\t{}\t{}\n",
            shape, stat.tokens, stat.words, stat.bare_numbers
        ));
    }

    out.push_str("CHAR\tcharacter\toccurrences\tdescriptions\n");
    let mut chars: Vec<(&char, &CharStat)> = counts.chars.iter().collect();
    chars.sort_by(|a, b| a.0.cmp(b.0));
    for (c, stat) in chars {
        out.push_str(&format!(
            "CHAR\t{}\t{}\t{}\n",
            c, stat.occurrences, stat.descriptions
        ));
    }

    out.push_str("PAIR\torigin\tfired\tdestroyed\tmade\n");
    let mut effects: Vec<(&usize, &PairEffect)> = counts.pair_effects.iter().collect();
    effects.sort_by(|a, b| a.0.cmp(b.0));
    for (i, effect) in effects {
        out.push_str(&format!(
            "PAIR\t{}\t{}\t{}\t{}\n",
            rules
                .capture_replace_pairs
                .origin(*i)
                .map(|origin| origin.to_string())
                .unwrap_or_default(),
            effect.fired,
            effect.destroyed.len(),
            effect.made.len()
        ));
    }
    out
}

/// The report itself.
fn render(
    rules: &SeqSimTable,
    split_regex: &Regex,
    counts: &Counts,
    reads: &[Read],
    subjects: usize,
    rows: usize,
) -> String {
    // The two registers are named once here rather than left to be inferred from a row. A word and
    // a token are prot-scriber's, and are lower-cased because that is what the description pipeline
    // does to them; a title is the database's own text, untouched. `klma_20055` beside
    // `Aga2p KLMA_20055` is those two things and not a discrepancy, and a reader should not have to
    // work that out.
    let mut out = format!(
        "# prot-scriber {}\n\
         # words and tokens are prot-scriber's, lower-cased as the rules leave them;\n\
         # a title is the database's own text, as it was read.\n\n",
        env!("CARGO_PKG_VERSION")
    );

    out.push_str("input\n");
    for read in reads {
        out.push_str(&format!(
            "  {:<7} {}\n            {} title(s)   {}\n",
            read.kind,
            read.path,
            thousands(read.titles),
            match &read.digest {
                Some(digest) => format!("blake3 {}", &digest[..16]),
                // A stream has no path to record a hash against and cannot be read a second time
                // to check one, so saying nothing is the honest answer rather than a hash of what
                // happened to arrive.
                None => String::from("not hashed: a stream cannot be read twice"),
            }
        ));
    }
    if subjects > 0 {
        out.push_str(&format!(
            "            counted once per subject sequence, not once per row \
             ({} distinct subjects)\n",
            thousands(subjects as u64)
        ));
        // A SEARCH RESULT IS NOT THE DATABASE, and the difference is not the same size for every
        // number below. Boilerplate is boilerplate in any sample -- a format word is in every
        // title whichever titles you took -- but a hit table holds only the sequences something
        // matched, which is a sample biased towards whatever the query proteome resembles, and the
        // rare words are exactly what such a sample distorts. The singleton count is the evidence
        // the identifier class rests on, so the caveat is stated where it applies rather than
        // being left for the reader to remember.
        out.push_str(
            "            a hit table is a SAMPLE of the database, biased towards what the queries\n\
             \x20           resemble: the format words below are unaffected, the 'seen once'\n\
             \x20           counts are inflated. --fasta reads the database itself.\n",
        );
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
    out.push_str(&taken_apart(counts, rows));
    out.push_str(&not_separated(counts, rows));
    out.push_str(&identifier_shaped(counts, rows));
    out.push_str(&pairs_made_and_destroyed(rules, counts));
    out.push_str(&never_fired(rules, counts));
    out.push_str(&consistency(rules, split_regex));
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
            "  {:<24} {:>7.1} %  of descriptions{}\n{}",
            word,
            100.0 * share,
            marker(stat),
            titles(&stat.samples)
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
fn taken_apart(counts: &Counts, rows_wanted: usize) -> String {
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
    for (shape, stat) in rows.iter().take(rows_wanted) {
        out.push_str(&format!(
            "  {:<20} {:>10} token(s) -> {:>8} word(s), {:>8} bare number(s)\n{}",
            shape,
            thousands(stat.tokens),
            thousands(stat.words),
            thousands(stat.bare_numbers),
            tokens_in_titles(&stat.samples)
        ));
    }
    if rows.len() > rows_wanted {
        out.push_str(&format!(
            "  ... and {} more, not shown.\n",
            thousands(rows.len() as u64 - rows_wanted as u64)
        ));
    }
    out
}

/// Characters that stand in descriptions and are neither part of a word nor separators.
fn not_separated(counts: &Counts, rows_wanted: usize) -> String {
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
    for (c, stat) in rows.iter().take(rows_wanted) {
        out.push_str(&format!(
            "  {:<6} {:>12} occurrence(s) in {:>12} description(s)\n{}",
            format!("{:?}", c),
            thousands(stat.occurrences),
            thousands(stat.descriptions),
            titles(&stat.samples)
        ));
    }
    out
}

/// What each capture-replace pair made of the words it touched, and what it took away.
///
/// The destroyed side is the one nothing could show before. `corpus diff` reports the words that
/// appeared and the words that went between two whole-database builds and attributes neither to a
/// rule, so the case that mattered -- a pair eating `cd5` and leaving `cd` -- was found by a person
/// holding in mind which rule had changed between the builds. Here the two word sets are the
/// description before the pair fired and after it fired, so the difference belongs to that pair by
/// construction, in one pass.
fn pairs_made_and_destroyed(rules: &SeqSimTable, counts: &Counts) -> String {
    let mut fired: Vec<(&usize, &PairEffect)> = counts
        .pair_effects
        .iter()
        .filter(|(_, effect)| effect.fired > 0)
        .collect();
    fired.sort_by(|a, b| b.1.fired.cmp(&a.1.fired).then(a.0.cmp(b.0)));

    let mut out = format!(
        "\nWHAT THE CAPTURE-REPLACE PAIRS MADE AND DESTROYED   {} of {} fired\n",
        fired.len(),
        rules.capture_replace_pairs.len()
    );
    out.push_str(
        "  A pair rewrites a description, so it can take a word away as easily as it can make\n\
         \x20 one. DESTROYED is the side that has never been visible: widening the gene-name pair\n\
         \x20 from two letters to three was argued for by 20,173 words APPEARING between two\n\
         \x20 whole-database builds, headed by wd40, sh3 and vp2 -- real names the old form had\n\
         \x20 been eating. Both sides are here, against the pair that did it.\n",
    );
    if fired.is_empty() {
        out.push_str("\n  none -- no pair changed a description.\n");
        return out;
    }
    out.push('\n');
    for (i, effect) in fired {
        out.push_str(&format!(
            "  {}   fired on {} description(s)\n",
            rules
                .capture_replace_pairs
                .origin(*i)
                .map(|origin| origin.to_string())
                .unwrap_or_else(|| String::from("?")),
            thousands(effect.fired)
        ));
        out.push_str(&format!(
            "      {}\n",
            rules.capture_replace_pairs[*i].0.as_str()
        ));
        out.push_str(&commonest("destroyed", &effect.destroyed));
        out.push_str(&commonest("made", &effect.made));
    }
    out
}

/// The commonest few of a set of words, on one line.
///
/// # Arguments
///
/// * `label` - `destroyed` or `made`.
/// * `words` - The words and how often each was touched.
fn commonest(label: &str, words: &HashMap<String, u64>) -> String {
    if words.is_empty() {
        return format!("      {:<10} none\n", label);
    }
    let mut ranked: Vec<(&String, &u64)> = words.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let shown: Vec<String> = ranked
        .iter()
        .take(8)
        .map(|(word, n)| format!("{} ({})", word, thousands(**n)))
        .collect();
    format!(
        "      {:<10} {} distinct   {}{}\n",
        label,
        thousands(words.len() as u64),
        shown.join(", "),
        if ranked.len() > 8 { ", ..." } else { "" }
    )
}

/// Words shaped like an identifier, and whether each stands alone or inside a description.
///
/// The alone/inside split is the whole of the difference between the two rules the August record
/// numbers 9 and 10: a code that IS the description wants a blacklist rule, and a code that is only
/// part of one cannot be reached by a blacklist at all and wants a capture-replace pair.
fn identifier_shaped(counts: &Counts, rows_wanted: usize) -> String {
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
    for (word, stat) in rows.iter().take(rows_wanted) {
        out.push_str(&format!(
            "  {:<24} {:>10} seen   {:>8} alone   {:>8} inside{}\n{}",
            word,
            thousands(stat.occurrences),
            thousands(stat.alone),
            thousands(stat.descriptions - stat.alone.min(stat.descriptions)),
            marker(stat),
            titles(&stat.samples)
        ));
    }
    if rows.len() > rows_wanted {
        out.push_str(&format!(
            "  ... and {} more, not shown.\n",
            thousands(rows.len() as u64 - rows_wanted as u64)
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

/// The titles kept for a row, one per line, each labelled as the database's own text.
fn titles(samples: &[String]) -> String {
    samples
        .iter()
        .map(|title| format!("      in title   {}\n", title))
        .collect()
}

/// The same, for a token and the title it stood in.
fn tokens_in_titles(samples: &[(String, String)]) -> String {
    samples
        .iter()
        .map(|(token, title)| format!("      token   {}\n      in title   {}\n", token, title))
        .collect()
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
