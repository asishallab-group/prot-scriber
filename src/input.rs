//! Reading and parsing of everything prot-scriber is given as input: the tabular sequence
//! similarity search results, the optional gene family file, and the optional regular expression
//! files that override the compiled in defaults.

pub mod regex_files;
pub mod seq_families;
pub mod seq_sim_table;
