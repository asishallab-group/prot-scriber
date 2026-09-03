//! What a run writes.
//!
//! `table` is the deliverable: one row per annotee, the human readable description it was given,
//! and the few numbers behind it. `trace` is the account of HOW that description was chosen, for
//! whoever asked to see it -- a separate stream, written as each annotee finishes and then
//! forgotten, so that what a run has to hold does not grow with its input.
//!
//! Two sinks, not one thing and its detail: a run can write either, both or neither.

pub mod table;
pub mod trace;
