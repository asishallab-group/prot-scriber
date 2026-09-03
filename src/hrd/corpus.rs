//! The word statistics a human readable description is chosen by.
//!
//! A corpus is a bag of words with their counts. From it comes each word's inverse information
//! content, the constant those are centered at, and so the score that decides whether a word is
//! worth having in a description at all.
//!
//! It is one type because there is more than one thing a corpus can be counted over -- today the
//! hits of the one protein being annotated, and nothing else -- and the statistics must not differ
//! between them.

use crate::hrd::description::matches_any_regex;
use crate::hrd::stats::{mean, quantile};
use regex::Regex;
use std::collections::HashMap;

/// Words and how often each was seen, together with the total number of words seen.
///
/// Counts, not frequencies: counts of two corpora add, probabilities do not, and a count stays
/// true when a corpus is pruned or extended while a probability silently stops being one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Corpus {
    counts: HashMap<String, u64>,
    tokens: u64,
}

impl Corpus {
    /// Count one occurrence of `word`.
    ///
    /// Nothing here decides what is worth counting; the caller does, because what counts as a word
    /// and whether it is informative are properties of the text the corpus is made from, not of
    /// the statistics.
    pub fn observe(&mut self, word: &str) {
        match self.counts.get_mut(word) {
            Some(count) => *count += 1,
            None => {
                self.counts.insert(word.to_string(), 1);
            }
        }
        self.tokens += 1;
    }

    /// Count every informative word of one description.
    ///
    /// A word already counted has passed the non-informative expressions in a past call, so it is
    /// not tested against them again -- which matters, because that test is a scan over a list of
    /// regular expressions and this is called once per word of every description there is.
    ///
    /// # Arguments
    ///
    /// * `words` - The description, split into words.
    /// * `non_informative` - The expressions that recognise a word carrying no meaning of its own.
    ///   Such a word is not counted at all: it is worth a fixed minimum wherever it stands, so it
    ///   has no frequency to speak of.
    pub fn observe_description(&mut self, words: &[String], non_informative: &[Regex]) {
        for word in words {
            if self.knows(word) || !matches_any_regex(word, non_informative) {
                self.observe(word);
            }
        }
    }

    /// Whether `word` has been seen.
    pub fn knows(&self, word: &str) -> bool {
        self.counts.contains_key(word)
    }

    /// How often `word` was seen; zero if it never was.
    pub fn count(&self, word: &str) -> u64 {
        self.counts.get(word).copied().unwrap_or(0)
    }

    /// Whether nothing has been seen.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// The inverse information content of `word`, `-ln(1 - p)` for `p` the share of the corpus's
    /// words that this word is. `None` for a word the corpus never saw.
    ///
    /// Note that this grows with `p`: a word that is common here is worth more, which is what
    /// finding the description these hits agree on requires. It is not a measure of how
    /// informative the word is in general, which grows the other way.
    ///
    /// A corpus of a single distinct word has no distribution to speak of -- that word is all of
    /// it -- and `-ln(1 - 1)` is infinite, so it is worth a flat one.
    pub fn iic(&self, word: &str) -> Option<f64> {
        if !self.knows(word) {
            return None;
        }
        Some(if self.counts.len() > 1 {
            let p = self.count(word) as f64 / self.tokens as f64;
            -f64::log(1. - p, std::f64::consts::E)
        } else {
            1.0
        })
    }

    /// The constant the scores are centered at: the `tau`-th quantile of the inverse information
    /// contents, taken over the distinct words rather than over their occurrences, or their mean
    /// if `tau` is the literal 50.0.
    ///
    /// This constant is the whole of the selection -- a word is worth having exactly when its
    /// inverse information content is above it -- so where it is put is where the line between a
    /// word worth saying and a word not worth saying is drawn.
    ///
    /// Zero when every word was seen equally often, because then every word is worth the same and
    /// centering would leave nothing at all above the line.
    ///
    /// # Arguments
    ///
    /// * `tau` - The quantile, between zero and one, or the literal 50.0 for the mean.
    pub fn centre(&self, tau: f64) -> f64 {
        if self.counts.is_empty() || self.counts.values().min() == self.counts.values().max() {
            return 0.0;
        }
        if tau != 50.0 && !(0.0..=1.0).contains(&tau) {
            panic!(
                "\n\nCannot compute quantile {:?} because it is not a valid value between zero and one (inclusive) or a literal 50.0.\n\n",
                tau
            );
        }
        let mut iics: Vec<f64> = self
            .counts
            .keys()
            .map(|word| self.iic(word).unwrap())
            .collect();
        // SORTED BEFORE EITHER BRANCH, so this depends on the SET of values and never on the order
        // `HashMap` hands them over -- which `RandomState` seeds afresh for every map, so it is a
        // different order in every process. `quantile` would sort anyway; `mean` is Welford's
        // method and would not, and floating point addition is not associative. The resulting
        // one-ULP shift in the centre flips the exact-equality tie-break downstream, because the
        // centre IS the zero crossing for word scores.
        //
        // One sort covers the whole path: each word's inverse information content is computed on
        // its own, and the token total it divides by is a sum of integer counts, exact in f64 in
        // any order.
        iics.sort_by(f64::total_cmp);
        if tau == 50.0 {
            mean(&iics)
        } else {
            quantile(&mut iics, tau)
        }
    }

    /// What each word is worth: its inverse information content less the centre, so that a word
    /// commoner than the centre scores above zero and a rarer one below.
    ///
    /// # Arguments
    ///
    /// * `tau` - The quantile to centre at; see `centre`.
    pub fn scores(&self, tau: f64) -> HashMap<String, f64> {
        let centre = self.centre(tau);
        self.counts
            .keys()
            .map(|word| (word.clone(), self.iic(word).unwrap() - centre))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;
    use pretty_assertions::assert_eq;

    /// A corpus of the given counts, built the way a file used to be read back into one.
    ///
    /// Was `Corpus::of_counts`, and went with the corpus verb: nothing in a run ever built a corpus
    /// from counts, only from descriptions. It stays here because the test below needs a FRESH
    /// `HashMap` each time -- a clone copies the `RandomState` and every copy then iterates alike,
    /// which is the trap that once shipped a test proving nothing -- and building these by
    /// `observe` would be five million calls per corpus.
    fn of_counts<I: IntoIterator<Item = (String, u64)>>(counts: I) -> Corpus {
        let mut corpus = Corpus::default();
        for (word, count) in counts {
            corpus.tokens += count;
            *corpus.counts.entry(word).or_insert(0) += count;
        }
        corpus
    }

    fn corpus_of(words: &[&str]) -> Corpus {
        let mut corpus = Corpus::default();
        for word in words {
            corpus.observe(word);
        }
        corpus
    }

    #[test]
    fn a_corpus_counts_words_and_their_occurrences() {
        let corpus = corpus_of(&["protein", "kinase", "protein"]);
        assert_eq!(2, corpus.count("protein"));
        assert_eq!(1, corpus.count("kinase"));
        assert_eq!(0, corpus.count("receptor"));
        assert!(corpus.knows("protein"));
        assert!(!corpus.knows("receptor"));
        assert_eq!(2, corpus.counts.len());
        assert_eq!(3, corpus.tokens);

        let nothing = Corpus::default();
        assert!(nothing.is_empty());
        assert_eq!(0, nothing.tokens);
    }

    #[test]
    fn what_a_word_is_worth_grows_with_how_common_it_is() {
        let corpus = corpus_of(&["protein", "protein", "protein", "kinase"]);
        // -ln(1 - 3/4) and -ln(1 - 1/4):
        assert_abs_diff_eq!(1.386294, corpus.iic("protein").unwrap(), epsilon = 1e-6);
        assert_abs_diff_eq!(0.287682, corpus.iic("kinase").unwrap(), epsilon = 1e-6);
        assert!(corpus.iic("protein").unwrap() > corpus.iic("kinase").unwrap());
        assert_eq!(None, corpus.iic("receptor"));
    }

    #[test]
    fn a_corpus_of_one_word_gives_it_a_flat_one() {
        // Because `-ln(1 - 1)` is infinite, and there is nothing to compare the word against:
        let corpus = corpus_of(&["protein", "protein"]);
        assert_eq!(Some(1.0), corpus.iic("protein"));
    }

    #[test]
    fn words_seen_equally_often_are_not_centered() {
        // Centering them would put every one of them at zero, and the description would be empty:
        let corpus = corpus_of(&["protein", "kinase", "receptor"]);
        assert_eq!(0.0, corpus.centre(0.5));
        assert_eq!(0.0, Corpus::default().centre(0.5));
        for score in corpus.scores(0.5).values() {
            assert!(*score > 0.0);
        }
    }

    #[test]
    fn a_word_commoner_than_the_centre_scores_above_zero_and_a_rarer_one_below() {
        let corpus = corpus_of(&[
            "kinase",
            "kinase",
            "kinase",
            "kinase",
            "receptor",
            "receptor",
            "receptor",
            "protein",
            "protein",
            "phytosulfokine",
        ]);
        let scores = corpus.scores(0.5);
        assert_abs_diff_eq!(0.289909, corpus.centre(0.5), epsilon = 1e-6);
        assert_abs_diff_eq!(0.220916, scores["kinase"], epsilon = 1e-6);
        assert_abs_diff_eq!(0.066766, scores["receptor"], epsilon = 1e-6);
        assert_abs_diff_eq!(-0.066766, scores["protein"], epsilon = 1e-6);
        assert_abs_diff_eq!(-0.184549, scores["phytosulfokine"], epsilon = 1e-6);
    }

    #[test]
    fn the_centre_can_be_the_mean_instead_of_a_quantile() {
        let corpus = corpus_of(&["kinase", "kinase", "receptor", "protein"]);
        let iics: Vec<f64> = ["kinase", "receptor", "protein"]
            .iter()
            .map(|word| corpus.iic(word).unwrap())
            .collect();
        assert_abs_diff_eq!(
            iics.iter().sum::<f64>() / 3.0,
            corpus.centre(50.0),
            epsilon = 1e-12
        );
    }

    #[test]
    #[should_panic(expected = "not a valid value between zero and one")]
    fn a_quantile_outside_zero_to_one_is_a_mistake_worth_stopping_for() {
        corpus_of(&["kinase", "kinase", "receptor"]).centre(1.5);
    }

    /// The centre must depend on the SET of inverse information contents and never on the order
    /// they arrive in -- and they arrive in the order of a `HashMap`, which `RandomState` seeds
    /// afresh for every map.
    ///
    /// This is the half of the 20.08.2026 reproducibility fix that the refactor lost. It was a
    /// sort inside `word_scores_quantile`, a function this module replaced; `quantile` still sorts
    /// (in place, by necessity), but `mean` is Welford's incremental method, floating point
    /// addition is not associative, and 50.0 -- the shipped default -- is the branch that takes
    /// the mean. A one-ULP shift in the centre is not cosmetic: the centre IS the zero crossing
    /// for word scores, so a word sitting on it changes side and the description changes with it.
    /// Measured on a `corpus-dispersion` build 02.09.2026: two runs of one binary over one input
    /// disagreed on 6 of 1215 gene families.
    ///
    /// SEPARATE CORPORA, NEVER A CLONE. A clone copies the `RandomState`, so every copy iterates
    /// identically and the test passes unsorted -- exactly the trap that shipped a test proving
    /// nothing when this was first fixed. `Corpus::of_counts` builds a fresh map each time, and
    /// Rust seeds each new map from a thread-local counter, so these thirty-two really do differ.
    #[test]
    fn the_centre_does_not_depend_on_the_order_the_words_are_iterated_in() {
        // Enough words, with counts spread widely enough, that the summands differ in the low bits
        // -- two or three words of similar frequency would sum the same in any order.
        let counts: Vec<(String, u64)> = (1..=257u64)
            .map(|i| (format!("word{:03}", i), i * i + 7))
            .collect();

        let first = of_counts(counts.clone()).centre(50.0);
        for round in 1..32 {
            assert_eq!(
                first,
                of_counts(counts.clone()).centre(50.0),
                "the centre moved between two corpora holding identical counts (round {round}); \
                 it is being computed over a HashMap's iteration order"
            );
        }
    }
}
