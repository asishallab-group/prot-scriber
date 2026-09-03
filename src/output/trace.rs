//! Writing out how a human readable description came to be chosen.
//!
//! Everything here is about one annotee at a time, and is written the moment that annotee is
//! finished. That is not a stylistic choice: a trace is very much larger than the description it
//! explains -- it holds every hit description that was scored -- so a run that collected traces
//! and reported them at the end would need memory in proportion to its whole input, where
//! prot-scriber needs memory in proportion to one query. What makes that possible is that a
//! description is final the moment it is made (see `AnnotationProcess::conclude`), so there is
//! nothing to wait for.
//!
//! The consequence is that traces arrive in the order the annotations happened, which is not the
//! order of the output table: that is sorted by identifier, and sorting is the thing that needs
//! all of the rows at once. Each trace stands on its own, so `sort` after the fact is available to
//! anyone who wants a byte-stable file.

use crate::annotation_process::annotee::Annotee;
use crate::error::Error;
use crate::hrd::{Annotation, Phrase};
use crate::output::table::OutputFormat;
use serde::Serialize;
use std::collections::HashSet;
use std::fmt::Write as FmtWrite;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::sync::{Arc, Mutex};

/// How the account of an annotation is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceFormat {
    /// For a person to read.
    Text,
    /// One JSON object per line, for a program to read.
    Jsonl,
}

/// Where an account of an annotation is written to, and what has become of the writing so far.
struct Target {
    /// The stream itself.
    out: Box<dyn Write + Send>,
    /// The first failure to write, kept rather than acted on: the annotation is happening on
    /// several threads and inside `rayon`'s iterators, where there is nothing sensible to return
    /// an error to. `TraceSink::finish` reports it once the run is over, which is early enough --
    /// nothing is lost by finishing an annotation whose account of itself could not be written.
    failure: Option<io::Error>,
    /// The annotees written so far, but only for a sink that was asked for particular ones: it is
    /// how `finish` can say that an identifier the user asked about was never annotated. A sink
    /// that writes everything counts instead of remembering.
    written: HashSet<String>,
    /// How many accounts have been written.
    count: usize,
}

/// One destination for accounts of annotations, and which annotations it wants.
///
/// Cloneable and shared, because the annotation of what is left when all input has been read
/// happens on `rayon`'s thread pool: the destination is one stream whichever thread reaches it.
#[derive(Clone)]
pub struct TraceSink {
    /// What to call this destination in a diagnostic about it.
    name: String,
    /// How to write.
    format: TraceFormat,
    /// The annotees to write about, or `None` for all of them.
    only: Option<Arc<HashSet<String>>>,
    /// The stream, shared between the threads that annotate.
    target: Arc<Mutex<Target>>,
}

impl std::fmt::Debug for TraceSink {
    /// A stream has no useful `Debug`, and `AnnotationProcess` derives one.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "TraceSink({:?}, {:?}, {})",
            self.name,
            self.format,
            match &self.only {
                Some(names) => format!("{} annotees", names.len()),
                None => String::from("every annotee"),
            }
        )
    }
}

impl TraceSink {
    /// A destination writing to standard output.
    ///
    /// # Arguments
    ///
    /// * `format` - How to write.
    /// * `only` - The annotees to write about, or `None` for all of them.
    pub fn to_stdout(format: TraceFormat, only: Option<HashSet<String>>) -> TraceSink {
        TraceSink::new(
            String::from("standard output"),
            Box::new(io::stdout()),
            format,
            only,
        )
    }

    /// A destination writing to a file, which is created or truncated now rather than at the end
    /// of the run: a path that cannot be written is a mistake worth hearing about before an hour
    /// of annotation, not after it.
    ///
    /// # Arguments
    ///
    /// * `path` - The file to write.
    /// * `format` - How to write.
    /// * `only` - The annotees to write about, or `None` for all of them.
    pub fn to_file(
        path: &str,
        format: TraceFormat,
        only: Option<HashSet<String>>,
    ) -> Result<TraceSink, Error> {
        let file = File::create(path).map_err(|e| {
            Error::Io(format!(
                "\n\nCould not write the explanation to file {:?}: {}\n\n",
                path, e
            ))
        })?;
        Ok(TraceSink::new(
            format!("file {:?}", path),
            Box::new(BufWriter::new(file)),
            format,
            only,
        ))
    }

    /// # Arguments
    ///
    /// * `name` - What to call this destination in a diagnostic.
    /// * `out` - The stream to write to.
    /// * `format` - How to write.
    /// * `only` - The annotees to write about, or `None` for all of them.
    fn new(
        name: String,
        out: Box<dyn Write + Send>,
        format: TraceFormat,
        only: Option<HashSet<String>>,
    ) -> TraceSink {
        TraceSink {
            name,
            format,
            only: only.map(Arc::new),
            target: Arc::new(Mutex::new(Target {
                out,
                failure: None,
                written: HashSet::new(),
                count: 0,
            })),
        }
    }

    /// Whether this destination wants to hear about the argument `annotee`. Asked before an
    /// account is rendered, so that a run explaining one query does not render one for every
    /// query it annotates.
    ///
    /// # Arguments
    ///
    /// * `annotee` - The identifier of the query or family just annotated.
    pub fn wants(&self, annotee: &str) -> bool {
        match &self.only {
            Some(names) => names.contains(annotee),
            None => true,
        }
    }

    /// Writes the account of one annotation, and forgets it.
    ///
    /// # Arguments
    ///
    /// * `annotee` - The identifier of the query or family that was annotated.
    /// * `kind` - Which of the two it is.
    /// * `annotation` - What the annotation consisted of.
    /// * `description` - The description that will be reported for it, polished.
    pub fn record(
        &self,
        annotee: &str,
        kind: Annotee,
        annotation: &Annotation,
        description: &str,
    ) {
        let rendered = match self.format {
            TraceFormat::Text => text(annotee, kind, annotation, description),
            TraceFormat::Jsonl => jsonl(annotee, kind, annotation, description),
        };
        let mut target = match self.target.lock() {
            Ok(target) => target,
            // Another thread panicked while holding the stream. The panic hook ends the process,
            // so this is only reachable while that is happening:
            Err(poisoned) => poisoned.into_inner(),
        };
        if target.failure.is_none() {
            if let Err(e) = target.out.write_all(rendered.as_bytes()) {
                target.failure = Some(e);
            }
        }
        target.count += 1;
        if self.only.is_some() {
            target.written.insert(annotee.to_string());
        }
    }

    /// Flushes the destination and reports whatever went wrong with it: a write that failed, or an
    /// annotee the user asked about that no run of this input could have annotated.
    ///
    /// A misspelled identifier is worth an error rather than a silence. `--explain AT1G0101.1`,
    /// one character short, otherwise produces an empty explanation of a successful run, which
    /// reads exactly like a query that could not be annotated.
    pub fn finish(&self) -> Result<(), Error> {
        let mut target = match self.target.lock() {
            Ok(target) => target,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Err(e) = target.out.flush() {
            if target.failure.is_none() {
                target.failure = Some(e);
            }
        }
        if let Some(e) = &target.failure {
            return Err(Error::Io(format!(
                "\n\nCould not write the explanation to {}: {}\n\n",
                self.name, e
            )));
        }
        if let Some(names) = &self.only {
            let mut missing: Vec<&String> = names
                .iter()
                .filter(|name| !target.written.contains(*name))
                .collect();
            if !missing.is_empty() {
                missing.sort();
                return Err(Error::Usage(format!(
                    "\n\nCannot explain {}, because nothing of that name was annotated. --explain takes the identifiers that appear in the output table: a query identifier ('qacc' in the input tables), or the name of a sequence family when --seq-families (-f) is given.\n\n",
                    missing
                        .iter()
                        .map(|name| format!("{:?}", name))
                        .collect::<Vec<String>>()
                        .join(", ")
                )));
            }
        }
        Ok(())
    }
}

/// The destinations a command line asked for, empty when it asked for none.
///
/// # Arguments
///
/// * `explain` - The annotees to explain.
/// * `explain_out` - Where to write the explanation, or `None` for standard output.
/// * `output` - Where the output table goes, so that two different things cannot be sent to
///   standard output at once.
pub fn sinks(
    explain: &[String],
    explain_out: Option<&str>,
    format: OutputFormat,
    output: &str,
) -> Result<Vec<TraceSink>, Error> {
    let mut sinks: Vec<TraceSink> = vec![];
    // In this format the account of each annotation *is* the output table, written where the
    // table would have gone:
    if format == OutputFormat::Jsonl {
        sinks.push(if output == crate::default::STREAM_PATH {
            TraceSink::to_stdout(TraceFormat::Jsonl, None)
        } else {
            TraceSink::to_file(output, TraceFormat::Jsonl, None)?
        });
    }
    if explain.is_empty() {
        return Ok(sinks);
    }
    if explain.iter().any(|annotee| annotee.trim().is_empty()) {
        return Err(Error::Usage(String::from(
            "\n\n--explain was given an empty identifier. It takes the identifiers that appear in the output table, separated by commas or by repeating the option.\n\n",
        )));
    }
    let only: HashSet<String> = explain.iter().cloned().collect();
    match explain_out {
        Some(path) => sinks.push(TraceSink::to_file(path, TraceFormat::Text, Some(only))?),
        // Both are data, and there is one standard output between them:
        None if output == crate::default::STREAM_PATH => {
            return Err(Error::Usage(String::from(
                "\n\nThe output table and the --explain output would both go to standard output, where they would be mixed into each other. Send one of them to a file: --output (-o) for the table, --explain-out for the explanation.\n\n",
            )))
        }
        None => sinks.push(TraceSink::to_stdout(TraceFormat::Text, Some(only))),
    }
    Ok(sinks)
}

/// Renders one annotation for a person to read.
///
/// # Arguments
///
/// * `annotee` - The identifier of the query or family that was annotated.
/// * `kind` - Which of the two it is.
/// * `annotation` - What the annotation consisted of.
/// * `description` - The description that will be reported for it, polished.
fn text(annotee: &str, kind: Annotee, annotation: &Annotation, description: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "== {} ({}) ==",
        annotee,
        match kind {
            Annotee::Query => "query",
            Annotee::Family => "sequence family",
        }
    );
    let _ = writeln!(out, "description  {}", description);
    match (&annotation.description, annotation.verdict()) {
        (Some(chosen), _) => {
            if chosen != description {
                let _ = writeln!(out, "             chosen as {:?}, then polished", chosen);
            }
            let _ = writeln!(out, "score        {:.4}", annotation.score);
        }
        (None, verdict) => {
            let _ = writeln!(out, "             because {}", verdict.unwrap_or("?"));
        }
    }
    let _ = writeln!(
        out,
        "chosen from  {} hit description{}, {} informative word{}, {} distinct phrase{}",
        annotation.scored.len(),
        plural(annotation.scored.len()),
        annotation.words.len(),
        plural(annotation.words.len()),
        annotation.candidates.len(),
        plural(annotation.candidates.len()),
    );

    if !annotation.candidates.is_empty() {
        let _ = writeln!(out, "\nphrases, best first");
        for (rank, phrase) in annotation.candidates.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {:>9.4}  {}{}",
                phrase.score,
                phrase.text(),
                if rank == 0 { "   <- chosen" } else { "" }
            );
        }
    }

    if !annotation.words.is_empty() {
        let _ = writeln!(out, "\nword scores, best first");
        for word in &annotation.words {
            let _ = writeln!(
                out,
                "  {:>9.4}  {:<24}  seen {} time{}",
                word.score,
                word.word,
                word.frequency as u64,
                plural(word.frequency as usize),
            );
        }
    }

    if !annotation.scored.is_empty() {
        let _ = writeln!(out, "\nhit descriptions, in the order they were scored");
        let scored_words: HashSet<&str> =
            annotation.words.iter().map(|word| word.word.as_str()).collect();
        for (i, scored) in annotation.scored.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {:>4}  {}{}",
                i + 1,
                scored.source,
                match &scored.query {
                    Some(query) => format!("  (hit of {})", query),
                    None => String::new(),
                }
            );
            let _ = writeln!(out, "        description  {}", scored.description);
            let non_informative: Vec<&String> = scored
                .words
                .iter()
                .filter(|word| !scored_words.contains(word.as_str()))
                .collect();
            if !non_informative.is_empty() {
                let _ = writeln!(
                    out,
                    "        not scored   {}",
                    non_informative
                        .iter()
                        .map(|word| (*word).clone())
                        .collect::<Vec<String>>()
                        .join(", ")
                );
            }
            match &scored.phrase {
                Some(phrase) => {
                    let _ = writeln!(
                        out,
                        "        proposes     {}  ({:.4})",
                        phrase.text(),
                        phrase.score
                    );
                }
                None => {
                    let _ = writeln!(out, "        proposes     nothing");
                }
            }
        }
    }
    out.push('\n');
    out
}

/// One annotation as a JSON object: the same account `text` renders, for a program to read.
///
/// Written out rather than derived on `crate::hrd`'s own types, so that the field names of the
/// format prot-scriber publishes are stated here and do not change when a field of an internal
/// struct is renamed.
#[derive(Serialize)]
struct JsonAnnotation<'a> {
    /// The identifier of the query or family, as it appears in the output table.
    annotee: &'a str,
    /// `"query"` or `"family"`.
    kind: &'a str,
    /// The description reported for it, polished.
    description: &'a str,
    /// The phrase it was made from, before polishing, when polishing changed it.
    chosen: Option<&'a str>,
    /// What that phrase scored, absent when there was no phrase.
    score: Option<f64>,
    /// Why there is no description, absent when there is one.
    verdict: Option<&'static str>,
    /// The distinct phrases that were proposed, best first.
    candidates: Vec<JsonPhrase<'a>>,
    /// The informative words and what each was worth, best first.
    words: Vec<JsonWord<'a>>,
    /// The hit descriptions that were scored, in the order they were scored in.
    hits: Vec<JsonHit<'a>>,
}

#[derive(Serialize)]
struct JsonPhrase<'a> {
    phrase: String,
    score: f64,
    /// The words of the phrase, for whoever would otherwise have to split it again.
    words: &'a [String],
}

#[derive(Serialize)]
struct JsonWord<'a> {
    word: &'a str,
    frequency: f64,
    score: f64,
}

#[derive(Serialize)]
struct JsonHit<'a> {
    /// The hit's accession.
    hit: &'a str,
    /// The query it was found for, when a family was annotated; absent otherwise.
    query: Option<&'a str>,
    /// The description as it was scored: filtered, rewritten and lower-cased.
    description: &'a str,
    /// The words it was split into.
    words: &'a [String],
    /// The phrase it proposed, absent when it proposed none.
    proposes: Option<JsonPhrase<'a>>,
}

/// Renders one phrase.
///
/// # Arguments
///
/// * `phrase` - The phrase to render.
fn json_phrase(phrase: &Phrase) -> JsonPhrase<'_> {
    JsonPhrase {
        phrase: phrase.text(),
        score: phrase.score,
        words: &phrase.words,
    }
}

/// Renders one annotation as a single line of JSON.
///
/// # Arguments
///
/// * `annotee` - The identifier of the query or family that was annotated.
/// * `kind` - Which of the two it is.
/// * `annotation` - What the annotation consisted of.
/// * `description` - The description that will be reported for it, polished.
fn jsonl(annotee: &str, kind: Annotee, annotation: &Annotation, description: &str) -> String {
    let rendered = JsonAnnotation {
        annotee,
        kind: match kind {
            Annotee::Query => "query",
            Annotee::Family => "family",
        },
        description,
        chosen: annotation
            .description
            .as_deref()
            .filter(|chosen| *chosen != description),
        score: annotation.description.as_ref().map(|_| annotation.score),
        verdict: annotation.verdict(),
        candidates: annotation.candidates.iter().map(json_phrase).collect(),
        words: annotation
            .words
            .iter()
            .map(|word| JsonWord {
                word: &word.word,
                frequency: word.frequency,
                score: word.score,
            })
            .collect(),
        hits: annotation
            .scored
            .iter()
            .map(|scored| JsonHit {
                hit: &scored.source,
                query: scored.query.as_deref(),
                description: &scored.description,
                words: &scored.words,
                proposes: scored.phrase.as_ref().map(json_phrase),
            })
            .collect(),
    };
    match serde_json::to_string(&rendered) {
        Ok(mut line) => {
            line.push('\n');
            line
        }
        // `Serialize` is derived over strings and finite numbers, so there is nothing here that
        // can fail to serialize; a row that somehow did must still be a row, and must still say
        // which annotee it is missing:
        Err(e) => format!(
            "{{\"annotee\":{},\"error\":{}}}\n",
            serde_json::to_string(annotee).unwrap_or_else(|_| String::from("null")),
            serde_json::to_string(&e.to_string()).unwrap_or_else(|_| String::from("null"))
        ),
    }
}

/// The plural `s`, or nothing.
///
/// # Arguments
///
/// * `n` - How many there are.
fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}
