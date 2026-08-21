//! Turning the raw sequence titles (`stitle` in Blast terminology) found in sequence
//! similarity search results into the short descriptions prot-scriber scores and assembles
//! human readable descriptions from. The regular expressions applied here are either
//! prot-scriber's compiled in defaults (see `crate::default`) or read from the files passed to
//! the respective command line arguments (see `crate::input::regex_files`).

use crate::default::MAX_MATCH_REPLACE_ITERATIONS;
use regex::Regex;

/// Applies a list of regular expressions to a string. If any of them match returns true,
/// otherwise returns false.
/// This function is used to exclude non-informative descriptions from a human readable
/// sequence title and to check for non-informative words to be excluded from scoring
///
/// # Arguments
///
/// * testee - The text to be tested for any matching argument regular expression (`regexs`)
/// * regexs - A vector of regular expression to be applied to the testee argument.
pub fn matches_blacklist(testee: &str, regexs: &[Regex]) -> bool {
    regexs.iter().any(|x| x.is_match(testee))
}

/// Fasta entries (`stitle` in sequence similarity search, e.g. Blast output) have a long title in
/// which the sequence identifier and often taxonomic information is given along with a short human
/// readable protein description. We are only interested in the latter. This function extracts the
/// short description using regular expressions.
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
pub fn filter_stitle(
    stitle: &str,
    regexs: &[Regex],
    capture_replace_pairs: Option<&Vec<(fancy_regex::Regex, String)>>,
) -> String {
    let mut desc = regexs
        .iter()
        .fold(stitle.to_string(), |accumulated, current| {
            current.replace_all(&accumulated, "").to_string()
        })
        .to_lowercase();
    apply_capture_replace_pairs(&mut desc, capture_replace_pairs);
    // Remove preceding and trailing whitespaces, and return:
    desc.trim().to_string()
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
    capture_replace_pairs: Option<&Vec<(fancy_regex::Regex, String)>>,
) {
    // Use regular expressions and replace with capture groups, if argument is given:
    if let Some(rr_tuples) = capture_replace_pairs {
        for rr_tpl in rr_tuples {
            for _ in 0..MAX_MATCH_REPLACE_ITERATIONS {
                if rr_tpl.0.is_match(s).unwrap() {
                    // `s` is a mutable reference. Set the value it points to to the string
                    // produced by `replace`:
                    *s = rr_tpl.0.replace(s, &rr_tpl.1).to_string();
                } else {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use crate::default::*;

    #[test]
    fn default_filter_regexs_extract_uni_prot_descriptions() {
        // Test 1:
        let t1 = "sp|C0LGP4|Y3475_ARATH Probable LRR receptor-like serine/threonine-protein kinase At3g47570 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1";
        assert_eq!(
            filter_stitle(t1, &FILTER_REGEXS, None),
            "lrr receptor serine/threonine-protein kinase"
        );

        // Test 2 - using `default::REPLACE_REGEXS_DESCRIPTION`:
        let mut hit_words = "sp|C0LGP4|Y3475_ARATH receptor-like protein eix2 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1".to_string();
        let mut expected = "receptor protein eix";
        assert_eq!(
            expected,
            filter_stitle(
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
            filter_stitle(
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
            filter_stitle(
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
            filter_stitle(
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
            filter_stitle(
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
            filter_stitle(
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
            filter_stitle(
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
            filter_stitle(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );

        // Test 10 checks that the Drosophila specific HRD description prefix 'LOW QUALITY
        // PROTEIN:' is removed:
        hit_words = "tr|A0A6P4E2J9|A0A6P4E2J9_DRORH LOW QUALITY PROTEIN: muscarinic acetylcholine receptor DM1 OS=Drosophila rhopaloa OX=1041015 GN=LOC108039593 PE=3 SV=1".to_string();
        expected = "muscarinic acetylcholine receptor dm";
        assert_eq!(
            expected,
            filter_stitle(
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
            filter_stitle(
                &hit_words,
                &FILTER_REGEXS,
                Some(&(*CAPTURE_REPLACE_DESCRIPTION_PAIRS))
            )
        );
    }

    #[test]
    fn default_matches_blacklist_regexs() {
        let t1 = "LRR receptor-like serine/threonine-protein kinase EFR";
        assert!(!matches_blacklist(t1, &BLACKLIST_STITLE_REGEXS));

        let t2 = "Probable LRR receptor-like serine/threonine-protein kinase At3g47570";
        assert!(matches_blacklist(t2, &BLACKLIST_STITLE_REGEXS));

        let t3 = "Putative receptor-like protein kinase At3g47110";
        assert!(matches_blacklist(t3, &BLACKLIST_STITLE_REGEXS));

        let t4 = "hypothetical receptor-like protein kinase At3g47110";
        assert!(matches_blacklist(t4, &BLACKLIST_STITLE_REGEXS));

        let t5 = "whole Genome shotgun Sequence";
        assert!(matches_blacklist(t5, &BLACKLIST_STITLE_REGEXS));

        let t6 = "predicted Receptor-like protein kinase";
        assert!(matches_blacklist(t6, &BLACKLIST_STITLE_REGEXS));
    }
}
