//! prot-scriber's domain model: the biological sequences it annotates. A `Query` is a single such
//! sequence together with the Hits a sequence similarity search produced for it; a `SeqFamily` is
//! a set of queries that is annotated as one.

pub mod query;
pub mod seq_family;
