use crate::default::NON_INFORMATIVE_WORD_SCORE;
use crate::description::matches_blacklist;
use regex::Regex;
use crate::stats::{mean, quantile};
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
    /// Its centred inverse information content, which is what a phrase's score is a sum of.
    pub score: f64,
}

/// A phrase, i.e. a run of words taken from one description, and what it scored.
#[derive(Debug, Clone, PartialEq)]
pub struct Phrase {
    /// The words, in the order they stood in the description they were taken from.
    pub words: Vec<String>,
    /// The sum of their scores.
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
/// * `split_regex` - The regular expression used to split descriptions (parsed `stitle`) into
///   vectors of words (`String`).
/// * `non_informative_words_regexs` - A reference to a vector holding regular expressions used to
///   identify non informative words, that receive only a minimum score.
/// * `center_at_quantile` - A real value between zero and one used to center the inverse
///   information content scores.
/// * `explain` - Whether anything will read the account this returns. When nothing will, the parts
///   of it that only a reader wants -- each description as text, and which hit it came from -- are
///   left out, and what remains is what the scoring itself needs.
pub fn generate_human_readable_description(
    descriptions: &[&str],
    split_regex: &Regex,
    non_informative_words_regexs: &[Regex],
    center_at_quantile: &f64,
    explain: bool,
) -> Annotation {
    let mut annotation = Annotation {
        scored: descriptions
            .iter()
            .map(|description| Scored {
                source: String::new(),
                query: None,
                words: split_descriptions(description, split_regex),
                description: (*description).to_string(),
                phrase: None,
            })
            .collect(),
        ..Default::default()
    };
    if annotation.scored.is_empty() {
        return annotation;
    }

    // The universe of informative words, maintaining the word-frequencies:
    let mut informative_words_universe: Vec<String> = vec![];
    for scored in &annotation.scored {
        for word in &scored.words {
            // Build the word universe for later calculation of word-frequencies, but only consider
            // words that are not classified as non-informative. Note that if a word already is
            // contained in the universe, it has passed the blacklist in a past iteration, so we
            // don't need to check again:
            if informative_words_universe.contains(word)
                || !matches_blacklist(word, non_informative_words_regexs)
            {
                informative_words_universe.push(word.clone());
            }
        }
    }
    // Only continue with the process of generating a human readable description if at least a
    // single informative word has been found:
    if informative_words_universe.is_empty() {
        return annotation;
    }

    // Calculate the frequency of the informative universe words:
    let word_frequencies = frequencies(&informative_words_universe);
    let ciic: HashMap<String, f64> =
        centered_inverse_information_content(&word_frequencies, center_at_quantile);
    annotation.words = {
        let mut words: Vec<WordScore> = ciic
            .iter()
            .map(|(word, score)| WordScore {
                word: word.clone(),
                frequency: word_frequencies[word],
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

/// Calculates the word frequencies for argument `universe_words` and returns a `HashMap<String,
/// f64>` mapping the words to their respective frequency. Note that this functions returns
/// absolute frequencies in terms of number of appearances.
///
/// # Arguments
///
/// * `universe_words: &Vector<String>` - vector of words
pub fn frequencies(universe_words: &[String]) -> HashMap<String, f64> {
    let mut word_freqs: HashMap<String, f64> = HashMap::new();
    for word in universe_words.iter() {
        if !word_freqs.contains_key(word) {
            let n_appearances = universe_words.iter().filter(|x| (*x) == word).count() as f64;
            word_freqs.insert((*word).clone(), n_appearances);
        }
    }
    word_freqs
}

/// Computes the score of the informative words in argument `wrd_frequencies.keys()` using 'inverse
/// information content' calculated as `-1 * log(1 - probability(word))`, where 'probability' =
/// frequency tanges between zero and one. In order to avoid infinite values for a word that is the
/// single element of the word-set, i.e. it has a frequency of one, the score of one is used.
/// Returns a HashMap of word centered IIC key-value-pairs (`HashMap<String, f64>`). Note that
/// providing argument `center_at_quantile` as a literal 50.0 yields centering at the mean instead
/// of a quantile.
///
/// # Arguments
///
/// * `wrd_frequencies` - An instance of dictionary of all words with their frequencies.
/// * `center_at_quantile` - A real value between zero and one used to center the inverse
///   information content scores or a literal 50.0 indicating to center at the mean instead of a
///   quantile.
pub fn centered_inverse_information_content(
    wrd_frequencies: &HashMap<String, f64>,
    center_at_quantile: &f64,
) -> HashMap<String, f64> {
    // Initialize default result:
    let mut ciic_result: HashMap<String, f64> = HashMap::new();

    if !wrd_frequencies.is_empty() {
        // Calculate inverse information content (IIC):
        let sum_wrd_frequencies: f64 = wrd_frequencies.values().sum();
        let mut inv_inf_cntnt: Vec<(String, f64)> = vec![];
        for word in wrd_frequencies.keys() {
            if wrd_frequencies.len() > 1 {
                let pw = wrd_frequencies[word] / sum_wrd_frequencies;
                let iic: f64 = -f64::log(1. - pw, std::f64::consts::E);
                inv_inf_cntnt.push((word.to_string(), iic));
            } else {
                inv_inf_cntnt.push((word.to_string(), 1.0));
            }
        }

        // Center inverse information content (IIC) values, if and only if there is variation
        // between the calculated IIC values. Variation can only result from varying frequencies,
        // so find out if the argument `wrd_frequencies` contains such values:
        let mut wrd_frequency_vals_iter = wrd_frequencies.values();
        let mut current_val: &f64 = wrd_frequency_vals_iter.next().unwrap();
        let mut iic_values_all_identical = true;
        for val in wrd_frequency_vals_iter {
            if current_val != val {
                iic_values_all_identical = false;

                // Once a comparison was false, we _must not_ compare more pairs, because if the
                // last pair is in fact identical the boolean result would not be correct:
                break;
            }
            current_val = val;
        }

        // Calculate mean inverse information content for centering:
        let mut subtract_4_centering = 0.0;
        // Note that only in case of variance between IIC values, we calculate the indicated
        // quantile IIC to be subtracted from the actual IIC for centering. Otherwise the above
        // default zero will be subtracted:
        if !iic_values_all_identical {
            subtract_4_centering = word_scores_quantile(&inv_inf_cntnt, *center_at_quantile);
        }
        // Center inverse information content:
        for word_iic_tuple in inv_inf_cntnt {
            ciic_result.insert(word_iic_tuple.0, word_iic_tuple.1 - subtract_4_centering);
        }
    }

    ciic_result
}

/// Computes and returns the argument `quantile` score of an argument word-score vector `values`.
/// The method used is explained here: https://www-users.york.ac.uk/~mb55/intro/quantile.htm
///
/// # Arguments
///
/// * `values` - A reference to a word-score vector
/// * `tau` - A value between 0.0 and 1.0 indicating the quantile to calculate, or a literal 50.0
///   indicating to use the mean instead of a quantile.
pub fn word_scores_quantile(values: &[(String, f64)], tau: f64) -> f64 {
    if tau != 50.0 && !(0.0..=1.0).contains(&tau) {
        panic!(
            "\n\nCannot compute quantile {:?} because it is not a valid value between zero and one (inclusive) or a literal 50.0.\n\n",
            tau
        );
    }
    let mut scores: Vec<f64> = values.iter().map(|(_, s)| *s).collect();
    if tau == 50.0 {
        mean(&scores)
    } else {
        quantile(&mut scores, tau)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use crate::default::{
        CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE, NON_INFORMATIVE_WORDS_REGEXS,
        SPLIT_DESCRIPTION_REGEX,
    };
    use approx::assert_abs_diff_eq;
    use std::vec;

    #[test]
    fn test_word_scores_quantile() {
        let input = vec![
            ("".to_string(), 1.0),
            ("".to_string(), 2.0),
            ("".to_string(), 3.0),
            ("".to_string(), 4.0),
        ];
        assert_eq!(word_scores_quantile(&input, 0.5), 2.5);
        assert_abs_diff_eq!(word_scores_quantile(&input, 1.0 / 3.0), 1.777777, epsilon = 1e-5);
        assert_abs_diff_eq!(
            word_scores_quantile(&input, 0.4),
            2.0 + 0.1 * 2.0 / 3.0,
            epsilon = 1e-5
        );
        assert_abs_diff_eq!(
            word_scores_quantile(&input, 0.75),
            3.58 + 0.01 * 1.0 / 3.0,
            epsilon = 1e-5
        );
        assert_abs_diff_eq!(word_scores_quantile(&input, 0.8), 3.8, epsilon = 1e-5);
        assert_eq!(word_scores_quantile(&input, 1.0), 4.0);
        assert_eq!(word_scores_quantile(&input, 0.0), 1.0);
    }

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

    #[test]
    fn test_frequencies() {
        let mut words = vec![
            "alcohol".to_string(),
            "dehydrogenase".to_string(),
            "c".to_string(),
            "terminal".to_string(),
        ];
        let mut expected = HashMap::new();
        expected.insert("terminal".to_string(), 1.0);
        expected.insert("dehydrogenase".to_string(), 1.0);
        expected.insert("alcohol".to_string(), 1.0);
        expected.insert("c".to_string(), 1.0);
        assert_eq!(expected, frequencies(&words));

        words = vec![
            "importin".to_string(),
            "5".to_string(),
            "importin".to_string(),
            "5".to_string(),
            "ran".to_string(),
            "binding".to_string(),
            "6".to_string(),
            "ran".to_string(),
            "binding".to_string(),
            "6".to_string(),
            "importin".to_string(),
            "subunit".to_string(),
            "beta".to_string(),
            "3".to_string(),
            "importin".to_string(),
            "subunit".to_string(),
            "beta".to_string(),
            "3".to_string(),
        ];
        expected = HashMap::new();
        expected.insert("subunit".to_string(), 2.0);
        expected.insert("5".to_string(), 2.0);
        expected.insert("binding".to_string(), 2.0);
        expected.insert("3".to_string(), 2.0);
        expected.insert("ran".to_string(), 2.0);
        expected.insert("6".to_string(), 2.0);
        expected.insert("beta".to_string(), 2.0);
        expected.insert("importin".to_string(), 4.0);
        assert_eq!(expected, frequencies(&words));
    }

    #[test]
    fn test_centered_inverse_information_content() {
        let mut freq_map = HashMap::new();
        freq_map.insert("a".to_string(), 3_f64);
        freq_map.insert("b".to_string(), 2_f64);
        freq_map.insert("c".to_string(), 2_f64);
        freq_map.insert("d".to_string(), 1_f64);
        freq_map.insert("e".to_string(), 1_f64);
        freq_map.insert("f".to_string(), 1_f64);

        let mut freq_sum: f64 = freq_map.values().sum();

        let mut expected: HashMap<String, f64> = HashMap::new();
        expected.insert(
            "a".to_string(),
            -f64::log(1. - 3. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "b".to_string(),
            -f64::log(1. - 2. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "c".to_string(),
            -f64::log(1. - 2. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "d".to_string(),
            -f64::log(1. - 1. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "e".to_string(),
            -f64::log(1. - 1. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "f".to_string(),
            -f64::log(1. - 1. / freq_sum, std::f64::consts::E),
        );
        let iic_scores: Vec<f64> = expected.values().copied().collect();
        let mut iic_scores = iic_scores;
        let mean_ciic: f64 = quantile(&mut iic_scores, 0.5);
        // center the expected IIC:
        let mut centered_expected: HashMap<String, f64> = HashMap::new();
        for (word, iic) in &expected {
            centered_expected.insert((*word).clone(), iic - mean_ciic);
        }

        // test iteratively:
        let result: HashMap<String, f64> = centered_inverse_information_content(&freq_map, &0.5);
        for word in centered_expected.keys() {
            assert_abs_diff_eq!(
                *centered_expected.get(word).unwrap(),
                *result.get(word).unwrap(),
                epsilon = 1e-6
            );
        }

        // Test 2:
        freq_map = HashMap::new();
        freq_map.insert("alcohol".to_string(), 2.0);
        freq_map.insert("terminal".to_string(), 2.0);
        freq_map.insert("geraniol".to_string(), 2.0);
        freq_map.insert("manitol".to_string(), 3.0);
        freq_map.insert("dehydrogenase".to_string(), 7.0);
        freq_map.insert("c".to_string(), 1.0);
        freq_map.insert("cinnamyl".to_string(), 1.0);
        freq_sum = freq_map.values().sum();
        expected = HashMap::new();
        expected.insert(
            "alcohol".to_string(),
            -f64::log(1. - 2. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "terminal".to_string(),
            -f64::log(1. - 2. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "geraniol".to_string(),
            -f64::log(1. - 2. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "manitol".to_string(),
            -f64::log(1. - 3. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "dehydrogenase".to_string(),
            -f64::log(1. - 7. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "c".to_string(),
            -f64::log(1. - 1. / freq_sum, std::f64::consts::E),
        );
        expected.insert(
            "cinnamyl".to_string(),
            -f64::log(1. - 1. / freq_sum, std::f64::consts::E),
        );
        let iic_scores: Vec<f64> = expected.values().copied().collect();
        let mut iic_scores = iic_scores;
        let mean_ciic: f64 = quantile(&mut iic_scores, 0.5);
        // center the expected IIC:
        let mut centered_expected: HashMap<String, f64> = HashMap::new();
        for (word, iic) in &expected {
            centered_expected.insert((*word).clone(), iic - mean_ciic);
        }
        // test iteratively:
        let result: HashMap<String, f64> = centered_inverse_information_content(&freq_map, &0.5);
        for word in centered_expected.keys() {
            assert_abs_diff_eq!(
                *centered_expected.get(word).unwrap(),
                *result.get(word).unwrap(),
                epsilon = 1e-6
            );
        }

        // test special case of all equally frequent words:
        freq_map = HashMap::new();
        freq_map.insert("foo".to_string(), 1.0);
        freq_map.insert("bar".to_string(), 1.0);
        freq_map.insert("baz".to_string(), 1.0);
        centered_expected = HashMap::new();
        // All words should have this NON CENTERED inverse information content:
        let iic: f64 = -f64::log(1. - 1. / 3., std::f64::consts::E);
        centered_expected.insert("foo".to_string(), iic);
        centered_expected.insert("bar".to_string(), iic);
        centered_expected.insert("baz".to_string(), iic);
        assert_eq!(
            centered_expected,
            centered_inverse_information_content(&freq_map, &0.5)
        );
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

        let mut word_freqs: HashMap<String, f64> = HashMap::new();
        word_freqs.insert("6".to_string(), 2.0);
        word_freqs.insert("importin".to_string(), 5.0);
        word_freqs.insert("ran".to_string(), 2.0);
        word_freqs.insert("3".to_string(), 2.0);
        word_freqs.insert("subunit".to_string(), 2.0);
        word_freqs.insert("beta".to_string(), 2.0);
        word_freqs.insert("5".to_string(), 3.0);
        word_freqs.insert("binding".to_string(), 2.0);

        let mut ciic = centered_inverse_information_content(&word_freqs, &0.5);

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

        word_freqs = HashMap::new();
        for word in &desc4 {
            word_freqs.insert(word.clone(), 1.0);
        }
        ciic = centered_inverse_information_content(&word_freqs, &0.5);
        let phrase4 = highest_scoring_phrase(&desc4, &ciic).unwrap();
        // Expect the full input description to be replicated:
        assert_eq!(desc4, phrase4.0);

        let desc5 = vec![
            "receptor".to_string(),
            "protein".to_string(),
            "eix2".to_string(),
        ];
        word_freqs = HashMap::new();
        word_freqs.insert("receptor".to_string(), 2.0);
        word_freqs.insert("eix1".to_string(), 1.0);
        word_freqs.insert("eix2".to_string(), 1.0);
        ciic = centered_inverse_information_content(&word_freqs, &0.5);
        let phrase5 = highest_scoring_phrase(&desc5, &ciic).unwrap();
        assert_eq!(
            vec!["receptor".to_string(), "protein".to_string()],
            phrase5.0
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
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
            false,
        );
        assert!(
            unread.scored.iter().all(|scored| scored.description.is_empty()),
            "the descriptions were copied for an annotation nobody asked to see"
        );

        let explained = generate_human_readable_description(
            &borrowed,
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
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
        let hit_hrds = vec![
            "importin-5".to_string(),
            "importin-5".to_string(),
            "importin-5".to_string(),
            "ran-binding protein 6".to_string(),
            "ran-binding protein 6".to_string(),
            "importin subunit beta-3".to_string(),
            "importin subunit beta-3".to_string(),
        ];
        let annotation = generate_human_readable_description(
            &hit_hrds.iter().map(String::as_str).collect::<Vec<&str>>(),
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
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
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
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
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &(CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE),
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
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &(CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE),
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
            &SPLIT_DESCRIPTION_REGEX,
            &NON_INFORMATIVE_WORDS_REGEXS,
            &(CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE),
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
