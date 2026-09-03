//! Applying rule lists to a description: the blacklist that discards a hit, the filter
//! expressions that delete substrings of one, and the capture-replace pairs that rewrite one.
//! The regular expressions are either prot-scriber's compiled in defaults (see `crate::default`)
//! or read from the files passed to the respective command line arguments (see
//! `crate::input::regex_files`).
//!
//! THIS SERVES BOTH ENDS OF THE RUN, which is why it is written as "a description" rather than
//! "a sequence title". Going IN, the raw `stitle` of every hit is prepared here into the short
//! description prot-scriber scores (`crate::input::seq_sim_table`, in the parsing thread).
//! Coming OUT, `apply_capture_replace_pairs` is used once more on the FINISHED human readable
//! description, to polish it (`--polish-capture-replace-pairs`, in
//! `crate::annotation_process`). Same operation, different lists, opposite ends.

use crate::default::MAX_MATCH_REPLACE_ITERATIONS;
use crate::input::regex_files::{PairList, RuleList};
use regex::Regex;

/// One expression of one list, named where it can be.
///
/// The text alone does not identify a rule: `(?i)\bprobable\b` stands in `blacklist-regexs` line 11
/// and in `filter-regexs-uniprot` line 72, and the two mean opposite things. `origin` is `None`
/// only for expressions that were not read from a list of lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// Where it stands, as `list:line`.
    pub origin: Option<String>,
    /// The expression, as it stands in the list it was read from.
    pub expression: String,
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.origin {
            Some(origin) => write!(f, "{}  ({})", self.expression, origin),
            None => write!(f, "{}", self.expression),
        }
    }
}

/// One expression that changed a sequence title on its way to becoming a description, and what the
/// title became.
///
/// Recorded only when someone asked to see it -- `prot-scriber explain --stitle` -- and by the
/// same code that does the work when nobody did, so that what is shown cannot drift away from what
/// happens.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The expression that changed the title, and where it stands.
    pub rule: Rule,
    /// What the match was replaced with. `None` for the filter expressions, which delete.
    pub replacement: Option<String>,
    /// The title after this expression had been applied.
    pub result: String,
}

/// What became of a sequence title at each stage of turning it into a description. Only the
/// expressions that changed something are recorded; a list of twenty-six that did nothing is not
/// an explanation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Steps {
    /// The blacklist expression that discarded the title, if one did. Nothing further happened to
    /// it: the hit is not used at all.
    pub discarded_by: Option<Rule>,
    /// How many blacklist expressions were TESTED against this title.
    ///
    /// The scan stops at the first match, so this is the index of the match plus one, or the whole
    /// list when none matched. It is the only thing about a rule that did nothing which cannot be
    /// derived afterwards, and it is what keeps a report from dividing by the wrong number: an
    /// expression below the one that matched was not offered this title at all, and "0 of 81,806"
    /// for a rule that saw four hundred is the sort of statement this record exists to prevent.
    ///
    /// The filter expressions and the capture-replace pairs need no such field: they are applied
    /// unconditionally, so every one of them was checked on every title that reached its stage.
    pub blacklist_checked: usize,
    /// The filter expressions that changed the title.
    pub filtered: Vec<Step>,
    /// The title after lower-casing, which is where the capture-replace pairs start.
    pub lowered: String,
    /// The capture-replace pairs that changed it.
    pub rewritten: Vec<Step>,
    /// Whether the lower-casing that follows the pairs changed anything. It can only do so for a
    /// pair whose replacement carries a capital, prot-scriber's own introducing none.
    pub lowered_again: bool,
    /// Whether nothing was left of the title, so the hit contributes no description.
    pub emptied: bool,
}

/// Applies a list of regular expressions to a string. If any of them match returns true,
/// otherwise returns false.
/// This function is used to exclude non-informative descriptions from a human readable
/// sequence title and to check for non-informative words to be excluded from scoring
///
/// # Arguments
///
/// * testee - The text to be tested for any matching argument regular expression (`regexs`)
/// * regexs - A vector of regular expression to be applied to the testee argument.
pub fn matches_any_regex(testee: &str, regexs: &[Regex]) -> bool {
    regexs.iter().any(|x| x.is_match(testee))
}

/// Fasta entries (`stitle` in sequence similarity search, e.g. Blast output) have a long title in
/// which the sequence identifier and often taxonomic information is given along with a short human
/// readable protein description. We are only interested in the latter. This function extracts the
/// short description using regular expressions.
///
/// Given `steps`, it records what each expression made of the title on the way, which is what
/// `prot-scriber explain --stitle` reports. There is no second implementation to keep in step with
/// this one: an account of what prot-scriber does to a description is worth having only if it is
/// produced by the code that does it, and the recording costs one branch per expression when
/// nobody is watching.
///
/// # Arguments
///
/// * stitle - The sequence title line as found in the original Fasta file.
/// * regexs - A vector of regular expressions to be applied in series to the argument stitle to
///   extract the desired short description.
/// * `capture_replace_pairs` - An `Option` of a vector of tuples, pairing a regular expression
///   (see crate fancy-regex for details on the syntax) and the
///   capture-group replacement string. These are iteratively applied and
///   the argument descriptions to prepare it for final splitting into
///   words (see `split_descriptions` for details).
/// * `steps` - Where to record what happened, or `None` to do the work and say nothing.
pub fn filter_stitle(
    stitle: &str,
    regexs: &RuleList,
    capture_replace_pairs: Option<&PairList>,
    mut steps: Option<&mut Steps>,
) -> String {
    let mut desc = stitle.to_string();
    for (i, regex) in regexs.iter().enumerate() {
        let after = regex.replace_all(&desc, "").to_string();
        if let Some(steps) = steps.as_deref_mut() {
            if after != desc {
                steps.filtered.push(Step {
                    rule: rule_at(regexs, i),
                    replacement: None,
                    result: after.clone(),
                });
            }
        }
        desc = after;
    }
    desc = desc.to_lowercase();
    if let Some(steps) = steps.as_deref_mut() {
        steps.lowered = desc.clone();
    }
    apply_capture_replace_pairs_recording(
        &mut desc,
        capture_replace_pairs,
        steps.as_deref_mut().map(|steps| &mut steps.rewritten),
    );
    // A capture-replace pair may put back what the lower-casing above took away: its replacement
    // is a string the user wrote, and nothing stops it carrying capitals. This used to be done
    // once more by the caller that reads the input tables, and only by that caller, so a title put
    // through 'explain --stitle' came out differently from the same title in a run:
    let lowered = desc.to_lowercase();
    if let Some(steps) = steps {
        steps.lowered_again = lowered != desc;
    }
    // Remove preceding and trailing whitespaces, and return:
    lowered.trim().to_string()
}

/// The first blacklist expression that matches, which is the one that discarded the description.
///
/// # Arguments
///
/// * `testee` - The text to be tested.
/// * `regexs` - The expressions to test it against.
pub fn first_blacklist_match(testee: &str, regexs: &RuleList) -> (Option<Rule>, usize) {
    match regexs.iter().position(|regex| regex.is_match(testee)) {
        // The match is the last one tested, so the count is its index plus one.
        Some(i) => (Some(rule_at(regexs, i)), i + 1),
        None => (None, regexs.len()),
    }
}

/// The `i`th expression of `regexs`, with where it stands if the list knows.
///
/// # Arguments
///
/// * `regexs` - The list.
/// * `i` - The index of the expression within it.
fn rule_at(regexs: &RuleList, i: usize) -> Rule {
    Rule {
        origin: regexs.origin(i).map(|origin| origin.to_string()),
        expression: regexs[i].as_str().to_string(),
    }
}

/// Iteratively applies argument pairs of regular expressions (fancy-regex) and replace
/// instructions (strings) to change the string referenced by argument `s`.
///
/// # Arguments
///
/// * s - A reference to a String to be modified by iterative application of the argument
///   capture-replace-pairs.
/// * capture_replace_pairs - An `Option` containing a vector of tuples, within each the first
///   entry is a regular expression (fancy-regex) and a replace instruction (string).
pub fn apply_capture_replace_pairs(
    s: &mut String,
    capture_replace_pairs: Option<&PairList>,
) {
    apply_capture_replace_pairs_recording(s, capture_replace_pairs, None)
}

/// Does what `apply_capture_replace_pairs` does, and records the pairs that changed the string;
/// see `filter_stitle`.
///
/// # Arguments
///
/// * `s` - The string to modify.
/// * `capture_replace_pairs` - The pairs to apply.
/// * `steps` - Where to record what happened, or `None`.
pub fn apply_capture_replace_pairs_recording(
    s: &mut String,
    capture_replace_pairs: Option<&PairList>,
    mut steps: Option<&mut Vec<Step>>,
) {
    // Use regular expressions and replace with capture groups, if argument is given:
    if let Some(rr_tuples) = capture_replace_pairs {
        for (i, rr_tpl) in rr_tuples.iter().enumerate() {
            let before = if steps.is_some() { s.clone() } else { String::new() };
            for _ in 0..MAX_MATCH_REPLACE_ITERATIONS {
                if rr_tpl.0.is_match(s).unwrap() {
                    // `s` is a mutable reference. Set the value it points to to the string
                    // produced by `replace`:
                    *s = rr_tpl.0.replace(s, &rr_tpl.1).to_string();
                } else {
                    break;
                }
            }
            if let Some(steps) = steps.as_deref_mut() {
                if *s != before {
                    steps.push(Step {
                        rule: Rule {
                            origin: rr_tuples.origin(i).map(|origin| origin.to_string()),
                            expression: rr_tpl.0.as_str().to_string(),
                        },
                        replacement: Some(rr_tpl.1.clone()),
                        result: s.clone(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::assets::DefaultList;
    use crate::input::regex_files::parse_rules;

    /// `filter_stitle` with nothing recorded, which is what every test below but the two about
    /// recording wants.
    ///
    /// # Arguments
    ///
    /// * `stitle` - The sequence title.
    /// * `regexs` - The filter expressions.
    /// * `pairs` - The capture-replace pairs.
    fn filtered(
        stitle: &str,
        regexs: &RuleList,
        pairs: Option<&PairList>,
    ) -> String {
        filter_stitle(stitle, regexs, pairs, None)
    }
    use pretty_assertions::assert_eq;
    use crate::default::*;

    /// `prot-scriber explain --stitle` is worth having only because it is produced by the code
    /// that does the work rather than by a second account of it. This is what says the two have
    /// not come apart: recording the steps must change nothing about the description they lead to.
    #[test]
    fn recording_the_steps_changes_nothing_about_the_result() {
        for stitle in [
            "sp|Q9SX12|ADH1_ARATH At2g26220 Alcohol dehydrogenase 1 OS=Arabidopsis thaliana OX=3702 GN=ADH1 PE=1 SV=2",
            "tr|A0A1U8|A0A1U8_SOLTU Leucine-rich repeat receptor-like protein kinase OS=Solanum tuberosum",
            "XP_006345678.1 cytochrome P450 71A1-like [Solanum tuberosum]",
            "UniRef90_Q9SX12 Alcohol dehydrogenase n=12 Tax=Brassicaceae TaxID=3700 RepID=ADH1_ARATH",
            "",
            "   ",
        ] {
            let mut steps = Steps::default();
            assert_eq!(
                filter_stitle(
                    stitle,
                    &FILTER_REGEXS,
                    Some(&CAPTURE_REPLACE_DESCRIPTION_PAIRS),
                    None
                ),
                filter_stitle(
                    stitle,
                    &FILTER_REGEXS,
                    Some(&CAPTURE_REPLACE_DESCRIPTION_PAIRS),
                    Some(&mut steps)
                ),
                "the recorded run of {:?} produced a different description",
                stitle
            );
        }
    }

    /// Only the expressions that changed something are recorded: a list of twenty-five that did
    /// nothing is not an explanation of anything.
    #[test]
    fn the_recorded_steps_are_the_ones_that_changed_something() {
        let mut steps = Steps::default();
        let stitle = "sp|Q9SX12|ADH1_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana";
        let description = filter_stitle(
            stitle,
            &FILTER_REGEXS,
            Some(&CAPTURE_REPLACE_DESCRIPTION_PAIRS),
            Some(&mut steps),
        );
        assert!(
            steps.filtered.len() < FILTER_REGEXS.len(),
            "every expression was recorded, including the ones that did nothing"
        );
        assert!(!steps.filtered.is_empty(), "nothing at all was recorded");
        for step in &steps.filtered {
            assert_ne!(stitle, step.result, "a step that changed nothing was recorded");
        }
        // The last thing recorded is the last thing that happened, but for the trimming:
        assert_eq!(
            description,
            steps
                .rewritten
                .last()
                .map(|step| step.result.trim().to_string())
                .unwrap_or_else(|| steps.lowered.trim().to_string())
        );
    }

    /// RefSeq marks a sequence it is not confident in with a `LOW QUALITY PROTEIN:` prefix. It says
    /// nothing about what the protein does, and prot-scriber's own default filter list has removed
    /// it since 2024 -- but the NCBI-NR list, the one anyone searching RefSeq or NR is told to use,
    /// did not. Measured on `eggnog@6656/hits_refseq_protein.tsv` on 25.08.2026: 38,182 of
    /// 2,704,089 hit descriptions carry the prefix, and 7,073 of 55,063 descriptions generated from
    /// them began "quality protein".
    #[test]
    fn the_ncbi_nr_filter_regexs_remove_the_low_quality_prefix() {
        let ncbi_nr = crate::input::regex_files::parse_rules(
            crate::input::assets::FILTER_STITLE_REGEXS_NCBI_NR,
            "@filter-regexs-ncbi-nr",
        )
        .expect("the built-in NCBI-NR filter list does not parse");

        for stitle in [
            "XP_073996516.1 LOW QUALITY PROTEIN: centrin-1-like [Rhodnius prolixus]",
            "XP_040563780.1 low quality protein: e3 ubiquitin-protein ligase rnf19a [Danio rerio]",
        ] {
            let description = filter_stitle(stitle, &ncbi_nr, None, None);
            assert!(
                !description.contains("quality"),
                "the NCBI-NR list left the LOW QUALITY PROTEIN prefix in {:?}",
                description
            );
            assert!(
                !description.is_empty(),
                "the NCBI-NR list removed everything from {:?}",
                stitle
            );
        }
    }

    #[test]
    fn default_filter_regexs_extract_uni_prot_descriptions() {
        // Test 1:
        // No `Probable`: that word is the blacklist's, and a title carrying it is discarded whole
        // rather than trimmed, so a filter-only assertion about it describes nothing a run does.
        // What is under test here is the accession, the `-like` suffix, the locus code and the tail.
        let t1 = "sp|C0LGP4|Y3475_ARATH LRR receptor-like serine/threonine-protein kinase At3g47570 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1";
        assert_eq!(
            filtered(t1, &FILTER_REGEXS, None),
            "lrr receptor serine/threonine-protein kinase"
        );

        // Test 2 - using `default::REPLACE_REGEXS_DESCRIPTION`:
        let mut hit_words = "sp|C0LGP4|Y3475_ARATH receptor-like protein eix2 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1".to_string();
        let mut expected = "receptor protein eix";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 3 - using `default::REPLACE_REGEXS_DESCRIPTION`:
        hit_words = "sp|C0LGP4|Y3475_ARATH subtilisin-like protease sbt4.15 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1".to_string();
        expected = "subtilisin protease sbt";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 4 - using `default::REPLACE_REGEXS_DESCRIPTION`:
        hit_words = "sp|C0LGP4|Y3475_ARATH duf4228 domain protein OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1".to_string();
        expected = "duf~4228 domain protein";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 5 - removes "8-7" from input stitle "sp|Q6YZZ2|GL87_ORYSJ Germin-like protein 8-7 OS=Oryza sativa subsp. japonica OX=39947 GN=GER6 PE=2 SV=1":
        hit_words = "sp|Q6YZZ2|GL87_ORYSJ Germin-like protein 8-7 OS=Oryza sativa subsp. japonica OX=39947 GN=GER6 PE=2 SV=1".to_string();
        expected = "germin protein";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 6 remove duplicated words using default capture replace pairs:
        hit_words = "sp|Q6YZZ2|GL87_ORYSJ WRKY-like wrky-domain protein OS=Oryza sativa subsp. japonica OX=39947 GN=GER6 PE=2 SV=1".to_string();
        expected = "wrky domain protein";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 7 checks that the identifier is filtered out
        hit_words = "sp|Q9C8M9|SRF6_ARATH Protein STRUBBELIG-RECEPTOR FAMILY 6 OS=Arabidopsis thaliana OX=3702 GN=SRF6 PE=1 SV=1".to_string();
        expected = "protein strubbelig receptor family";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 8 also checks that no additional letters are deleted
        hit_words = "sp|Q6R2K2|SRF4_ARATH n Transferase Domain Containing Protein OS=Arabidopsis thaliana OX=3702 GN=SRF4 PE=2 SV=1".to_string();
        expected = "n transferase domain containing protein";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 9 also checks that no additional letters are deleted
        hit_words = "sp|Q6R2K2|SRF4_ARATH P Transferase Domain Containing Protein OS=Arabidopsis thaliana OX=3702 GN=SRF4 PE=2 SV=1".to_string();
        expected = "p transferase domain containing protein";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 10 checks that the Drosophila specific HRD description prefix 'LOW QUALITY
        // PROTEIN:' is removed:
        hit_words = "tr|A0A6P4E2J9|A0A6P4E2J9_DRORH LOW QUALITY PROTEIN: muscarinic acetylcholine receptor DM1 OS=Drosophila rhopaloa OX=1041015 GN=LOC108039593 PE=3 SV=1".to_string();
        expected = "muscarinic acetylcholine receptor dm1";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 11 checks that the Drosophila specific HRD description prefix 'Blast:' is removed:
        hit_words = "tr|A0A3B0K592|A0A3B0K592_DROGU Blast:Homeobox protein abdominal-A OS=Drosophila guanche OX=7266 GN=DGUA_6G017991 PE=3 SV=1".to_string();
        expected = "homeobox protein abdominal a";
        assert_eq!(
            expected,
            filtered(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );
    }

    #[test]
    fn default_matches_any_regex_regexs() {
        let t1 = "LRR receptor-like serine/threonine-protein kinase EFR";
        assert!(!matches_any_regex(t1, &BLACKLIST_STITLE_REGEXS));

        let t2 = "Probable LRR receptor-like serine/threonine-protein kinase At3g47570";
        assert!(matches_any_regex(t2, &BLACKLIST_STITLE_REGEXS));

        let t3 = "Putative receptor-like protein kinase At3g47110";
        assert!(matches_any_regex(t3, &BLACKLIST_STITLE_REGEXS));

        let t4 = "hypothetical receptor-like protein kinase At3g47110";
        assert!(matches_any_regex(t4, &BLACKLIST_STITLE_REGEXS));

        let t5 = "whole Genome shotgun Sequence";
        assert!(matches_any_regex(t5, &BLACKLIST_STITLE_REGEXS));

        let t6 = "predicted Receptor-like protein kinase";
        assert!(matches_any_regex(t6, &BLACKLIST_STITLE_REGEXS));
    }

    /// A description that is nothing but a locus code says nothing, whatever shape the code is.
    ///
    /// The rule was written for a code that ENDS in its digits, so every systematic name that
    /// carries a letter after them walked through it and became the finished description --
    /// 'ZYRO0A01628g' is one of 196 such words in Swiss-Prot's own vocabulary. What separates a
    /// locus code from a gene name is a run of four or more digits, not where the run sits.
    /// A locus code sitting INSIDE a real description has to go too, and the blacklist cannot
    /// reach it: the blacklist discards a whole description, so it is anchored, and a code with a
    /// description around it is not what it is anchored to. Swiss-Prot's own vocabulary carries
    /// 1,793 such words, 1,769 of them seen exactly once -- which is what an identifier looks
    /// like, and what a word of a description does not.
    ///
    /// A run of four or more digits is again what marks one. The thing this must not eat is a
    /// domain family accession, DUF4228 and its like, which the capture-replace pairs deliberately
    /// keep by joining prefix to number; the rule therefore runs AFTER that join, and the tilde
    /// the join leaves behind is what protects it.
    #[test]
    fn a_locus_code_inside_a_description_is_removed_and_a_domain_accession_is_not() {
        let cases = [
            // The shape that started this: a systematic name after a real description.
            (
                "sp|C5DPA1|YNF8_ZYGRC Vacuolar membrane protein ZYRO0A01628g OS=Zygosaccharomyces rouxii OX=559307 GN=ZYRO0A01628g PE=3 SV=1",
                "vacuolar membrane protein",
            ),
            // The same shape without a blacklisted word. `Uncharacterized`, which this fixture
            // used to open with, is the blacklist's now: a hit whose description is that is worth
            // nothing at all, and the run never reaches these expressions with it.
            (
                "sp|Q0CJ21|Y135_ASPTN Mitochondrial protein AO090001000135 OS=Aspergillus terreus OX=341663 PE=3 SV=1",
                "mitochondrial protein",
            ),
            // Two codes at once, one of them dotted.
            (
                "sp|Q8J1M8|YLO4_SCHPO UPF0768 protein C1952.04c OS=Schizosaccharomyces pombe OX=284812 PE=3 SV=1",
                "upf protein",
            ),
            // A domain family accession is a name, not an identifier, and must survive.
            (
                "sp|C0LGP4|Y3475_ARATH duf4228 domain protein OS=Arabidopsis thaliana OX=3702 PE=2 SV=1",
                "duf~4228 domain protein",
            ),
            // So must a gene name: no run of four digits, so nothing here is a locus code.
            (
                "sp|Q6NUK1|SCMC1_HUMAN Calcium-binding carrier protein slc25a24 OS=Homo sapiens OX=9606 PE=1 SV=1",
                "calcium binding carrier protein slc25a24",
            ),
        ];
        for (stitle, expected) in cases {
            assert_eq!(
                expected,
                filtered(
                    stitle,
                    &FILTER_REGEXS,
                    Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
                ),
                "filtering {:?}",
                stitle
            );
        }
    }

    #[test]
    fn a_locus_code_is_blacklisted_wherever_its_digits_sit() {
        // The one shape the old rule caught: letters, then the digits, then nothing.
        assert!(matches_any_regex("can6812812.1", &BLACKLIST_STITLE_REGEXS));
        // Every other shape walked through it -- including 'At3g47570', the Arabidopsis locus
        // this rule was written for, which only ever reached the blacklist when a hit happened
        // to say 'Probable' or 'Putative' in front of it.
        for code in [
            "At3g47570",
            "GRMZM2G702093",
            "ZYRO0A01628g",
            "AO090001000135",
            "A82775C",
            "BH02290",
            "A1Q3065",
        ] {
            assert!(
                matches_any_regex(code, &BLACKLIST_STITLE_REGEXS),
                "{} should be blacklisted", code
            );
        }
        // Gene names are not locus codes: no run of four digits, so the rule must leave them.
        // SLC25A24 and CYB561A3 are the shapes that make the run, and not the digit count,
        // the thing to test on.
        for name in ["TP53", "IL6", "SH3", "SLC25A24", "CYB561A3", "C18orf32"] {
            assert!(
                !matches_any_regex(name, &BLACKLIST_STITLE_REGEXS),
                "{} should not be blacklisted", name
            );
        }
        // Nor is a code that is merely PART of a description: the rule is anchored, because the
        // rest of the description is what says what the protein does.
        for description in ["Transposon Tn1545 resolvase", "Protein IS1081 helper"] {
            assert!(
                !matches_any_regex(description, &BLACKLIST_STITLE_REGEXS),
                "{} should not be blacklisted", description
            );
        }
    }

    /// An expression anchored with `^` makes a claim about the START of a title, and the start is
    /// the one part of a title whose shape every database fixes: UniProt writes `sp|` or `tr|`, the
    /// PDB writes an entry id, RefSeq and NR write an accession, UniRef writes `UniRef50_`. So an
    /// anchored expression is the one class of filter rule whose reachability can be settled without
    /// a database in hand -- and the only class where being ordered wrongly makes a rule
    /// unreachable rather than merely rare.
    ///
    /// Unanchored expressions are deliberately NOT checked: most of them fire on titles too rare to
    /// put in a fixture, and a check that fires on a correct configuration is noise.
    #[test]
    fn every_anchored_filter_expression_can_reach_a_title_its_database_writes() {
        // Real titles, in the shape each database actually writes them, plus one per database that
        // degenerates to punctuation and digits -- which is what the last expression of every list
        // is for, and which nothing else in this fixture would reach.
        let databases: Vec<(DefaultList, Vec<&str>)> = vec![
            (
                DefaultList::FilterRegexsUniprot,
                vec![
                    "sp|P00001|A_ARATH Receptor like protein kinase 1 OS=Arabidopsis thaliana OX=3702 GN=A PE=1 SV=1",
                    "tr|H0YKL1|H0YKL1_HUMAN Uncharacterized protein OS=Homo sapiens OX=9606 GN=X PE=4 SV=1",
                    "sp|P00002|B_ARATH 1.2.3.4 OS=Arabidopsis thaliana OX=3702 GN=B PE=1 SV=1",
                ],
            ),
            (
                DefaultList::FilterRegexsPdb,
                vec![
                    "9ab1_A mol:protein length:141 Hemoglobin alpha",
                    "9ab2_B mol:protein length:99 1.2.3.4",
                ],
            ),
            (
                DefaultList::FilterRegexsRefseq,
                vec![
                    "XP_002877578.1 alcohol dehydrogenase [Arabidopsis lyrata]",
                    "WP_000123456.1 MULTISPECIES: alcohol dehydrogenase [Bacteria]",
                    "XP_000000001.1 1.2.3.4 [Arabidopsis lyrata]",
                ],
            ),
            (
                DefaultList::FilterRegexsNcbiNr,
                vec![
                    "XP_002877578.1 alcohol dehydrogenase [Arabidopsis lyrata]",
                    "XP_000000001.1 1.2.3.4 [Arabidopsis lyrata]",
                ],
            ),
            (
                DefaultList::FilterRegexsUniref,
                vec![
                    "UniRef50_P00001 Alcohol dehydrogenase n=2 Tax=Bacteria TaxID=2 RepID=A_ARATH",
                    "UniRef50_P00002 1.2.3.4 n=1 Tax=Bacteria TaxID=2 RepID=B_ARATH",
                ],
            ),
        ];

        let mut unreachable: Vec<String> = vec![];
        for (list, titles) in databases {
            let rules = parse_rules(list.content(), &list.name()).unwrap();
            for (i, regex) in rules.iter().enumerate() {
                if !regex.as_str().contains('^') {
                    continue;
                }
                // The list is a fold, so an expression sees what the ones above it left behind.
                // Reachability has to be judged in that order, not against the raw title.
                let reached = titles.iter().any(|title| {
                    let mut desc = title.to_string();
                    for above in rules.iter().take(i) {
                        desc = above.replace_all(&desc, "").to_string();
                    }
                    regex.is_match(&desc)
                });
                if !reached {
                    unreachable.push(format!(
                        "  {} line {}: {}",
                        list.name(),
                        rules.origin(i).map(|o| o.line).unwrap_or(0),
                        regex.as_str()
                    ));
                }
            }
        }

        assert!(
            unreachable.is_empty(),
            "anchored expression(s) that no title of their own database can reach:\n{}",
            unreachable.join("\n")
        );
    }
}
