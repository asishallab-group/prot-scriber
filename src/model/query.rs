//! The `Query` biological sequence a sequence similarity search was carried out for, and the
//! search's Hits for it.

use crate::hrd::{generate_human_readable_description, Annotation};
use regex::Regex;
use std::collections::HashMap;

/// A sequence similarity search is executed for a query sequence, which is represented by `Query`.
#[derive(Debug, Clone, Default)]
pub struct Query {
    /// The sequence similarity search results (Blast Hits)
    pub hits: HashMap<String, String>,
    /// A counter of how many times this query was parsed in sequence similarity search results
    pub n_parsed_from_sssr_tables: u16,
}

/// Representation of a query in a sequence similarity search (SSS), e.g. Blast or Diamond.
impl Query {
    /// Returns a new and initialized instance of struct `Query`.
    pub fn new() -> Query {
        Query {
            hits: HashMap::<String, String>::new(),
            n_parsed_from_sssr_tables: 0,
        }
    }

    /// Generates a human readable description for this biological query sequence, and returns it
    /// together with everything it was chosen from; see `Annotation`.
    ///
    /// The hits are scored in the order of their accessions rather than in the order a `HashMap`
    /// happens to yield them, so that what an `--explain` trace lists is the same list twice
    /// running. Which hit comes first does not decide anything -- a phrase that two hits propose
    /// is proposed once, and phrases of equal score are ranked alphabetically -- but a report of
    /// how a result came about is worth nothing if it is not itself reproducible.
    ///
    /// # Arguments
    ///
    /// * `&self` - A mutable reference to self, this instance of Query
    /// * `split_regex` - A reference to a regular expression used to split descriptions (`stitle`
    ///   in Blast terminology) into words.
    /// * `non_informative_words_regexs` - A reference to a vector holding regular expressions used
    ///   to identify non informative words, that receive only a minimum score.
    /// * `center_at_quantile` - A real value between zero and one used to center the inverse
    ///   information content scores.
    pub fn annotate(
        &self,
        split_regex: &Regex,
        non_informative_words_regexs: &[Regex],
        center_at_quantile: &f64,
    ) -> Annotation {
        let mut hits: Vec<(&String, &String)> = self.hits.iter().collect();
        hits.sort_unstable_by_key(|(hit_id, _)| *hit_id);
        let hit_descriptions: Vec<String> =
            hits.iter().map(|(_, description)| (*description).clone()).collect();
        let mut annotation = generate_human_readable_description(
            &hit_descriptions,
            split_regex,
            non_informative_words_regexs,
            center_at_quantile,
        );
        for (scored, (hit_id, _)) in annotation.scored.iter_mut().zip(hits) {
            scored.source = hit_id.clone();
        }
        annotation
    }
}
