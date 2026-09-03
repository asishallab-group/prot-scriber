//! How a human readable description is made from the descriptions of an annotee's hits.
//!
//! The pipeline in order: `description` applies the rule lists to each raw sequence title,
//! `split_descriptions` below cuts what survives into words, this module scores those words by
//! how many of the annotee's hits agree on them, and `highest_scoring_phrase` picks the run of
//! words that says most. `corpus` is the per-annotee word statistic the scoring reads, and
//! `stats` the two order statistics it is centred on.

pub mod corpus;
pub mod description;
pub mod stats;

use crate::default::NON_INFORMATIVE_WORD_SCORE;
use crate::hrd::corpus::Corpus;
use regex::Regex;
use std::cmp::Ordering;
use std::collections::HashMap;

/// One word of the universe the scoring was carried out over, and what it was worth.
///
/// Only informative words appear here. A word that matched one of the non-informative
/// expressions is not scored at all -- it is worth `NON_INFORMATIVE_WORD_SCORE` wherever it
/// stands -- so its absence from this list is what says it was found non-informative.
#[derive(Debug, Clone, PartialEq)]
pub struct WordScore {
    /// The word.
    pub word: String,
    /// How often it appeared, counted over every description that was scored.
    pub frequency: f64,
    /// What it was worth. Positive means the word is commoner among these hits than the quantile
    /// the scores were centred at, and so is worth having in the description; negative means
    /// rarer.
    pub score: f64,
}

/// Everything the choice of a description depends on apart from the hits themselves.
///
/// Gathered into one value because it is settled once for a whole run and then read by every
/// annotation in it, and because the list had reached the length at which the compiler stops
/// telling one `&Regex` from another.
pub struct Scoring<'a> {
    /// Splits a description into words.
    pub split_regex: &'a Regex,
    /// Recognise a word that carries no meaning of its own, which is scored as such rather than
    /// counted.
    pub non_informative_words_regexs: &'a [Regex],
    /// The quantile of the word scores that the line between worth saying and not worth saying is
    /// drawn at, or the literal 50.0 for their mean.
    pub center_at_quantile: f64,
}

/// A phrase, i.e. a run of words taken from one description, and what it scored.
#[derive(Debug, Clone, PartialEq)]
pub struct Phrase {
    /// The words, in the order they stood in the description they were taken from.
    pub words: Vec<String>,
    /// What the phrase was worth, which is what it was ranked against the other phrases by.
    ///
    /// This is *not* in general the sum of the scores of `words`: a word scoring below the
    /// centre is sometimes carried along by the word after it without its score being counted.
    /// See `a_phrase_can_keep_a_word_whose_score_it_leaves_out`, which is where that is
    /// characterised, and why it is left alone.
    pub score: f64,
}

impl Phrase {
    /// The phrase as it would be written out, which is also how two phrases are compared when
    /// they score the same.
    pub fn text(&self) -> String {
        self.words.join(" ")
    }
}

/// One description as it entered the scoring, and what it proposed.
#[derive(Debug, Clone, PartialEq)]
pub struct Scored {
    /// The accession of the hit whose title this description was made from.
    pub source: String,
    /// The query that hit was found for. Only set when a whole family is being annotated, its
    /// descriptions coming from more than one query.
    pub query: Option<String>,
    /// The description as it was scored, i.e. after the blacklist, the filter expressions and the
    /// capture-replace pairs of the table it was read from have been applied to the `stitle`.
    /// Empty unless the annotation was asked to explain itself; see the `explain` argument of
    /// `generate_human_readable_description`.
    pub description: String,
    /// The words it was split into.
    pub words: Vec<String>,
    /// The highest scoring phrase it yielded, if any. `None` when it consists of non-informative
    /// words alone.
    pub phrase: Option<Phrase>,
}

/// Everything the generation of one human readable description consisted of: the descriptions it
/// chose between, the words they are made of and what each was worth, every phrase that was
/// proposed, and which of them won.
///
/// It exists because all of it was being computed and then thrown away, leaving a run unable to
/// answer the one question anyone asks of it -- why this description and not another one. What is
/// returned costs nothing beyond keeping what was built anyway, and is dropped as soon as the
/// annotee it belongs to has been written out; see `AnnotationProcess::conclude`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Annotation {
    /// The chosen description, before polishing. `None` when there was nothing to choose from.
    pub description: Option<String>,
    /// Its score.
    pub score: f64,
    /// The descriptions that were scored, in the order they were scored in.
    pub scored: Vec<Scored>,
    /// The informative words and their scores, highest first.
    pub words: Vec<WordScore>,
    /// The distinct phrases that were proposed, best first, so `candidates[0]` is the one that
    /// won and `candidates[1]` is what it beat.
    pub candidates: Vec<Phrase>,
}

impl Annotation {
    /// Why there is no description, in the words of the stage that had nothing to hand on. `None`
    /// when there is one.
    pub fn verdict(&self) -> Option<&'static str> {
        if self.description.is_some() {
            None
        } else if self.scored.is_empty() {
            Some("no hit of significant similarity was found")
        } else if self.words.is_empty() {
            Some("every word of every hit description is non-informative")
        } else {
            Some("no hit description yielded a phrase of informative words")
        }
    }
}

/// Main function for generating human-readable descriptions (hrds).
///
/// Returns everything the choice was made of, not only the choice; see `Annotation`.
///
/// # Arguments
///
/// * `descriptions` - The Hit descriptions to choose a human readable description from. Borrowed:
///   they belong to the queries, and an annotation that copied them cost twice what it had to --
///   invisible for one query, several MiB for a family of fifty thousand hit descriptions.
/// * `scoring` - How words are scored; settled once for the whole run.
/// * `explain` - Whether anything will read the account this returns. When nothing will, the parts
///   of it that only a reader wants -- each description as text, and which hit it came from -- are
///   left out, and what remains is what the scoring itself needs.
pub fn generate_human_readable_description(
    descriptions: &[&str],
    scoring: &Scoring,
    explain: bool,
) -> Annotation {
    let mut annotation = Annotation {
        scored: descriptions
            .iter()
            .map(|description| Scored {
                source: String::new(),
                query: None,
                words: split_descriptions(description, scoring.split_regex),
                description: if explain {
                    (*description).to_string()
                } else {
                    String::new()
                },
                phrase: None,
            })
            .collect(),
        ..Default::default()
    };
    if annotation.scored.is_empty() {
        return annotation;
    }

    // The corpus the words are scored against, which is this annotee's own hit descriptions and
    // nothing else. Only informative words are counted; a word already counted has passed the
    // blacklist in a past iteration, so it is not tested again:
    let mut corpus = Corpus::default();
    for scored in &annotation.scored {
        corpus.observe_description(&scored.words, scoring.non_informative_words_regexs);
    }
    // Only continue with the process of generating a human readable description if at least a
    // single informative word has been found:
    if corpus.is_empty() {
        return annotation;
    }

    // How far above what these hits mostly say each word is, which is the whole of the selection.
    let ciic: HashMap<String, f64> = corpus.scores(scoring.center_at_quantile);
    annotation.words = {
        let mut words: Vec<WordScore> = ciic
            .iter()
            .map(|(word, score)| WordScore {
                word: word.clone(),
                frequency: corpus.count(word) as f64,
                score: *score,
            })
            .collect();
        words.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.word.cmp(&b.word))
        });
        words
    };

    // Find the highest scoring phrase each description has to offer:
    for scored in annotation.scored.iter_mut() {
        scored.phrase =
            highest_scoring_phrase(&scored.words, &ciic).map(|(words, score)| Phrase { words, score });
    }

    // The distinct phrases, ranked. Two descriptions proposing the same phrase propose it once,
    // and the ranking is what used to be a scan for the maximum: the highest score wins, and
    // between phrases that score the same the one that comes first alphabetically does, so that
    // the result does not depend on the order the hits happened to be read in.
    let mut ranked: Vec<(String, Phrase)> = vec![];
    for scored in &annotation.scored {
        if let Some(phrase) = &scored.phrase {
            if !ranked.iter().any(|(_, known)| known == phrase) {
                ranked.push((phrase.text(), phrase.clone()));
            }
        }
    }
    ranked.sort_by(|(a_text, a), (b_text, b)| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a_text.cmp(b_text))
    });
    if let Some((text, phrase)) = ranked.first() {
        annotation.description = Some(text.clone());
        annotation.score = phrase.score;
    }
    annotation.candidates = ranked.into_iter().map(|(_, phrase)| phrase).collect();

    annotation
}

/// Find the highest scoring "phrase" in argument `description`. A phrase is a subset of the
/// argument vector maintaining the order of elements. The highest scoring phrase is found using
/// the linear solution to the longest, or highest scoring, path problem in directed acyclic
/// graphs. The argument `description` is converted into a graph, in which each word has edges to
/// all words appearing after it in the `description`. An `Option<(Vec<String>, f64)>` is returned
/// holding the highest scoring phrase and that phrase's score.
///
/// Because the graph is complete and an edge is labelled with the score of the word it leads to
/// alone, the highest scoring path is simply the words scoring above zero -- which is to say that
/// the constant the scores were centred at is the whole of the selection. The one exception is a
/// word below the centre that a tie in the path scores carries along without its score; see
/// `a_phrase_can_keep_a_word_whose_score_it_leaves_out`.
///
/// # Arguments
///
/// * `description` - A vector of words representing the description for which to find the best
///   scoring phrase.
/// * `ciic` - A reference to a HashMap holding the centered inverse information content scores for
///   the informative words appearing in the argument `description`.
pub fn highest_scoring_phrase(
    description: &[String],
    ciic: &HashMap<String, f64>,
) -> Option<(Vec<String>, f64)> {
    // Initialize the default result:
    let mut result: Option<(Vec<String>, f64)> = None;
    // There's only work to do, if the argument `description` _has_ words:
    if !description.is_empty() {
        // Each word in argument `description` is a vertex in a directed acyclic graph (DAG). An
        // additional start vertex (index 0) is added that has edges to all words:
        let n_vertices = description.len() + 1;
        // Initialize backtracing for dynamic programming; that is the highest scoring path through the
        // word DAG:
        let mut path_predecessors: Vec<usize> = vec![0; n_vertices];
        // The score of the highest scoring path to each vertex _i_ is stored in this vector:
        let mut vertex_path_scores: Vec<f64> = vec![0.0; n_vertices];
        for vertex_indx in 0..n_vertices {
            let v_edges_to_descendants: Vec<usize> = if vertex_indx == n_vertices - 1 {
                // Last word in argument `description`
                vec![]
            } else {
                // Any word i in argument `description` has edges to all words k>i following it in
                // `description`:
                ((vertex_indx + 1)..n_vertices).collect()
            };
            for desc_vertex_indx in v_edges_to_descendants {
                // The word matching the vertex is index minus one, because we inserted a start vertex
                // at index zero:
                let desc_vertex = &description[desc_vertex_indx - 1];
                // Label edges with the score of the word (vertex) the respective edge leads to. If it
                // is an informative word, lookup its score, otherwise use the minimum default score
                // for non-informative words:
                let edge_label: f64 = if ciic.contains_key(desc_vertex) {
                    *ciic.get(desc_vertex).unwrap()
                } else {
                    NON_INFORMATIVE_WORD_SCORE
                };
                // Set the score of the path to the currently processed vertex (word):
                if vertex_path_scores[desc_vertex_indx]
                    <= vertex_path_scores[vertex_indx] + edge_label
                {
                    vertex_path_scores[desc_vertex_indx] =
                        vertex_path_scores[vertex_indx] + edge_label;
                    path_predecessors[desc_vertex_indx] = vertex_indx;
                }
            }
        }
        // Find the path that yielded the highest score:
        let mut max_path_score_indx: usize = 0;
        for i in 1..vertex_path_scores.len() {
            if vertex_path_scores[i] > vertex_path_scores[max_path_score_indx] {
                max_path_score_indx = i;
            }
        }
        if max_path_score_indx > 0 {
            // Backtrace using dynamic programming the path with the highest score:
            let mut high_score_path: Vec<String> = vec![];
            let mut next_pred_indx: usize = max_path_score_indx;
            loop {
                // Get the word matching the vertex index `next_pred_indx` by subtracting one from it. This
                // needs to be done, because we inserted a start vertex with index zero:
                high_score_path.push(description[next_pred_indx - 1].clone());
                next_pred_indx = path_predecessors[next_pred_indx];
                if next_pred_indx == 0 {
                    break;
                }
            }
            // Highest scoring phrase and it's score:
            result = Some((
                high_score_path.into_iter().rev().collect(),
                vertex_path_scores[max_path_score_indx],
            ));
        }
    }
    result
}

/// Given filtered Hit descriptions it splits each word and returns a vector.
///
/// # Arguments
///
/// * `description` - A reference to the parsed `stitle` to be split into words
/// * `split_regex` - A reference to the regular expression to be used to split the argument
///   `description` into words.
pub fn split_descriptions(description: &str, split_regex: &Regex) -> Vec<String> {
    // Split the description using a simple regular expression:
    split_regex
        .split(description.trim())
        .map(|wrd| wrd.to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use pretty_assertions::assert_eq;
    use crate::default::{
        CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
        NON_INFORMATIVE_WORDS_REGEXS, SPLIT_DESCRIPTION_REGEX,
    };
    use std::vec;


    #[test]
    fn test_split_descriptions() {
        // Test 1:
        let hit_words = "alcohol dehydrogenase c terminal".to_string();
        let expected = vec!["alcohol", "dehydrogenase", "c", "terminal"];
        assert_eq!(
            expected,
            split_descriptions(&hit_words, &SPLIT_DESCRIPTION_REGEX)
        );
    }

    /// Punctuation that cannot be part of a word must separate words.
    ///
    /// The class had the punctuation of prose in it -- brackets, braces, angle brackets and the
    /// arithmetic signs were missing -- so a word that touched one carried it. Counted over the
    /// whole of Swiss-Prot that is 17,053 tokens in ~180 types, and it does not merely add junk:
    /// it SPLITS the vocabulary, because '[nad' is then a different word from 'nad' and the two
    /// never reinforce each other's count. 'superoxide dismutase [Cu-Zn]' is the shape it comes
    /// from, and the bracket reached the finished description.
    #[test]
    fn punctuation_that_cannot_be_part_of_a_word_separates_words() {
        let cases: Vec<(&str, Vec<&str>)> = vec![
            // Square brackets: an enzyme's cofactor or reaction sense, the commonest by far.
            ("superoxide dismutase [Cu-Zn]", vec!["superoxide", "dismutase", "Cu", "Zn"]),
            (
                "glutamate--ammonia ligase [ADP-forming]",
                vec!["glutamate", "ammonia", "ligase", "ADP", "forming"],
            ),
            // The plus of an oxidised cofactor, which stood as a word of its own 4,603 times.
            (
                "alcohol dehydrogenase [NAD(P)+]",
                vec!["alcohol", "dehydrogenase", "NAD", "P"],
            ),
            // Braces, angle brackets, the star and the caret: rare, and the same mistake.
            ("evidence {ECO} bearing", vec!["evidence", "ECO", "bearing"]),
            ("protein <du> fragment", vec!["protein", "du", "fragment"]),
            ("antigen E8^E2C protein", vec!["antigen", "E8", "E2C", "protein"]),
            ("kinase* domain", vec!["kinase", "domain"]),
        ];
        for (description, expected) in cases {
            assert_eq!(
                expected,
                split_descriptions(description, &SPLIT_DESCRIPTION_REGEX),
                "splitting {:?}",
                description
            );
        }
    }



    /// Scoring as a run with no arguments beyond its input does it: prot-scriber's own rules.
    fn default_scoring() -> Scoring<'static> {
        Scoring {
            split_regex: &SPLIT_DESCRIPTION_REGEX,
            non_informative_words_regexs: &NON_INFORMATIVE_WORDS_REGEXS,
            center_at_quantile: CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
        }
    }


    /// A non-informative word stays in the description, and that is the point of it.
    ///
    /// Marking a word non-informative does not remove it from the output. It removes it from the
    /// frequency counting, and the word is then worth `NON_INFORMATIVE_WORD_SCORE` wherever it
    /// stands -- a *positive* number, so it always joins the phrase it is in. Measured on the
    /// gene-family benchmark, 25.08.2026: putting `domain` on the non-informative list took the
    /// number of InterPro family descriptions containing `domain` from 141 to 156. It went up.
    ///
    /// That reads like a defect and is not one. What prot-scriber assigns is a *human readable*
    /// description, and `protein`, `and`, `of`, `family` carry no information while carrying the
    /// readability: without them `hva protein` is `hva` and `ran binding protein` is `ran
    /// binding`, which are not descriptions. A small positive score says exactly "let this word
    /// decide nothing, and break no sentence". The same benchmark found that not one of 1215
    /// family descriptions consisted of non-informative words alone -- they are a suffix on the
    /// answer, never the answer.
    ///
    /// So this test exists to stop the sign being "fixed". Scoring such a word zero would not even
    /// do anything, which is the second thing it pins: the path-score update is `<=` over a vector
    /// starting at zero, so a word costing nothing is taken along at no cost, by the same
    /// tie-break as `a_phrase_can_keep_a_word_whose_score_it_leaves_out`. Only a strictly negative
    /// score drops one, and dropping them is not wanted.
    #[test]
    fn a_non_informative_word_is_kept_out_only_by_a_negative_score() {
        let ciic = four_words_two_of_them_below_center();
        let description = vec![
            "receptor".to_string(),
            "like".to_string(),
            "kinase".to_string(),
        ];
        let phrase = |scores: &HashMap<String, f64>| highest_scoring_phrase(&description, scores).unwrap().0;

        // Absent from the scores, so worth NON_INFORMATIVE_WORD_SCORE, which is positive:
        assert_eq!(description, phrase(&ciic));

        // Worth exactly nothing: still taken along, because `<=` gives the tie to the longer path.
        let mut at_zero = ciic.clone();
        at_zero.insert("like".to_string(), 0.0);
        assert_eq!(description, phrase(&at_zero));

        // Worth less than nothing: now, and only now, it is left out.
        let mut negative = ciic.clone();
        negative.insert("like".to_string(), -0.001);
        assert_eq!(
            vec!["receptor".to_string(), "kinase".to_string()],
            phrase(&negative)
        );
    }


    /// The word scores of a corpus in which each word was seen the given number of times.
    fn scores_of_counts(counts: &[(&str, u64)]) -> HashMap<String, f64> {
        let mut corpus = Corpus::default();
        for (word, count) in counts {
            for _ in 0..*count {
                corpus.observe(word);
            }
        }
        corpus.scores(0.5)
    }

    #[test]
    fn test_highest_scoring_phrase() {
        let desc1: Vec<String> = vec!["importin".to_string(), "5".to_string()];
        let desc2: Vec<String> = vec![
            "ran".to_string(),
            "binding".to_string(),
            "protein".to_string(),
            "6".to_string(),
        ];
        let desc3: Vec<String> = vec!["ran".to_string(), "6".to_string()];
        let desc4: Vec<String> = vec![
            "protein".to_string(),
            "narrow".to_string(),
            "leaf".to_string(),
            "1".to_string(),
        ];

        let mut ciic = scores_of_counts(&[
            ("6", 2),
            ("importin", 5),
            ("ran", 2),
            ("3", 2),
            ("subunit", 2),
            ("beta", 2),
            ("5", 3),
            ("binding", 2),
        ]);

        let phrase1 = highest_scoring_phrase(&desc1, &ciic).unwrap();
        let expected1 = vec!["importin".to_string(), "5".to_string()];
        assert_eq!(expected1, phrase1.0);

        let phrase2 = highest_scoring_phrase(&desc2, &ciic).unwrap();
        let expected2 = vec![
            "ran".to_string(),
            "binding".to_string(),
            "protein".to_string(),
        ];
        assert_eq!(expected2, phrase2.0);

        let phrase3 = highest_scoring_phrase(&desc3, &ciic);
        assert!(phrase3.is_none());

        ciic = scores_of_counts(&[("protein", 1), ("narrow", 1), ("leaf", 1), ("1", 1)]);
        let phrase4 = highest_scoring_phrase(&desc4, &ciic).unwrap();
        // Expect the full input description to be replicated:
        assert_eq!(desc4, phrase4.0);

        let desc5 = vec![
            "receptor".to_string(),
            "protein".to_string(),
            "eix2".to_string(),
        ];
        ciic = scores_of_counts(&[("receptor", 2), ("eix1", 1), ("eix2", 1)]);
        let phrase5 = highest_scoring_phrase(&desc5, &ciic).unwrap();
        assert_eq!(
            vec!["receptor".to_string(), "protein".to_string()],
            phrase5.0
        );
    }

    /// The word scores that make the case below. Four words of decreasing frequency, centered at
    /// the median, so that two of them are worth more than the center and two less:
    ///
    ///     kinase          4/10   +0.2209
    ///     receptor        3/10   +0.0668
    ///     protein         2/10   -0.0668
    ///     phytosulfokine  1/10   -0.1845
    fn four_words_two_of_them_below_center() -> HashMap<String, f64> {
        scores_of_counts(&[
            ("kinase", 4),
            ("receptor", 3),
            ("protein", 2),
            ("phytosulfokine", 1),
        ])
    }

    /// A phrase can contain a word whose score was not counted towards the phrase's own, so
    /// `Phrase.score` is not the sum of the scores of `Phrase.words`.
    ///
    /// This is an artefact of `<=` in the path-score update over a `vertex_path_scores` that
    /// starts at zero. A word scoring below the center never raises the score of the path
    /// reaching it, so that path keeps the vector's initial zero; the next word above the center
    /// then finds the path through the below-center word to be worth exactly as much as the path
    /// straight from the start vertex, and `<=` makes the tie go to the longer one. The word is
    /// taken along, its negative score is not.
    ///
    /// It is characterised here, not fixed, because it is load-bearing: below the center is where
    /// the rare, specific words are -- `cysteine`, `phytosulfokine` -- and this accident is the
    /// only mechanism in the program that ever admits one. Both obvious repairs make output worse.
    /// A word score that carried specificity of its own would make it unnecessary; until there is
    /// one, this test says what the program does so that a change to it cannot be silent.
    #[test]
    fn a_phrase_can_keep_a_word_whose_score_it_leaves_out() {
        let ciic = four_words_two_of_them_below_center();
        assert!(
            ciic["phytosulfokine"] < 0.0,
            "the premise of this test is that this word scores below the center"
        );
        assert!(ciic["receptor"] > 0.0);

        let description = vec!["phytosulfokine".to_string(), "receptor".to_string()];
        let (words, score) = highest_scoring_phrase(&description, &ciic).unwrap();

        // The below-center word is part of the phrase:
        assert_eq!(description, words);
        // ... but not of its score, which is the other word's alone:
        assert_eq!(ciic["receptor"], score);
        // ... so the two differ by exactly what the word it kept was worth:
        let sum_of_its_words: f64 = words.iter().map(|word| ciic[word]).sum();
        assert_eq!(-ciic["phytosulfokine"], score - sum_of_its_words);
    }

    /// What `NON_INFORMATIVE_WORD_SCORE` is worth relative to the scores it stands among, because
    /// it is a constant and they are not: they are centered inverse information contents, and
    /// anything that moves the score scale -- a different centering, a factor for specificity --
    /// moves it out from under this constant.
    ///
    /// Three things hold today and none is stated anywhere else. It is positive, so a
    /// non-informative word standing between two informative ones is always taken along rather
    /// than breaking the phrase. It is orders of magnitude below the scores of words that mean
    /// something, so it cannot outweigh one of them. **But it does decide between two phrases that
    /// are otherwise equal, and the phrase carrying the extra word always wins** -- appending a
    /// non-informative word strictly increases a phrase's score, so given the choice prot-scriber
    /// takes the longer one.
    ///
    /// That last is not a corner case. Measured on the gene-family benchmark, 25.08.2026, family
    /// IPR014404, whose hits include `Aga2p KLMA_20055` -- a protein name and a yeast systematic
    /// locus tag, which the splitting expression breaks at the underscore into `klma` and `20055`:
    ///
    ///     0.453011310186  aga2p 20055   <- chosen
    ///     0.453010310186  aga2p
    ///     difference: 1.000e-06
    ///
    /// The reference calls that family "A-agglutinin-binding subunit Aga2". `aga2p` was there to
    /// be chosen and lost by exactly this constant.
    #[test]
    fn a_non_informative_word_joins_a_phrase_and_breaks_a_tie_towards_the_longer_one() {
        let ciic = four_words_two_of_them_below_center();
        // "like" is not in the universe of informative words, which is how a non-informative word
        // reaches `highest_scoring_phrase`:
        let description = vec![
            "receptor".to_string(),
            "like".to_string(),
            "kinase".to_string(),
        ];
        let (words, score) = highest_scoring_phrase(&description, &ciic).unwrap();

        // It joins the phrase instead of cutting it in two, and adds to what the phrase is worth,
        // which is to say it is positive:
        assert_eq!(description, words);
        assert_eq!(
            ciic["receptor"] + NON_INFORMATIVE_WORD_SCORE + ciic["kinase"],
            score
        );
        assert!(score > ciic["receptor"] + ciic["kinase"]);

        // Four orders of magnitude below the smallest score of a word that carries meaning, so no
        // count of non-informative words makes up an informative word's worth of difference:
        let smallest_informative = ciic
            .values()
            .map(|score| score.abs())
            .fold(f64::INFINITY, f64::min);
        assert!(
            NON_INFORMATIVE_WORD_SCORE * 10_000.0 < smallest_informative,
            "{} is not far enough below {}",
            NON_INFORMATIVE_WORD_SCORE,
            smallest_informative
        );

        // ... and yet it decides, whenever it is the only thing between two phrases. A phrase and
        // the same phrase with a non-informative word appended differ by exactly this constant,
        // and the longer one wins -- which is how `aga2p` became `aga2p 20055`.
        let with_a_trailing_number = vec![
            "receptor".to_string(),
            "kinase".to_string(),
            "20055".to_string(),
        ];
        let (words, longer) = highest_scoring_phrase(&with_a_trailing_number, &ciic).unwrap();
        assert_eq!(with_a_trailing_number, words);
        let shorter = highest_scoring_phrase(&with_a_trailing_number[..2], &ciic)
            .unwrap()
            .1;
        // To within the float error of two path sums, which is where the last digit of
        // `9.999999999732445e-7` comes from:
        assert_abs_diff_eq!(
            NON_INFORMATIVE_WORD_SCORE,
            longer - shorter,
            epsilon = 1e-15
        );
    }

    /// An account of an annotation costs what it holds, and a family holds tens of thousands of hit
    /// descriptions. So the parts of it that only a reader wants are made only when there is a
    /// reader: measured on four families of 50,000 descriptions, copying them regardless cost
    /// 1,260 bytes per description in flight against 1,144, i.e. 70.9 MiB against 65.1.
    #[test]
    fn an_annotation_nobody_will_read_does_not_copy_the_descriptions() {
        let hit_hrds = vec![
            "importin-5".to_string(),
            "ran-binding protein 6".to_string(),
        ];
        let borrowed: Vec<&str> = hit_hrds.iter().map(String::as_str).collect();

        let unread = generate_human_readable_description(
            &borrowed,
            &default_scoring(),
            false,
        );
        assert!(
            unread.scored.iter().all(|scored| scored.description.is_empty()),
            "the descriptions were copied for an annotation nobody asked to see"
        );

        let explained = generate_human_readable_description(
            &borrowed,
            &default_scoring(),
            true,
        );
        assert_eq!(
            hit_hrds,
            explained
                .scored
                .iter()
                .map(|scored| scored.description.clone())
                .collect::<Vec<String>>(),
            "an annotation that will be read must still carry what it was made of"
        );

        // Either way the choice itself is the same: what is dropped is only the evidence.
        assert_eq!(unread.description, explained.description);
        assert_eq!(unread.candidates, explained.candidates);
        assert_eq!(unread.words, explained.words);
    }

    /// What the choice was made of used to be computed and dropped on the floor; a run could say
    /// what it had decided and nothing about why. Everything below was already in memory the
    /// moment the description was chosen.
    #[test]
    fn an_annotation_carries_what_it_was_chosen_from() {
        let hit_hrds = [
            "importin-5",
            "importin-5",
            "importin-5",
            "ran-binding protein 6",
            "ran-binding protein 6",
            "importin subunit beta-3",
            "importin subunit beta-3",
        ];
        let annotation = generate_human_readable_description(
            &hit_hrds,
            &default_scoring(),
            true,
        );

        // Every description that was scored is accounted for, split into the words it was scored
        // as:
        assert_eq!(hit_hrds.len(), annotation.scored.len());
        assert_eq!(
            vec!["importin".to_string(), "5".to_string()],
            annotation.scored[0].words
        );

        // The winner is the best candidate, and the candidates are ranked, so the runner-up is
        // there to be compared against it:
        assert_eq!(Some("importin 3".to_string()), annotation.description);
        assert_eq!(annotation.description, annotation.candidates[0].text().into());
        assert_eq!(annotation.score, annotation.candidates[0].score);
        assert!(annotation.candidates.len() > 1);
        assert!(
            annotation
                .candidates
                .windows(2)
                .all(|pair| pair[0].score >= pair[1].score),
            "the candidates are not ranked: {:?}",
            annotation.candidates
        );

        // And every word that carried a score is there with the score and the count it carried:
        let importin = annotation
            .words
            .iter()
            .find(|scored| scored.word == "importin")
            .expect("'importin' is an informative word of the descriptions above");
        assert_eq!(5.0, importin.frequency);
        assert!(annotation.words.iter().all(|word| word.word != "protein"));
    }

    #[test]
    fn test_generate_human_readable_description() {
        // Test 1:
        // If one of the following 'manitol dehydrogenase' is removed the phrases 'manitol
        // dehydrogenase' *and* 'geraniol dehydrogenase' will receive identical scores. In such
        // cases the phrases are sorted using https://doc.rust-lang.org/std/vec/struct.Vec.html#method.sort
        // and the first one is selected
        let mut hit_hrds = vec![
            "manitol dehydrogenase".to_string(),
            "cinnamyl alcohol-dehydrogenase".to_string(),
            "geraniol dehydrogenase".to_string(),
            "geraniol|dehydrogenase terminal".to_string(),
            "manitol dehydrogenase".to_string(),
            "manitol dehydrogenase".to_string(),
            "alcohol dehydrogenase c-terminal".to_string(),
        ];
        let mut expected = "manitol dehydrogenase".to_string();
        let mut result = generate_human_readable_description(
            &hit_hrds.iter().map(String::as_str).collect::<Vec<&str>>(),
            &default_scoring(),
            true,
        )
        .description
        .unwrap();
        assert_eq!(expected, result);

        // Test 2:
        hit_hrds = vec![
            "importin-5".to_string(),
            "importin-5".to_string(),
            "importin-5".to_string(),
            "ran-binding protein 6".to_string(),
            "ran-binding protein 6".to_string(),
            "importin subunit beta-3".to_string(),
            "importin subunit beta-3".to_string(),
        ];
        expected = "importin 3".to_string();
        result = generate_human_readable_description(
            &hit_hrds.iter().map(String::as_str).collect::<Vec<&str>>(),
            &default_scoring(),
            true,
        )
        .description
        .unwrap();
        assert_eq!(expected, result);

        // Test 3:
        hit_hrds = vec![
            "receptor protein eix1".to_string(),
            "receptor protein eix2".to_string(),
        ];
        expected = "receptor protein".to_string();
        result = generate_human_readable_description(
            &hit_hrds.iter().map(String::as_str).collect::<Vec<&str>>(),
            &default_scoring(),
            true,
        )
        .description
        .unwrap();
        assert_eq!(expected, result);

        // Test 4:
        hit_hrds = vec![
            "member and protein or gene".to_string(),
            "gene and member or protein".to_string(),
            "member or protein".to_string(),
        ];
        let result_option = generate_human_readable_description(
            &hit_hrds.iter().map(String::as_str).collect::<Vec<&str>>(),
            &default_scoring(),
            true,
        );
        assert_eq!(None, result_option.description);
        // Every word of every description is non-informative, so there is no universe to score
        // over and nothing was proposed:
        assert!(result_option.words.is_empty());
        assert!(result_option.candidates.is_empty());
        assert_eq!(3, result_option.scored.len());
    }
}
