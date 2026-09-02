use crate::cli::{Args, NamedValue};
use crate::default::{

    CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE, NON_INFORMATIVE_WORDS_REGEXS,
    POLISH_CAPTURE_REPLACE_PAIRS, SPLIT_DESCRIPTION_REGEX, SPLIT_GENE_FAMILY_GENES_REGEX,
    SPLIT_GENE_FAMILY_ID_FROM_GENE_SET,
};
use crate::description::apply_capture_replace_pairs;
use crate::error::Error;
use crate::hrd::{Annotation, Scoring};
use crate::output_writer::Annotated;
use crate::trace::TraceSink;
use crate::input::regex_files::{
    parse_pair_file, parse_pairs, parse_regex_file, parse_regexs, PairList,
};
use crate::input::seq_families::parse_seq_family;
use crate::input::seq_sim_table::{parse_table, ParseMessage, SeqSimTable};
use crate::model::annotee::Annotee;
use crate::model::query::Query;
use crate::model::seq_family::SeqFamily;
use rayon::prelude::*;
use regex::Regex;
use std::collections::{HashMap, HashSet};
// `TryFrom` is in the prelude only from edition 2021 on, and this crate is on edition 2018:
use std::convert::TryFrom;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::io::{BufRead, BufReader};
use std::fs::File;

/// An instance of AnnotationProcess represents exactly what its name suggest, the assignment of
/// human readable descriptions, i.e. the annotation of queries or sets of these (biological
/// sequence families) with short and concise textual descriptions.
#[derive(Debug, Clone)]
pub struct AnnotationProcess {
    /// The input sequence similarity search result tables to parse, each carrying the settings
    /// it is to be parsed with (see `SeqSimTable`).
    pub seq_sim_search_tables: Vec<SeqSimTable>,
    /// The in memory database of parsed sequence similarity search results in terms of Queries
    /// with their respective Hits.
    pub queries: HashMap<String, Query>,
    /// The in memory database of biological sequence families, i.e. sets of query identifiers, to
    /// be annotated with human readable descriptions. Keys are the families identifier and values
    /// are the SeqFamily instances.
    pub seq_families: HashMap<String, SeqFamily>,
    /// This string separates the gene-family-identifier (name) from the gene-identifier list
    /// that family comprises.
    pub seq_family_id_genes_separator: String,
    /// A regular expression (Rust syntax) represented as String to satisfy the `Default` trait.
    /// This regex is used to split the list of gene-identifiers in the gene families file.
    pub seq_family_gene_ids_separator: Regex,
    /// An in memory index from Query identifier to SeqFamily identifier:
    pub query_id_to_seq_family_id_index: HashMap<String, String>,
    /// A regular expression used to split descriptions (`stitle` in Blast terminology) into words.
    pub description_split_regex: Regex,
    /// The path to the optional argument file holding regular expression, one per line, used to
    /// recognize non informative words. If not given, the
    /// `default::BLACKLIST_DESCRIPTION_WORDS_REGEXS` is used.
    pub non_informative_words_regexs: Vec<Regex>,
    /// The human readable descriptions (HRDs) generated for the queries, i.e. either single query
    /// sequences or families (sets of query sequences). Stored here using the query identifier as
    /// key and the generated HRD as values.
    pub human_readable_descriptions: HashMap<String, Annotated>,
    /// A list of "capture-replace-pairs", tuples of regular expressions and replace strings, is
    /// held here. These pairs are used to polish assigned human readable descriptions.
    pub polish_capture_replace_pairs: PairList,
    /// A real value between zero and one used to center the inverse information content scores.
    pub center_iic_at_quantile: f64,
    /// The number of parallel threads to use.
    pub n_threads: usize,
    /// In mode FamilyAnnotation also annotate lonely queries, i.e. queries not comprised in a
    /// sequence family?
    pub annotate_lonely_queries: bool,
    /// Does the user want informative messages about the annotation process printed out?
    pub verbose: bool,
    /// Exclude results that could not be annotated from the output?
    pub exclude_not_annotated_from_output: bool,
    /// The BLAKE3 hash of each input table, by path, taken while it was read. What makes a
    /// recorded run a record of the data as well as of the settings.
    pub input_digests: HashMap<String, String>,
    /// Hold every query until all input has been read, instead of annotating each one as soon as
    /// its rows are behind it? Lets an input whose rows are not grouped by query be read, at the
    /// cost of memory in proportion to the whole input.
    pub buffer_unsorted_input: bool,
    /// Where to write an account of how each description was chosen, if anyone asked for one.
    /// Each account is written as its annotee is finished and then forgotten; see `crate::trace`.
    pub traces: Vec<TraceSink>,
    /// Whether this run annotates single query sequences or families of them. Resolved once, when
    /// the process is built, and never again -- see `AnnotationProcess::mode`.
    mode: AnnotationProcessMode,
}

/// Representation of the mode an instance of AnnotationProcess runs in. Can be either (i)
/// annotation of single biological query sequences `SequenceAnnotation`, or (ii) annotation of
/// sets of such query sequences `FamilyAnnotation`. Annotation means the generation of human
/// readable descriptions for either (i) single queries, or (ii) whole sets of biological
/// sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationProcessMode {
    SequenceAnnotation,
    FamilyAnnotation,
}

impl AnnotationProcess {
    /// Runs this annotation process: parses each input sequence similarity search result table in
    /// its own thread, inserts the queries they send as they arrive, and generates and polishes the
    /// human readable descriptions. Afterwards `self.human_readable_descriptions` holds the result.
    ///
    /// Returns the first failure any of the parsing threads reported, or the first the insertion
    /// of a parsed query caused. It is returned rather than acted upon here because only `main`
    /// knows what to do with it, and it is the *first* one because the later ones are usually its
    /// consequences.
    ///
    /// A run in which every input table yielded not one record is a failure too, `EmptyResult`.
    /// Such a run has not annotated a proteome that had nothing to say; it has not read the
    /// proteome at all, and what it would otherwise hand its caller is an output table that is
    /// indistinguishable from a real, empty analysis.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A reference to a mutable instance of AnnotationProcess.
    pub fn run(&mut self) -> Result<(), Error> {
        // Are we printing information verbosely? (Note that by copying this boolean, we avoid
        // running into problems with the borrow-checker in the threads' println! statement:
        let verbose = self.verbose;

        // If there are more input tables than the self.n_threads, only use n_threads
        // parallel processes.
        let n = if self.seq_sim_search_tables.len() <= self.n_threads {
            self.seq_sim_search_tables.len()
        } else {
            self.n_threads
        };

        // Setup communication between threads:
        let (tx, rx) = mpsc::channel();

        // Enable the threads to access the input sequence similarity search result tables. Each table
        // carries the settings it is to be parsed with, so this is the only state the parsing threads
        // share, and the lock is held just long enough to take the next table off the queue:
        let sssts_mutex = Arc::new(Mutex::new(self.seq_sim_search_tables.clone()));

        // Prepare `n` threads for sequence similarity search parsing, each thread will parse a table
        // not yet processed until no tables are left to be processed:
        for _ in 0..n {
            let tx_i = tx.clone();
            let sssts_mutex_clone = sssts_mutex.clone();

            // ... start the thread:
            thread::spawn(move || loop {
                let mut sssts = sssts_mutex_clone.lock().unwrap();

                // Stop, if all input sequence similarity search tables have been parsed already:
                if sssts.is_empty() {
                    break;
                }

                // Get the current input sequence similarity search table:
                let sss_tbl = sssts.pop().unwrap();
                // Free the lock, so other threads may access `sssts_mutex`:
                drop(sssts);

                // Because we are in a `loop` we need to clone the cloned sender:
                parse_table(&sss_tbl, tx_i.clone());

                // Inform user, if requested. Progress reports are diagnostics and go to
                // standard error, which leaves standard output for data:
                if verbose {
                    eprintln!("Finished parsing {:?}", sss_tbl.path);
                }
            });
        }
        // Because of the above for loop tx needs to be cloned into tx_i's. tx needs to be dropped,
        // otherwise the below receiver loop will wait forever for tx to send some messages.
        drop(tx);

        // Process messages sent by the above threads. Note that this might trigger the annotation of
        // some queries or sequence families, if their data has been parsed completely.
        //
        // The loop keeps draining the channel after the first failure instead of returning from
        // the middle of it, because the parsing threads are still running and the receiver is
        // what tells them their work is still wanted. Nothing more is inserted once a failure has
        // been seen; the annotation is not going to be produced either way:
        let mut failure: Option<Error> = None;
        let mut records_parsed: usize = 0;
        let mut tables_without_records: Vec<String> = Vec::new();
        for message in rx {
            match message {
                ParseMessage::Query(qacc, query) => {
                    if failure.is_none() {
                        if let Err(e) = self.insert_query(qacc, query) {
                            failure = Some(e);
                        }
                    }
                }
                ParseMessage::TableRead {
                    path,
                    records,
                    digest,
                } => {
                    records_parsed += records;
                    // Kept so that a run can record which data it annotated, not merely which
                    // paths it was pointed at:
                    self.input_digests.insert(path.clone(), digest);
                    if records == 0 {
                        tables_without_records.push(path);
                    }
                }
                ParseMessage::Failed(e) => {
                    if failure.is_none() {
                        failure = Some(e);
                    }
                }
            }
        }
        if let Some(e) = failure {
            return Err(e);
        }

        // Nothing was read anywhere. One empty table among several is an ordinary outcome -- a
        // database in which this query set simply found no hit -- but if that is true of every
        // one of them, then what is being described here is not a proteome without hits, it is a
        // command line that did not reach the data. The tables are named in the order the user
        // gave them, not the order the threads happened to finish in:
        if records_parsed == 0 {
            tables_without_records.sort_unstable_by_key(|path| {
                self.seq_sim_search_tables
                    .iter()
                    .position(|table| &table.path == path)
                    .unwrap_or(usize::MAX)
            });
            return Err(Error::EmptyResult(format!(
                "\n\nCannot run Annotation-Process, because not a single record could be read from the sequence similarity search result table(s):\n{}\nNothing was annotated and no output was written. Please check that these files hold the search results you expect, and that the --db-sep and --db-header arguments describe them.\n\n",
                tables_without_records
                    .iter()
                    .map(|path| format!("  {:?}", path))
                    .collect::<Vec<String>>()
                    .join("\n")
            )));
        }

        // Make sure all queries or sequence families are annotated:
        self.process_rest_data();

        // Every account of an annotation has been written by now; what is left is to make sure it
        // arrived, and to say so if an identifier the user asked about never turned up:
        for sink in &self.traces {
            sink.finish()?;
        }

        // The other empty result, and the one that is not an error: the input was read, and what
        // it holds does not describe anything. That is a finding about the proteome and the run
        // succeeded in establishing it, so it is said on standard error and the header-only
        // output table is written as usual -- but it is said, because a header-only table looks
        // the same from the outside whether it is the truth or a mistake:
        if self.human_readable_descriptions.is_empty() {
            eprintln!(
                "\nWarning: {} record(s) were read from the input table(s), but no annotation could be generated from any of them; the output holds its header line and nothing else. Every description was either discarded by the --db-blacklist list or emptied by the --db-filter list.\n",
                records_parsed
            );
        }

        Ok(())
    }

    /// Creates a default instance of struct AnnotationProcess and returns it.
    pub fn new() -> AnnotationProcess {
        let nt = if num_cpus::get() < 2 {
            2
        } else {
            num_cpus::get()
        };
        AnnotationProcess {
            seq_sim_search_tables: vec![],
            queries: HashMap::new(),
            seq_families: HashMap::new(),
            seq_family_id_genes_separator: (*SPLIT_GENE_FAMILY_ID_FROM_GENE_SET).to_string(),
            seq_family_gene_ids_separator: (*SPLIT_GENE_FAMILY_GENES_REGEX).clone(),
            description_split_regex: (*SPLIT_DESCRIPTION_REGEX).clone(),
            non_informative_words_regexs: (*NON_INFORMATIVE_WORDS_REGEXS).clone(),
            query_id_to_seq_family_id_index: HashMap::new(),
            human_readable_descriptions: HashMap::new(),
            traces: vec![],
            polish_capture_replace_pairs: (*POLISH_CAPTURE_REPLACE_PAIRS).clone(),
            center_iic_at_quantile: CENTER_INVERSE_INFORMATION_CONTENT_AT_QUANTILE,
            n_threads: nt,
            annotate_lonely_queries: false,
            verbose: false,
            exclude_not_annotated_from_output: false,
            buffer_unsorted_input: false,
            input_digests: HashMap::new(),
            mode: AnnotationProcessMode::SequenceAnnotation,
        }
    }

    /// Processes the sequence similarity search result (SSSR) data parsed for the argument
    /// `query`. Adds the data to existing one, of data already has been parsed for the argument
    /// `query` from a different SSSR file or inserts the new data into the in-memory database
    /// `self.queries`. If for the argument query all input SSSR tables
    /// (`self.seq_sim_search_tables`) have produced data, this data will be processed by invoking
    /// `self.process_query_data_complete`.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of AnnotationProcess, which
    ///   serves as an in memory database into which to insert the parsed query.
    /// * `qacc: String` - The identifier of the argument query, i.e. the to be key in
    ///   self.queries.
    /// * `query: Query` - A reference to the query to be inserted into the in memory database.
    pub fn insert_query(&mut self, qacc: String, query: Query) -> Result<(), Error> {
        // Fail if query.id already in results, this means the input SSSR files were not sorted
        // by query identifiers (`qacc` in Blast terminology):
        if !self.buffer_unsorted_input && self.human_readable_descriptions.contains_key(&qacc) {
            return Err(Error::MalformedData(format!( "\n\nFound query {:?} again after its rows appeared to be behind it. All rows belonging to one query must stand together in an input table, which is how Blast and Diamond write their output; concatenating tables, or sorting one by anything other than the query column, does not preserve it.\n\nEither group the rows -- 'sort -s -t\"<TAB>\" -k1,1 <your-table>' does it, and being a stable sort on the query column alone it leaves the order of each query's hits alone -- or give --unsorted-input, which holds every query until all input has been read. That reads any table, at the cost of needing memory in proportion to the whole input rather than to one query.\n\n", qacc)));
        }
        if !self.queries.contains_key(&qacc) {
            self.queries.insert(qacc.clone(), query);
        } else {
            let already_parsed_query = self.queries.get_mut(&qacc).unwrap();
            already_parsed_query.hits.extend(query.hits.clone());
        }

        let stored_query = self.queries.get_mut(&qacc).unwrap();
        stored_query.n_parsed_from_sssr_tables += 1;
        // Have all input SSSR files provided data for the argument `query`?
        //
        // Not asked when the input is not grouped by query: more rows for this query may still be
        // ahead, so there is no moment during parsing at which it is known to be finished.
        // Everything is annotated by `process_rest_data` once all input has been read.
        if !self.buffer_unsorted_input
            && stored_query.n_parsed_from_sssr_tables == self.seq_sim_search_tables.len() as u16
        {
            let _ = stored_query;
            // If yes, then process the parsed data:
            self.process_query_data_complete(qacc);
        }

        Ok(())
    }

    /// Inserts the argument `seq_family: SeqFamily` into this AnnotationProcess instance's
    /// `seq_families`, while also updating the in memory index of biological query sequence
    /// identifiers pointing to their respective sequence family (see
    /// `query_id_to_seq_family_id_index`).
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of AnnotationProcess, which
    ///   serves as an in memory database into which to insert the argument
    ///   biological sequence family.
    pub fn insert_seq_family(
        &mut self,
        seq_family_id: String,
        seq_family: SeqFamily,
    ) -> Result<(), Error> {
        for query_id in &seq_family.query_ids {
            if self.query_id_to_seq_family_id_index.contains_key(query_id) {
                let other_family_id = self.query_id_to_seq_family_id_index.get(query_id).unwrap();
                if *other_family_id != seq_family_id {
                    return Err(Error::MalformedData(format!("\n\nBiological sequence {:?} already set as member of family {:?}. But found {:?} again declared as member of another family {:?}.\nMake sure each biological sequence appears in one and only one family to avoid this problem.\n\n", query_id, other_family_id, query_id, seq_family_id)));
                }
            }
            self.query_id_to_seq_family_id_index
                .insert((*query_id).clone(), seq_family_id.clone());
        }
        self.seq_families.insert(seq_family_id, seq_family);
        // A run that has been given a family annotates families, and goes on doing so after the
        // last of them has been annotated and removed:
        self.mode = AnnotationProcessMode::FamilyAnnotation;

        Ok(())
    }

    /// The mode this AnnotationProcess runs in: annotation of single biological query sequences
    /// (`SequenceAnnotation`) or of sets of them (`FamilyAnnotation`).
    ///
    /// It is set when the first family is inserted and never unset, rather than derived on every
    /// use. It used to be re-derived from `self.seq_families` -- but that map is *drained*, a
    /// family being removed as it is annotated, so a run in family mode turned into a run in
    /// sequence mode the moment its last family was finished. A query still being completed after that was
    /// then annotated as a plain sequence, which is what `--annotate-non-family-queries` (`-a`)
    /// exists to ask for and had not been asked for. Whether that happened depended on nothing the
    /// user could see: one unrelated family left incomplete kept the map non-empty and the run
    /// honest.
    pub fn mode(&self) -> AnnotationProcessMode {
        self.mode
    }

    /// How this run scores words: settled once, and read by every annotation in the run.
    pub fn scoring(&self) -> Scoring<'_> {
        Scoring {
            split_regex: &self.description_split_regex,
            non_informative_words_regexs: &self.non_informative_words_regexs,
            center_at_quantile: self.center_iic_at_quantile,
        }
    }

    /// Function generates a human readable description (HRD) for the argument `query_id`. The
    /// resulting HRD is stored in `self.human_readable_descriptions` and thus the query is marked
    /// as processed. In order to optimize memory footprint the query and all of its contained
    /// sequence similarity search result (Hits in Blast terminology) data is removed.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of AnnotationProcess, which
    ///   serves as an in memory database into which to insert the parsed query.
    /// * `query_id: String` - An instance of `String` representing the query identifier
    pub fn annotate_query(&mut self, query_id: String) {
        // Generate the desired result, i.e. a human readable description for the Query:
        let scoring = self.scoring();
        let annotation = self.queries.get(&query_id).unwrap().annotate(
            &scoring,
            !self.traces.is_empty(),
        );
        // Add the new result to the in memory database, i.e.
        // `self.human_readable_descriptions`:
        if let Some(hrd) = self.conclude(&query_id, Annotee::Query, annotation) {
            self.human_readable_descriptions.insert(query_id.clone(), hrd);
        }
        // Free memory by removing the parsed input data, no longer required:
        self.queries.remove(&query_id);
    }

    /// Function generates a human readable description (HRD) for the argument `seq_family_id`. The
    /// resulting HRD is stored in `self.human_readable_descriptions` and thus the family is marked
    /// as processed. In order to optimize memory footprint the family and all of its contained
    /// query data is removed.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of AnnotationProcess, which
    ///   serves as an in memory database into which to insert the parsed query.
    /// * `seq_family_id: &String` - A reference to a `String` representing the biological sequence
    ///   family's (`SeqFamily`) identifier.
    pub fn annotate_seq_family(&mut self, seq_family_id: &String) {
        // Generate the desired result, i.e. a human readable description for the SeqFamily:
        let scoring = self.scoring();
        let seq_family = self.seq_families.get(seq_family_id).unwrap();
        let annotation = seq_family.annotate(
            &self.queries,
            &scoring,
            !self.traces.is_empty(),
        );
        // Add the new result to the in memory database, i.e.
        // `self.human_readable_descriptions`:
        if let Some(hrd) = self.conclude(seq_family_id, Annotee::Family, annotation) {
            self.human_readable_descriptions
                .insert((*seq_family_id).clone(), hrd);
        }
        // need to clone, otherwise had problems with the compiler (E0599):
        let query_ids = seq_family.query_ids.clone();
        // Free memory by removing the parsed input data, no longer required:
        for query_id in query_ids.iter() {
            self.queries.remove(query_id);
            self.query_id_to_seq_family_id_index.remove(query_id);
            self.seq_families.remove(seq_family_id);
        }
    }

    /// Invoked whenever a query instance has been supplied with results from _all_ sequence
    /// similarity search result (SSSR) files, implying that for that particular instance of
    /// `Query` no more SSSR results (Hits in Blast terminology) can be parsed. Thus, that query
    /// can be processed and a human readable description can be generated for it. If this instance
    /// of `AnnotationProcess` (`self`) is run in `AnnotationProcessMode::FamilyAnnotation` a
    /// similar approach is triggered for the SeqFamily that contains the argument `query_id`. If
    /// that family has SSSR result data for _all_ of its contained queries, the family will be
    /// processed and annotated.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of AnnotationProcess, which
    ///   serves as an in memory database into which to insert the parsed query.
    pub fn process_query_data_complete(&mut self, query_id: String) {
        let mode = self.mode();
        match mode {
            // Handle annotation of single biological sequences:
            AnnotationProcessMode::SequenceAnnotation => {
                self.annotate_query(query_id);
            }
            // Handle annotation of sets of biological sequences, so called "Gene Families":
            AnnotationProcessMode::FamilyAnnotation => {
                // Get SeqFamily for query sequence identifier (`query_id`):
                if self.query_id_to_seq_family_id_index.contains_key(&query_id) {
                    let seq_fam_id = self
                        .query_id_to_seq_family_id_index
                        .get(&query_id)
                        .unwrap()
                        .clone();
                    if self.seq_families.contains_key(&seq_fam_id) {
                        // Tell Family that parsing of Blast results for argument `query_id` has
                        // been completed:
                        let seq_fam = self.seq_families.get_mut(&seq_fam_id).unwrap();
                        seq_fam.mark_query_id_with_complete_data(&query_id);
                        // Ask SeqFamily if all queries have complete data:
                        if seq_fam.all_query_data_complete() {
                            self.annotate_seq_family(&seq_fam_id);
                        }
                    }
                } else if self.annotate_lonely_queries {
                    // If no family for qs_id can be found, annotate query as in
                    // SequenceAnnotation:
                    self.annotate_query(query_id);
                }
            }
        }
    }

    /// Invoked after all parsing of input sequence similarity search result (SSSR) files has
    /// finished to annotate those queries or biological sequence families that have not yet been
    /// annotated. These are those that do not have results in each separate SSSR file.
    ///
    /// # Arguments
    ///
    /// * `&mut self` - A mutable reference to the current instance of AnnotationProcess, which
    ///   serves as an in memory database into which to insert the parsed query.
    pub fn process_rest_data(&mut self) {
        // Note that below `par_iter` is used to process the data _in parallel_. To make this work
        // the parallel processes need to be independent and cannot write results of annotations
        // (HRDs) into the current instance of AnnotationProcess without using something like an
        // Mutex. Thus results are collected in terms of tuples containing the annotee identifier
        // and the generated human readable description.
        let mode = self.mode();
        let scoring = self.scoring();
        // Each entry is already concluded -- polished, or stood in for by an "unknown protein"
        // -- because what it was chosen from must not outlive it; see `conclude`. `None` is an
        // annotee the user asked to have left out of the output altogether.
        let hrd_tuples: Vec<(String, Option<Annotated>)> = match mode {
            // Handle annotation of single biological sequences:
            AnnotationProcessMode::SequenceAnnotation => {
                // Process queries that might have gotten parsed results only from a subset of the input
                // sequence similarity search result (SSSR) files:
                self.queries
                    .keys()
                    .cloned()
                    .collect::<Vec<String>>()
                    .par_iter()
                    .map(|query_id| {
                        let query = self.queries.get(query_id).unwrap();
                        let annotation = query.annotate(
                            &scoring,
                            !self.traces.is_empty(),
                        );
                        (
                            (*query_id).to_string(),
                            self.conclude(query_id, Annotee::Query, annotation),
                        )
                    })
                    .collect()
            }
            // Handle annotation of sets of biological sequences, so called "Gene Families":
            AnnotationProcessMode::FamilyAnnotation => {
                // Process seq families that might have queries that got no blast hits in some
                // input blast tables:
                let mut families: Vec<(String, Option<Annotated>)> = self
                    .seq_families
                    .keys()
                    .cloned()
                    .collect::<Vec<String>>()
                    .par_iter()
                    .map(|seq_fam_id| {
                        let seq_fam = self.seq_families.get(seq_fam_id).unwrap();
                        let annotation = seq_fam.annotate(
                            &self.queries,
                            &scoring,
                            !self.traces.is_empty(),
                        );
                        (
                            (*seq_fam_id).to_string(),
                            self.conclude(seq_fam_id, Annotee::Family, annotation),
                        )
                    })
                    .collect();

                // And the queries belonging to no family at all, if the user asked for them. A
                // query that has no hit in one of the input tables is never "complete", so
                // `process_query_data_complete` never considered it and it is still here. Until
                // the annotation mode was fixed this was reached by accident, the mode having
                // fallen back to sequence annotation once the last family was gone:
                if self.annotate_lonely_queries {
                    let lonely: Vec<(String, Option<Annotated>)> = self
                        .queries
                        .keys()
                        .filter(|query_id| {
                            !self.query_id_to_seq_family_id_index.contains_key(*query_id)
                        })
                        .cloned()
                        .collect::<Vec<String>>()
                        .par_iter()
                        .map(|query_id| {
                            let query = self.queries.get(query_id).unwrap();
                            let annotation = query.annotate(
                                &scoring,
                                !self.traces.is_empty(),
                            );
                            (
                                (*query_id).to_string(),
                                self.conclude(query_id, Annotee::Query, annotation),
                            )
                        })
                        .collect();
                    families.extend(lonely);
                }
                families
            }
        };

        // Free memory:
        self.queries = Default::default();
        self.seq_families = Default::default();
        self.query_id_to_seq_family_id_index = Default::default();

        // Set the human readable descriptions generated in parallel:
        for (annotee_id, hrd) in hrd_tuples {
            if let Some(hrd) = hrd {
                self.human_readable_descriptions.insert(annotee_id, hrd);
            }
        }
    }

    /// Turns one finished annotation into the description that will be reported for it, and lets
    /// go of everything that description was chosen from.
    ///
    /// The last step of generating a human readable description is to "polish" it, by iteratively
    /// applying capture-replace pairs to it. That used to be a pass over all of the descriptions
    /// once the run was over. It happens here instead, where each one is produced: polishing is a
    /// rewrite of one description and the result is the same either way, but an annotation that is
    /// finished the moment it is made is one that can be *written out* the moment it is made,
    /// which is what an explanation of a run has to be if it is not to grow with the input.
    ///
    /// Returns `None` for an annotee that could not be annotated and that the user asked to have
    /// left out of the output (`--exclude-not-annotated-queries`); otherwise the description, or
    /// the "unknown protein" or "unknown sequence family" that stands in for one.
    ///
    /// # Arguments
    ///
    /// * `id` - Which annotee, by its identifier.
    /// * `kind` - Whether a query or a whole family was annotated.
    /// * `annotation` - What the annotation of it consisted of.
    fn conclude(&self, id: &str, kind: Annotee, annotation: Annotation) -> Option<Annotated> {
        let mut hrd = annotation
            .description
            .clone()
            .unwrap_or_else(|| kind.unknown().to_string());
        apply_capture_replace_pairs(&mut hrd, Some(&self.polish_capture_replace_pairs));
        // Written out here, where it costs one annotee's worth of memory, and not kept:
        for sink in &self.traces {
            if sink.wants(id) {
                sink.record(id, kind, &annotation, &hrd);
            }
        }
        // An annotee left out of the table is still an annotee the user can ask about, so the
        // account of it above is written either way:
        if annotation.description.is_none() && self.exclude_not_annotated_from_output {
            return None;
        }
        Some(Annotated {
            description: hrd,
            score: annotation.score,
            hits: annotation.scored.len(),
            phrases: annotation.candidates.len(),
        })
    }

    /// Parses the command line argument --polish-capture-replace-pairs, which takes what every
    /// other rule list takes: a file, an `@NAME` built-in, or `none`.
    ///
    /// # Arguments
    ///
    /// * self - A mutable reference to the instance of AnnotationProcess
    /// * polish_capture_replace_pairs_arg - A scalar `&str` the provided command line argument
    ///   value
    pub fn set_polish_capture_replace_pairs(
        &mut self,
        polish_capture_replace_pairs_arg: &str,
    ) -> Result<(), Error> {
        let source = polish_capture_replace_pairs_arg.trim();
        self.polish_capture_replace_pairs = if source.eq_ignore_ascii_case("default") {
            (*POLISH_CAPTURE_REPLACE_PAIRS).clone()
        } else {
            crate::assets::resolve(
                source,
                parse_pair_file,
                parse_pairs,
            )?
        };

        Ok(())
    }

    /// Parses the command line argument --non-informative-words-regexs, which takes what every
    /// other rule list takes: a file, an `@NAME` built-in, or `none`.
    ///
    /// `none` means no list at all, so no word is held to be non-informative. There was no way to
    /// say that before, the option being a file name or nothing.
    ///
    /// # Arguments
    ///
    /// * self - A mutable reference to the instance of AnnotationProcess
    /// * non_informative_words_regexs_arg - A scalar `&str` the provided command line argument
    ///   value
    pub fn set_non_informative_words_regexs(
        &mut self,
        non_informative_words_regexs_arg: &str,
    ) -> Result<(), Error> {
        let source = non_informative_words_regexs_arg.trim();
        self.non_informative_words_regexs = if source.eq_ignore_ascii_case("default") {
            (*NON_INFORMATIVE_WORDS_REGEXS).clone()
        } else {
            crate::assets::resolve(source, parse_regex_file, parse_regexs)?
        };

        Ok(())
    }

    /// Parses line by line of the argument file `path` in which sets of biological sequence
    /// identifiers (a.k.a. gene families) are stored; one family per line. Each parsed family is
    /// stored in the argument `annotation_process`.
    ///
    /// # Arguments
    ///
    /// * `path` - The valid path to the file holding the to be parsed gene families.
    /// * `annotation_process` - The AnnotationProcess to be provided with the parsed gene families.
    pub fn parse_seq_families_file(&mut self, path: &str) -> Result<(), Error> {
        // Open stream to the gene families input file
        let file_path = path.to_string();
        let file = File::open(path)
            .map_err(|e| Error::opening(path, format!("No such file {:?}", path), &e))?;
        let reader = BufReader::new(file);
        // Already compiled: by clap for a command line, and by `plan::compile` for a run plan.
        // Each of those knows what carried the expression, which is what a reader needs and what
        // this could never say -- it was reached from both and named only one.
        // Cloned, not borrowed: the loop below inserts into `self`. Once per file, as the
        // compile it replaces was.
        let seq_family_gene_ids_separator = self.seq_family_gene_ids_separator.clone();
        // read file line by line
        for (i, line) in reader.lines().enumerate() {
            let family_line = line.map_err(|e| Error::reading(path, &e))?;

            // parse line. fail if malformatted, add to the annotation_process if OK
            match parse_seq_family(family_line, &self.seq_family_id_genes_separator, &seq_family_gene_ids_separator) {
                Ok((seq_fam_name, seq_fam_instance)) => {
                    self.insert_seq_family(seq_fam_name, seq_fam_instance)?
                }
                Err(e) => return Err(Error::MalformedData(format!("\n\n{:?} in file {:?} line <{:?}>. The expected format is \"<family-name>TABgene1,gene2,gene3,...\"\n\n", e, file_path, i))),
            }
        }

        Ok(())
    }
}

/// Fails unless every input table has a name of its own, since a name that means two tables cannot
/// be used to say which of them an option is for.
///
/// # Arguments
///
/// * `tables` - The input tables, in the order the user declared them.
fn reject_repeated_table_names(tables: &[SeqSimTable]) -> Result<(), Error> {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for table in tables {
        if let Some(first) = seen.insert(&table.name, &table.path) {
            return Err(Error::Usage(format!(
                "\n\nCannot run Annotation-Process, because two input tables are both called {:?}:\n  {}\n  {}\nA table is named by what stands before the '=' in --db NAME=PATH, or, when no name is given, after its file. Give at least one of them a name of its own.\n\n",
                table.name, first, table.path
            )));
        }
    }
    Ok(())
}

/// The table a per-table `--db-*` argument names, or the usage error of naming one that was never
/// declared.
///
/// # Arguments
///
/// * `tables` - The input tables the user declared.
/// * `option` - The option being resolved, as the user writes it.
/// * `argument` - Its `NAME=VALUE` argument.
fn find_table_index<T>(
    tables: &[SeqSimTable],
    option: &str,
    argument: &NamedValue<T>,
    already_given: &mut HashSet<String>,
) -> Result<usize, Error> {
    // Naming the tables is what allows an option to be given for some of them and not others, so
    // the positional form's "either none or one per table" no longer applies. What does still
    // apply is that one table cannot be given two answers to the same question: applying both and
    // keeping whichever came last would decide it by argument order, which is the thing this
    // interface exists to stop deciding anything.
    if !already_given.insert(argument.name.clone()) {
        return Err(Error::Usage(format!(
            "\n\nCannot run Annotation-Process, because {} was given more than once for the table {:?}. Each table takes at most one {}; give it once, or not at all to use prot-scriber's default.\n\n",
            option, argument.name, option
        )));
    }
    // The name is compared against what was declared, so a typo is refused here rather than
    // quietly applied to whichever table happened to be written first:
    let declared: Vec<String> = tables.iter().map(|table| format!("{:?}", table.name)).collect();
    tables
        .iter()
        .position(|table| table.name == argument.name)
        .ok_or_else(|| {
            Error::Usage(format!(
                "\n\nCannot run Annotation-Process, because {} names the table {:?}, which was not declared. The tables given with --db are: {}.\n\n",
                option,
                argument.name,
                if declared.is_empty() { String::from("none") } else { declared.join(", ") }
            ))
        })
}



impl TryFrom<&Args> for AnnotationProcess {
    type Error = Error;

    /// Builds the annotation process the argument command line describes, or reports the first
    /// thing about that command line that makes it impossible: an argument that cannot be paired
    /// with an input table, a header without one of the columns prot-scriber reads, a file of
    /// regular expressions that is not there. All of it happens before a single table is parsed,
    /// so a run that cannot work does not first spend an hour finding that out.
    ///
    /// # Arguments
    ///
    /// * `args` - The parsed command line arguments.
    fn try_from(args: &Args) -> Result<Self, Error> {
        let mut annotation_process: Self = Self::new();

        // Does the user want informative messages printed out?
        annotation_process.verbose = args.verbose;

        // Set number of parallel processes to use:
        if let Some(n_threads) = args.n_threads {
            annotation_process.n_threads = n_threads;
        }

        // Add biological sequence families information, if provided as input by the user:
        if let Some(seq_families) = &args.seq_families {
            // What is the character that separates a gene-family-identifier from its list of
            // gene-identifiers?
            // Not trimmed: a separator is whatever the user says it is, and trimming empties
            // the ones most worth spelling out -- a literal TAB, which is the default, or a space.
            if let Some(separator) = &args.seq_family_id_genes_separator {
                annotation_process.seq_family_id_genes_separator = separator.clone();
            }

            // What is the regular expression (string representation) that shall be used to split
            // the list of gene-identifiers a gene-family comprises?
            if let Some(separator) = &args.seq_family_gene_ids_separator {
                annotation_process.seq_family_gene_ids_separator = separator.clone();
            }

            // Shall non family queries also be annotated? Note that clap rejects this flag unless
            // --seq-families (-f) is given, so it is only ever read here.
            annotation_process.annotate_lonely_queries = args.annotate_non_family_queries;

            annotation_process.parse_seq_families_file(seq_families)?;
            if annotation_process.verbose {
                eprintln!(
                    "Loaded {:?} sequence families from {:?}",
                    annotation_process.seq_families.len(),
                    seq_families
                );
            }
        }

        // Build the input sequence similarity search result (SSSR) tables (Blast or Diamond),
        // each with prot-scriber's compiled in defaults, then apply the per table arguments the
        // user did provide. Every one of those names the table it belongs to, so there is nothing
        // here to count and no order to get wrong.
        let mut seq_sim_search_tables: Vec<SeqSimTable> = args
            .seq_sim_table
            .iter()
            .map(|declaration| {
                SeqSimTable::new(declaration.name.clone(), declaration.value.clone())
            })
            .collect();
        reject_repeated_table_names(&seq_sim_search_tables)?;

        // Each argument belongs to the table it names, and to no other. Order is not consulted,
        // so there is no order to get wrong.
        let mut named = HashSet::new();
        for header in &args.db_header {
            let index =
                find_table_index(&seq_sim_search_tables, "--db-header", header, &mut named)?;
            seq_sim_search_tables[index].set_header(&header.value);
        }
        named.clear();
        for separator in &args.db_sep {
            let index =
                find_table_index(&seq_sim_search_tables, "--db-sep", separator, &mut named)?;
            seq_sim_search_tables[index].set_field_separator(separator.value);
        }
        named.clear();
        for blacklist in &args.db_blacklist {
            let index =
                find_table_index(&seq_sim_search_tables, "--db-blacklist", blacklist, &mut named)?;
            seq_sim_search_tables[index].set_blacklist_regexs(&blacklist.value)?;
        }
        named.clear();
        for filter in &args.db_filter {
            let index =
                find_table_index(&seq_sim_search_tables, "--db-filter", filter, &mut named)?;
            seq_sim_search_tables[index].set_filter_regexs(&filter.value)?;
        }
        named.clear();
        for pairs in &args.db_capture_replace {
            let index =
                find_table_index(&seq_sim_search_tables, "--db-capture-replace", pairs, &mut named)?;
            seq_sim_search_tables[index].set_capture_replace_pairs(&pairs.value)?;
        }

        annotation_process.seq_sim_search_tables = seq_sim_search_tables;

        // Set the capture replace pairs (fancy-regex) used in the last step of the generation of
        // human readable descriptions. Note, that this can be "none" or "default".
        if let Some(polish_capture_replace_pairs) = &args.polish_capture_replace_pairs {
            annotation_process.set_polish_capture_replace_pairs(polish_capture_replace_pairs)?;
        }

        // Did the user supply a custom regular expression to split descriptions (`stitle` in Blast
        // terminology) into words? Note that clap has already compiled it.
        if let Some(description_split_regex) = &args.description_split_regex {
            annotation_process.description_split_regex = description_split_regex.clone();
        }

        // Did the user supply a custom quantile (percentile) value to be used to center inverse
        // word information content scores? Note that clap has already checked its range.
        if let Some(center_at_quantile) = args.center_inverse_word_information_content_at_quantile {
            annotation_process.center_iic_at_quantile = center_at_quantile;
        }

        // Did the user provide regular expressions, one per line, to be used to recognize
        // non-informative words? A file, an '@NAME' built-in, or 'none' -- the same three things
        // the per-table rule lists take.
        if let Some(non_informative_words_regexs) = &args.non_informative_words_regexs {
            annotation_process
                .set_non_informative_words_regexs(non_informative_words_regexs)?;
        }

        // Shall non annotable queries or sequence families be excluded from the output table?
        annotation_process.exclude_not_annotated_from_output = args.exclude_not_annotated_queries;
        annotation_process.buffer_unsorted_input = args.unsorted_input;

        Ok(annotation_process)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::path::Path;

    #[test]
    fn new_annotation_process_initializes_fields() {
        let ap = AnnotationProcess::new();
        assert_eq!(ap.seq_sim_search_tables.len(), 0);
        assert_eq!(ap.seq_families.len(), 0);
    }

    #[test]
    fn the_mode_is_fixed_by_the_first_family_and_survives_the_map_emptying() {
        let mut ap = AnnotationProcess::new();
        assert_eq!(ap.mode(), AnnotationProcessMode::SequenceAnnotation);

        // meaningless empty SeqFamily, but none the less...
        ap.insert_seq_family("Family1".to_string(), SeqFamily::new())
            .unwrap();
        assert_eq!(ap.mode(), AnnotationProcessMode::FamilyAnnotation);

        // Annotating a family removes it. That must not turn this into a run over single
        // sequences, which is what used to happen and what annotated queries belonging to no
        // family without --annotate-non-family-queries (-a) ever being given:
        ap.seq_families.clear();
        assert_eq!(
            ap.mode(),
            AnnotationProcessMode::FamilyAnnotation,
            "the mode followed the family map back to sequence annotation"
        );
    }

    #[test]
    fn insert_query_works() {
        let mut ap = AnnotationProcess::new();
        // let mut nq1 = Query::from_qacc("Soltu.DM.02G015700.1".to_string());
        let mut nq1 = Query::new();
        let h1 = ("hit_One","sp|C0LGP4|Y3475_ARATH Probable LRR receptor-like serine/threonine-protein kinase At3g47570 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1");
        let h2 = ("hit_Two","sp|C0LGP4|Y3475_ARATH Probable LRR receptor-like serine/threonine-protein kinase At3g47570 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1");
        nq1.hits.insert(h1.0.to_string(), h1.1.to_string());
        nq1.hits.insert(h2.0.to_string(), h2.1.to_string());
        // Test insert_query
        let qacc = "Soltu.DM.02G015700.1".to_string();
        ap.insert_query(qacc.clone(), nq1).unwrap();

        // check if query got inserted correctly
        assert!(ap.queries.contains_key(&qacc));
        assert_eq!(ap.queries.get(&qacc).unwrap().hits.len(), 2);

        // check if the hits for the query are present
        let sq = ap.queries.get(&qacc).unwrap();
        assert!(sq.hits.contains_key(h1.0));
        assert!(sq.hits.contains_key(h2.0));

        // Test two databases being read in:
        let mut nq2 = Query::new();
        let h3 = ("hit_Three","tr|A0A2G9HZP3|A0A2G9HZP3_9LAMI Serine/threonine protein kinase OS=Handroanthus impetiginosus OX=429701 GN=CDL12_04291 PE=4 SV=1");
        let h4 = ("hit_Four","tr|A0A0V0ITN0|A0A0V0ITN0_SOLCH Protein kinase domain-containing protein OS=Solanum chacoense OX=4108 PE=4 SV=1");
        nq2.hits.insert(h3.0.to_string(), h3.1.to_string());
        nq2.hits.insert(h4.0.to_string(), h4.1.to_string());
        ap.insert_query(qacc.clone(), nq2).unwrap();

        // check if query 'nq2' got inserted correctly
        assert!(ap.queries.contains_key(&qacc));
        assert_eq!(ap.queries.get(&qacc).unwrap().hits.len(), 4);

        // check if the hits for the query are present
        let sq = ap.queries.get(&qacc).unwrap();
        assert!(sq.hits.contains_key(h1.0));
        assert!(sq.hits.contains_key(h2.0));
        assert!(sq.hits.contains_key(h3.0));
        assert!(sq.hits.contains_key(h4.0));
    }

    #[test]
    fn insert_query_reports_an_unsorted_blast_table() {
        let mut ap = AnnotationProcess::new();
        let nq1 = Query::new();
        let qacc = "Soltu.DM.02G015700.1".to_string();

        // Mark nq1 as already processed:
        ap.human_readable_descriptions
            .insert(
                qacc.clone(),
                Annotated {
                    description: String::from("Unknown protein"),
                    ..Default::default()
                },
            );
        // A query that has already been annotated coming back means the input was not sorted by
        // query identifier, which is a property of the input file and not a bug:
        assert!(matches!(
            ap.insert_query(qacc, nq1),
            Err(Error::MalformedData(_))
        ));
    }

    #[test]
    fn insert_family_works() {
        let mut ap = AnnotationProcess::new();
        let mut sf1 = SeqFamily::new();
        sf1.query_ids = vec![
            "Query1".to_string(),
            "Query2".to_string(),
            "Query3".to_string(),
        ];
        let sf_id1 = "SeqFamily1".to_string();
        ap.insert_seq_family(sf_id1.clone(), sf1).unwrap();
        assert!(ap.seq_families.contains_key("SeqFamily1"));
        assert_eq!(
            *ap.query_id_to_seq_family_id_index.get("Query1").unwrap(),
            sf_id1
        );
        assert_eq!(
            *ap.query_id_to_seq_family_id_index.get("Query2").unwrap(),
            sf_id1
        );
        assert_eq!(
            *ap.query_id_to_seq_family_id_index.get("Query3").unwrap(),
            sf_id1
        );
        let mut sf2 = SeqFamily::new();
        sf2.query_ids = vec![
            "Query4".to_string(),
            "Query5".to_string(),
            "Query6".to_string(),
        ];
        let sf_id2 = "SeqFamily2".to_string();
        ap.insert_seq_family(sf_id2.clone(), sf2).unwrap();
        assert!(ap.seq_families.contains_key("SeqFamily2"));
        assert_eq!(
            *ap.query_id_to_seq_family_id_index.get("Query4").unwrap(),
            sf_id2
        );
        assert_eq!(
            *ap.query_id_to_seq_family_id_index.get("Query5").unwrap(),
            sf_id2
        );
        assert_eq!(
            *ap.query_id_to_seq_family_id_index.get("Query6").unwrap(),
            sf_id2
        );
    }

    #[test]
    fn double_assignment_of_seq_id_to_different_families_is_reported() {
        let mut ap = AnnotationProcess::new();
        let mut sf1 = SeqFamily::new();
        sf1.query_ids = vec![
            "Query1".to_string(),
            "Query2".to_string(),
            "Query3".to_string(),
        ];
        let sf_id1 = "SeqFamily1".to_string();
        ap.insert_seq_family(sf_id1.clone(), sf1).unwrap();
        let mut sf2 = SeqFamily::new();
        sf2.query_ids = vec!["Query1".to_string(), "Query4".to_string()];
        let sf_id2 = "SeqFamily2".to_string();
        assert!(matches!(
            ap.insert_seq_family(sf_id2, sf2),
            Err(Error::MalformedData(_))
        ));
    }

    // This test also tests the functions
    // * annotate_query
    // * annotate_seq_family
    // implicitly
    #[test]
    fn process_query_data_complete_works() {
        // Test queries:
        let mut ap = AnnotationProcess::new();
        ap.seq_sim_search_tables = vec![SeqSimTable::new("blast_out_table".to_string(), "blast_out_table.txt".to_string())];
        // let mut nq1 = Query::from_qacc("Soltu.DM.02G015700.1".to_string());
        let mut nq1 = Query::new();
        let qacc = "Soltu.DM.02G015700.1".to_string();
        ap.insert_query(qacc.clone(), nq1).unwrap();
        // Query should have been annotated:
        assert!(ap.human_readable_descriptions.contains_key(&qacc));
        assert!(!ap.queries.contains_key(&qacc));
        // Test families:
        ap = AnnotationProcess::new();
        ap.seq_sim_search_tables = vec![SeqSimTable::new("blast_out_table".to_string(), "blast_out_table.txt".to_string())];
        let mut sf1 = SeqFamily::new();
        sf1.query_ids = vec!["Soltu.DM.02G015700.1".to_string()];
        let sf_id1 = "SeqFamily1".to_string();
        ap.insert_seq_family(sf_id1.clone(), sf1).unwrap();
        nq1 = Query::new();
        ap.insert_query(qacc.clone(), nq1).unwrap();
        // Family should have been annotated:
        assert!(ap.human_readable_descriptions.contains_key(&sf_id1));
        assert!(!ap.queries.contains_key(&qacc));
        assert!(!ap.seq_families.contains_key(&sf_id1));
        assert!(!ap.query_id_to_seq_family_id_index.contains_key(&qacc));
    }

    #[test]
    fn process_rest_data_works() {
        // Test Queries:
        let mut ap = AnnotationProcess::new();
        // let mut nq1 = Query::from_qacc("Soltu.DM.02G015700.1".to_string());

        let mut nq1 = Query::new();
        let qacc = "Soltu.DM.02G015700.1".to_string();
        let h1 = ("hit_One","sp|C0LGP4|Y3475_ARATH Probable LRR receptor-like serine/threonine-protein kinase At3g47570 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1");
        let h2 = ("hit_Two","sp|C0LGP4|Y3475_ARATH Probable LRR receptor-like serine/threonine-protein kinase At3g47570 OS=Arabidopsis thaliana OX=3702 GN=At3g47570 PE=2 SV=1");
        nq1.hits.insert(h1.0.to_string(), h1.1.to_string());
        nq1.hits.insert(h2.0.to_string(), h2.1.to_string());

        ap.insert_query(qacc.clone(), nq1).unwrap();
        ap.process_rest_data();
        // Query should have been annotated:
        assert!(ap.human_readable_descriptions.contains_key(&qacc));
        assert!(!ap.queries.contains_key(&qacc));
        // Test Families:
        ap = AnnotationProcess::new();
        let mut sf1 = SeqFamily::new();
        sf1.query_ids = vec![qacc.clone()];
        let sf_id1 = "SeqFamily1".to_string();
        ap.insert_seq_family(sf_id1.clone(), sf1).unwrap();
        nq1 = Query::new();
        ap.insert_query(qacc.clone(), nq1).unwrap();
        let mut sf2 = SeqFamily::new();
        sf2.query_ids = vec!["The protein without known relatives".to_string()];
        let sf_id2 = "SeqFamily2".to_string();
        ap.insert_seq_family(sf_id2.clone(), sf2).unwrap();
        ap.process_rest_data();
        // Families should have been annotated:
        assert!(ap.human_readable_descriptions.contains_key(&sf_id1));
        assert!(!ap.queries.contains_key(&qacc));
        assert!(!ap.seq_families.contains_key(&sf_id1));
        assert!(!ap.query_id_to_seq_family_id_index.contains_key(&qacc));
        assert!(ap.human_readable_descriptions.contains_key(&sf_id2));
        assert!(!ap.seq_families.contains_key(&sf_id2));
    }

    #[test]
    fn run_annotates_queries() {
        let mut ap = AnnotationProcess::new();
        ap.seq_sim_search_tables.push(SeqSimTable::new(
                "Twelve_Proteins_vs_Swissprot_blastp".to_string(),
                Path::new("misc")
                    .join("Twelve_Proteins_vs_Swissprot_blastp.txt")
                    .to_str()
                    .unwrap()
                    .to_string(),
        ));
        ap.seq_sim_search_tables.push(SeqSimTable::new(
                "Twelve_Proteins_vs_trembl_blastp".to_string(),
                Path::new("misc")
                    .join("Twelve_Proteins_vs_trembl_blastp.txt")
                    .to_str()
                    .unwrap()
                    .to_string(),
        ));
        ap.run().unwrap();
        let hrds = ap.human_readable_descriptions;
        assert!(!hrds.is_empty());
        let queries_with_expected_result = vec![
            "Soltu.DM.01G022510.1".to_string(),
            "Soltu.DM.01G045390.1".to_string(),
            "Soltu.DM.02G015700.1".to_string(),
            "Soltu.DM.02G020600.1".to_string(),
            "Soltu.DM.03G011280.1".to_string(),
            "Soltu.DM.03G026010.1".to_string(),
            "Soltu.DM.04G035790.1".to_string(),
            "Soltu.DM.07G016620.1".to_string(),
            "Soltu.DM.09G022410.3".to_string(),
            "Soltu.DM.10G003150.1".to_string(),
            "Soltu.DM.S001650.1".to_string(),
        ];
        for qid in queries_with_expected_result {
            assert!(hrds.contains_key(&qid))
        }
        for (_, v) in hrds {
            assert!(!v.description.is_empty());
        }
    }

    #[test]
    fn run_annotates_families() {
        let mut ap = AnnotationProcess::new();
        ap.seq_sim_search_tables.push(SeqSimTable::new(
                "Twelve_Proteins_vs_Swissprot_blastp".to_string(),
                Path::new("misc")
                    .join("Twelve_Proteins_vs_Swissprot_blastp.txt")
                    .to_str()
                    .unwrap()
                    .to_string(),
        ));
        ap.seq_sim_search_tables.push(SeqSimTable::new(
                "Twelve_Proteins_vs_trembl_blastp".to_string(),
                Path::new("misc")
                    .join("Twelve_Proteins_vs_trembl_blastp.txt")
                    .to_str()
                    .unwrap()
                    .to_string(),
        ));
        let mut sf1 = SeqFamily::new();
        let sf1_id = "SeqFamily1".to_string();
        sf1.query_ids = vec![
            "Soltu.DM.01G022510.1".to_string(),
            "Soltu.DM.01G045390.1".to_string(),
            "Soltu.DM.02G015700.1".to_string(),
            "Soltu.DM.02G020600.1".to_string(),
            "Soltu.DM.03G011280.1".to_string(),
            "Soltu.DM.03G026010.1".to_string(),
            "Soltu.DM.04G035790.1".to_string(),
        ];
        let mut sf2 = SeqFamily::new();
        let sf2_id = "SeqFamily2".to_string();
        sf2.query_ids = vec![
            "Soltu.DM.07G016620.1".to_string(),
            "Soltu.DM.09G022410.3".to_string(),
            "Soltu.DM.10G003150.1".to_string(),
            "Soltu.DM.S001650.1".to_string(),
            "The_Protein_Without_Blast_hits".to_string(),
        ];
        ap.insert_seq_family(sf1_id.clone(), sf1).unwrap();
        ap.insert_seq_family(sf2_id.clone(), sf2).unwrap();
        ap.run().unwrap();
        let hrds = ap.human_readable_descriptions;
        assert_eq!(hrds.len(), 2);
        let queries_with_expected_result = vec![sf1_id, sf2_id];
        for qid in queries_with_expected_result {
            assert!(hrds.contains_key(&qid))
        }
        for (_, v) in hrds {
            assert!(!v.description.is_empty());
        }
    }

    /// A description is polished where it is produced, so that what `conclude` hands back is
    /// final: the row that will be written, and the last word of any account of how it came
    /// about.
    #[test]
    fn a_concluded_description_is_polished() {
        let ap = AnnotationProcess::new();
        for chosen in [
            "polyadenylate binding protein and",
            "polyadenylate binding protein",
            "polyadenylate binding protein the",
        ] {
            assert_eq!(
                Some("polyadenylate binding protein"),
                ap.conclude(
                    "Prot1",
                    Annotee::Query,
                    Annotation {
                        description: Some(chosen.to_string()),
                        ..Default::default()
                    }
                )
                .as_ref()
                .map(|annotated| annotated.description.as_str())
            );
        }
    }

    /// What stands in for a description there is none of depends on what was being annotated, and
    /// it is polished like any other -- it went through the same final pass before.
    #[test]
    fn an_annotee_without_a_description_is_called_unknown() {
        let ap = AnnotationProcess::new();
        assert_eq!(
            Some("unknown protein"),
            ap.conclude("Prot1", Annotee::Query, Annotation::default())
                .as_ref()
                .map(|annotated| annotated.description.as_str())
        );
        assert_eq!(
            Some("unknown sequence family"),
            ap.conclude("Fam1", Annotee::Family, Annotation::default())
                .as_ref()
                .map(|annotated| annotated.description.as_str())
        );

        // Unless the user asked for those rows to be left out altogether:
        let mut ap = AnnotationProcess::new();
        ap.exclude_not_annotated_from_output = true;
        assert_eq!(None, ap.conclude("Prot1", Annotee::Query, Annotation::default()));
    }

    #[test]
    fn parses_seq_families_file() {
        let mut ap = AnnotationProcess::new();
        let p = Path::new("misc")
            .join("test_gene_families.txt")
            .to_str()
            .unwrap()
            .to_string();
        ap.parse_seq_families_file(&p).unwrap();
        assert_eq!(ap.seq_families.len(), 6)
    }
}
