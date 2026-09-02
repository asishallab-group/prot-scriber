//! Two configurations of the rules over one pass of the data: `--try` and `--baseline`.
//!
//! The loop this replaces is MANUAL 2.2.6's, and it is three commands and two full passes over the
//! reference database: build a corpus, read its head, edit a list, build the corpus again, diff the
//! two. `corpus diff` then reports which words moved and which rule TEXTS differ, and never joins
//! the two halves -- so the rule that moved a word is supplied by whoever remembers what they
//! changed. The evidence that widened the gene-name pair from two letters to three was found that
//! way, by one person holding a day's edits in mind.
//!
//! Here both configurations see the same title at the same moment, so the difference between them
//! IS the edit, with nothing to remember and nothing written to disk.

use crate::assets;
use crate::description::Steps;
use crate::error::Error;
use crate::hrd::split_descriptions;
use crate::input::lines::thousands;
use crate::input::seq_sim_table::{SeqSimTable, Stage};
use regex::Regex;
use std::collections::{HashMap, HashSet};

/// How one title differed between the two configurations.
#[derive(Debug, Default)]
pub struct Difference {
    /// Titles seen.
    pub titles: u64,
    /// Titles the other configuration discarded and this one kept, and the reverse.
    pub newly_discarded: u64,
    pub newly_kept: u64,
    /// Descriptions that came out differently at all.
    pub changed: u64,
    /// Words the change took away, and words it brought in.
    pub gone: HashMap<String, u64>,
    /// Words that appeared. A rule creates words as readily as it removes them, and this is the
    /// half that caught `wd40`, `sh3` and `vp2` being destroyed.
    pub appeared: HashMap<String, u64>,
    /// Titles of `assets/titles_that_must_not_be_damaged.txt` this changed, with what became of
    /// each. Empty is the answer that lets a rule be written.
    pub damaged: Vec<(String, String, String)>,
}

/// A second set of rules to run beside the first, and what to call the comparison.
pub struct Variant {
    pub heading: &'static str,
    pub note: String,
    pub rules: SeqSimTable,
    /// Whether this second configuration is the state AFTER the change.
    ///
    /// The two options ask the question from opposite ends, and getting this wrong inverts every
    /// word in the report. `--try` holds the candidate, so it is the after; `--baseline` holds the
    /// list as it WAS, so the rules in use are the after and the baseline is the before. A word
    /// the candidate deletes must read as gone, not as appeared.
    pub is_after: bool,
}

/// The rules with one candidate expression appended to the stage it belongs to.
///
/// Appended, not inserted: the lists are folds and an expression's position is part of its meaning,
/// and the end is where an edit would most likely put it. The report says so rather than leaving it
/// to be discovered.
///
/// # Arguments
///
/// * `base` - The rules in use.
/// * `args` - The `--try` arguments, each `STAGE:EXPRESSION`.
pub fn with_candidates(base: &SeqSimTable, args: &[String]) -> Result<Option<Variant>, Error> {
    if args.is_empty() {
        return Ok(None);
    }
    let mut rules = base.clone();
    let mut described: Vec<String> = vec![];
    for arg in args {
        let (stage, expression) = arg.split_once(':').ok_or_else(|| {
            Error::Usage(format!(
                "\n\nCannot read {:?}: a candidate is written STAGE:EXPRESSION, e.g. \
                 --try 'filter:(?i)\\bmol:\\S+'. The stages are 'blacklist', 'filter' and \
                 'capture-replace'.\n\n",
                arg
            ))
        })?;
        let stage = Stage::parse(stage, arg)?;
        rules.append_rule(stage, expression, "--try")?;
        described.push(format!("{}:{}", stage_name(stage), expression));
    }
    Ok(Some(Variant {
        heading: "THE CANDIDATE",
        note: format!(
            "  {}\n  Appended to the end of its list, which is where an edit would most likely put\n\
             \x20 it -- the lists are folds, so where an expression stands is part of what it means.\n\
             \x20 Nothing has been written to any file.\n",
            described.join("\n  ")
        ),
        rules,
        is_after: true,
    }))
}

/// The rules with one whole list replaced by the list as it was.
///
/// # Arguments
///
/// * `base` - The rules in use.
/// * `args` - The `--baseline` arguments, each `STAGE=SOURCE`.
pub fn with_baseline(base: &SeqSimTable, args: &[String]) -> Result<Option<Variant>, Error> {
    if args.is_empty() {
        return Ok(None);
    }
    let mut rules = base.clone();
    let mut described: Vec<String> = vec![];
    for arg in args {
        let (stage, source) = arg.split_once('=').ok_or_else(|| {
            Error::Usage(format!(
                "\n\nCannot read {:?}: a baseline is written STAGE=SOURCE, e.g. \
                 --baseline 'filter=@filter-regexs-ncbi-nr'. The stages are 'blacklist', 'filter' \
                 and 'capture-replace'.\n\n",
                arg
            ))
        })?;
        let stage = Stage::parse(stage, arg)?;
        match stage {
            Stage::Blacklist => rules.set_blacklist_regexs(source)?,
            Stage::Filter => rules.set_filter_regexs(source)?,
            Stage::CaptureReplace => rules.set_capture_replace_pairs(source)?,
        }
        described.push(format!("{} = {}", stage_name(stage), source));
    }
    Ok(Some(Variant {
        heading: "AGAINST THE BASELINE",
        note: format!(
            "  {}\n  Both configurations saw the same title at the same moment, so what is below IS\n\
             \x20 the edit -- one pass over the database, and no need to hold what the old list\n\
             \x20 did in your head while reading what the new one does.\n",
            described.join("\n  ")
        ),
        rules,
        is_after: false,
    }))
}

/// The name a stage is written under.
fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Blacklist => "blacklist",
        Stage::Filter => "filter",
        Stage::CaptureReplace => "capture-replace",
    }
}

/// Accumulates how one title came out under each configuration.
///
/// # Arguments
///
/// * `stitle` - The title.
/// * `base` - The rules in use.
/// * `variant` - The other configuration.
/// * `split_regex` - The expression that splits a description into words.
/// * `into` - Where to accumulate.
pub fn observe(
    stitle: &str,
    base: &SeqSimTable,
    variant: &Variant,
    split_regex: &Regex,
    into: &mut Difference,
) {
    into.titles += 1;
    let in_use = base.hit_description(stitle, None);
    let other = variant.rules.hit_description(stitle, None);
    // Which of the two is the state after the change, so that "gone" means gone and not the
    // opposite of it. See `Variant::is_after`.
    let (was, now) = if variant.is_after {
        (&in_use, &other)
    } else {
        (&other, &in_use)
    };
    match (was, now) {
        (Some(_), None) => into.newly_discarded += 1,
        (None, Some(_)) => into.newly_kept += 1,
        _ => {}
    }
    if was == now {
        return;
    }
    into.changed += 1;
    let before: HashSet<String> = was
        .as_deref()
        .map(|d| split_descriptions(d, split_regex).into_iter().collect())
        .unwrap_or_default();
    let after: HashSet<String> = now
        .as_deref()
        .map(|d| split_descriptions(d, split_regex).into_iter().collect())
        .unwrap_or_default();
    for word in before.difference(&after) {
        *into.gone.entry(word.clone()).or_insert(0) += 1;
    }
    for word in after.difference(&before) {
        *into.appeared.entry(word.clone()).or_insert(0) += 1;
    }
}

/// Runs the two configurations over the titles that must not be damaged.
///
/// # Arguments
///
/// * `base` - The rules in use, i.e. with the candidate.
/// * `variant` - The rules without it.
/// * `into` - Where to record what was damaged.
pub fn check_known_good(base: &SeqSimTable, variant: &Variant, into: &mut Difference) {
    for line in assets::TITLES_THAT_MUST_NOT_BE_DAMAGED.lines() {
        let title = line.trim();
        if title.is_empty() || title.starts_with('#') {
            continue;
        }
        let mut steps = Steps::default();
        let in_use = base.hit_description(title, Some(&mut steps));
        let other = variant.rules.hit_description(title, None);
        let (was, now) = if variant.is_after {
            (in_use, other)
        } else {
            (other, in_use)
        };
        if was != now {
            into.damaged.push((
                title.to_string(),
                was.unwrap_or_else(|| String::from("<discarded>")),
                now.unwrap_or_else(|| String::from("<discarded>")),
            ));
        }
    }
}

/// The comparison, rendered.
///
/// # Arguments
///
/// * `variant` - What the second configuration was.
/// * `difference` - What it came to.
/// * `rows` - How many words to show per side.
pub fn render(variant: &Variant, difference: &Difference, rows: usize) -> String {
    let mut out = format!(
        "\n{}   {} of {} description(s) changed\n",
        variant.heading,
        thousands(difference.changed),
        thousands(difference.titles)
    );
    out.push_str(&variant.note);
    out.push_str(&format!(
        "\n  discarded that were kept    {:>12}\n  kept that were discarded    {:>12}\n",
        thousands(difference.newly_discarded),
        thousands(difference.newly_kept)
    ));
    out.push_str(&words("gone", &difference.gone, rows));
    out.push_str(&words("appeared", &difference.appeared, rows));

    if variant.heading == "THE CANDIDATE" {
        out.push_str(&format!(
            "\n  of the titles prot-scriber must not damage, {} touched\n",
            difference.damaged.len()
        ));
        if difference.damaged.is_empty() {
            out.push_str("  none -- the candidate leaves every one of them as it was.\n");
        } else {
            for (title, was, now) in difference.damaged.iter().take(rows) {
                out.push_str(&format!(
                    "      {}\n        was {:?}\n        now {:?}\n",
                    title, was, now
                ));
            }
            if difference.damaged.len() > rows {
                out.push_str(&format!(
                    "      ... and {} more, not shown.\n",
                    difference.damaged.len() - rows
                ));
            }
        }
    }
    out
}

/// The commonest words of one side of the difference.
fn words(label: &str, words: &HashMap<String, u64>, rows: usize) -> String {
    if words.is_empty() {
        return format!("  {:<10} none\n", label);
    }
    let mut ranked: Vec<(&String, &u64)> = words.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let shown: Vec<String> = ranked
        .iter()
        .take(rows)
        .map(|(word, n)| format!("{} ({})", word, thousands(**n)))
        .collect();
    format!(
        "  {:<10} {} distinct   {}{}\n",
        label,
        thousands(words.len() as u64),
        shown.join(", "),
        if ranked.len() > rows { ", ..." } else { "" }
    )
}
