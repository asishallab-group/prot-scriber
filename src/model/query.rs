//! The `Query` biological sequence a sequence similarity search was carried out for, and the
//! search's Hits for it.

use crate::hrd::{generate_human_readable_description, Annotation, Scoring};
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
    /// * `scoring` - How words are scored; settled once for the whole run.
    /// * `explain` - Whether anything will read the account of this annotation. Only an
    ///   `--explain` or `--format jsonl` run will, and what it costs is a copy of every hit
    ///   description plus the accession it came from.
    pub fn annotate(&self, scoring: &Scoring, explain: bool) -> Annotation {
        let mut hits: Vec<(&String, &String)> = self.hits.iter().collect();
        hits.sort_unstable_by_key(|(hit_id, _)| *hit_id);
        let hit_descriptions: Vec<&str> =
            hits.iter().map(|(_, description)| description.as_str()).collect();
        let mut annotation =
            generate_human_readable_description(&hit_descriptions, scoring, explain);
        if explain {
            for (scored, (hit_id, _)) in annotation.scored.iter_mut().zip(hits) {
                scored.source = hit_id.clone();
            }
        }
        annotation
    }
}
