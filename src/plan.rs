//! The record a run leaves of itself: every setting it resolved to, and a hash of every byte it
//! read, in a file that replays it.
//!
//! A command line is not a record of a run. It names files whose contents change, it says
//! `@filter-regexs-ncbi-nr` where what matters is the twenty expressions that name stood for on
//! the day, and it leaves out everything that was defaulted -- which is most of it, and which
//! moves between versions. Three years later the command line is still there and the run cannot be
//! repeated from it.
//!
//! So the plan stores the *resolved* settings, rule lists written out literally and never as the
//! name of a preset, and the BLAKE3 hash of each input table as it was actually read. It is
//! committable, it is diffable, and replaying it does not depend on what is in `assets/` or on
//! disk today.

use crate::annotation_process::{AnnotationProcess, AnnotationProcessMode};
use crate::error::Error;
use crate::input::seq_sim_table::SeqSimTable;
use serde::{Deserialize, Serialize};
// `TryFrom` is in the prelude only from edition 2021 on, and this crate is on edition 2018:
use std::convert::TryFrom;

/// Everything a run resolved to.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Plan {
    /// The prot-scriber that wrote this. A plan is a record of what a particular version did.
    pub prot_scriber_version: String,
    pub run: Run,
    pub scoring: Scoring,
    /// The input tables, in the order they were given.
    #[serde(default)]
    pub db: Vec<Db>,
    pub families: Option<Families>,
}

/// What the run was, apart from how descriptions were scored.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Run {
    /// `"sequence"` or `"family"`.
    pub mode: String,
    pub output: String,
    pub threads: usize,
    pub exclude_not_annotated: bool,
    pub unsorted_input: bool,
}

/// The settings that decide which words a description contributes, and what they are worth. Global
/// to the run, not per table.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Scoring {
    pub split_regex: String,
    /// The literal 50 means "centre at the mean" rather than at a quantile.
    pub center_at: f64,
    pub non_informative_words_regexs: Vec<String>,
    pub polish_capture_replace_pairs: Vec<(String, String)>,
}

/// One input table, and everything its descriptions were put through.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Db {
    pub name: String,
    pub path: String,
    /// The BLAKE3 hash of the bytes that were read, or absent if the plan was written without the
    /// table having been read -- which is what a dry run does.
    pub digest: Option<String>,
    pub field_separator: String,
    pub qacc_column: usize,
    pub sacc_column: usize,
    pub stitle_column: usize,
    pub blacklist_regexs: Vec<String>,
    pub filter_regexs: Vec<String>,
    pub capture_replace_pairs: Vec<(String, String)>,
}

/// The gene families file, if there was one.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Families {
    pub path: String,
    pub id_genes_separator: String,
    pub gene_ids_separator: String,
    pub annotate_non_family_queries: bool,
}

impl Plan {
    /// The plan a finished -- or resolved -- annotation process describes.
    ///
    /// # Arguments
    ///
    /// * `process` - The annotation process, after the command line has been resolved into it.
    /// * `tables` - The input tables, which the process gives up as it runs.
    /// * `output` - Where the annotation table was, or would be, written.
    /// * `families_path` - The `--seq-families` argument, if one was given.
    pub fn of(
        process: &AnnotationProcess,
        tables: &[SeqSimTable],
        output: &str,
        families_path: Option<&String>,
    ) -> Plan {
        Plan {
            prot_scriber_version: String::from(env!("CARGO_PKG_VERSION")),
            run: Run {
                mode: match process.mode() {
                    AnnotationProcessMode::SequenceAnnotation => String::from("sequence"),
                    AnnotationProcessMode::FamilyAnnotation => String::from("family"),
                },
                output: output.to_string(),
                threads: process.n_threads,
                exclude_not_annotated: process.exclude_not_annotated_from_output,
                unsorted_input: process.buffer_unsorted_input,
            },
            scoring: Scoring {
                split_regex: process.description_split_regex.as_str().to_string(),
                center_at: process.center_iic_at_quantile,
                non_informative_words_regexs: strings(&process.non_informative_words_regexs),
                polish_capture_replace_pairs: pairs(&process.polish_capture_replace_pairs),
            },
            db: tables
                .iter()
                .map(|table| Db {
                    name: table.name.clone(),
                    path: table.path.clone(),
                    digest: process.input_digests.get(&table.path).cloned(),
                    field_separator: table.field_separator.to_string(),
                    qacc_column: table.qacc_col,
                    sacc_column: table.sacc_col,
                    stitle_column: table.stitle_col,
                    blacklist_regexs: strings(&table.blacklist_regexs),
                    filter_regexs: strings(&table.filter_regexs),
                    capture_replace_pairs: pairs(&table.capture_replace_pairs),
                })
                .collect(),
            families: families_path.map(|path| Families {
                path: path.clone(),
                id_genes_separator: process.seq_family_id_genes_separator.clone(),
                gene_ids_separator: process.seq_family_gene_ids_separator.clone(),
                annotate_non_family_queries: process.annotate_lonely_queries,
            }),
        }
    }

    /// Renders this plan as the TOML that is written to disk, with a header saying what it is.
    pub fn to_toml(&self) -> Result<String, Error> {
        // A plan that will not render is a bug in prot-scriber, not something the user did: it
        // is built from settings that have already been validated.
        let body = toml::to_string_pretty(self)
            .unwrap_or_else(|e| panic!("the run plan does not render as TOML: {}", e));
        Ok(format!(
            "# What prot-scriber did, in full: every setting it resolved to and a hash of every\n\
             # byte it read. Replay it with\n\
             #\n\
             #     prot-scriber --plan <this file>\n\
             #\n\
             # The regular expressions are written out rather than named, so this does not depend\n\
             # on what a later prot-scriber calls its defaults.\n\n{}",
            body
        ))
    }

    /// Reads a plan from the TOML in `content`.
    ///
    /// # Arguments
    ///
    /// * `content` - The text of a plan file.
    /// * `source` - What to name in an error message.
    pub fn from_toml(content: &str, source: &str) -> Result<Plan, Error> {
        toml::from_str(content).map_err(|e| {
            Error::MalformedData(format!(
                "\n\nCannot read the run plan {:?}: {}\n\n",
                source, e
            ))
        })
    }
}

/// The source text of each of a list of regular expressions.
fn strings(regexs: &[regex::Regex]) -> Vec<String> {
    regexs.iter().map(|r| r.as_str().to_string()).collect()
}

/// The source text and replacement of each of a list of capture-replace pairs.
fn pairs(pairs: &[(fancy_regex::Regex, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(regex, replacement)| (regex.as_str().to_string(), replacement.clone()))
        .collect()
}

/// Reads a plan file, telling a path that is not there from one that cannot be read.
///
/// # Arguments
///
/// * `path` - The plan file to read.
pub fn read(path: &str) -> Result<String, Error> {
    std::fs::read_to_string(path)
        .map_err(|e| Error::opening(path, format!("No such run plan {:?}", path), &e))
}

impl TryFrom<&Plan> for AnnotationProcess {
    type Error = Error;

    /// Builds the annotation process a plan records. Every regular expression is taken from the
    /// plan itself, not from what this prot-scriber calls its defaults, which is what makes a
    /// replay independent of the version replaying it.
    ///
    /// # Arguments
    ///
    /// * `plan` - The recorded run.
    fn try_from(plan: &Plan) -> Result<AnnotationProcess, Error> {
        let mut process = AnnotationProcess::new();
        process.n_threads = plan.run.threads;
        process.exclude_not_annotated_from_output = plan.run.exclude_not_annotated;
        process.buffer_unsorted_input = plan.run.unsorted_input;
        process.center_iic_at_quantile = plan.scoring.center_at;
        process.description_split_regex = compile(&plan.scoring.split_regex, "split_regex")?;
        process.non_informative_words_regexs = compile_all(
            &plan.scoring.non_informative_words_regexs,
            "non_informative_words_regexs",
        )?;
        process.polish_capture_replace_pairs = compile_pairs(
            &plan.scoring.polish_capture_replace_pairs,
            "polish_capture_replace_pairs",
        )?;

        let mut tables = Vec::with_capacity(plan.db.len());
        for db in &plan.db {
            let mut table = SeqSimTable::new(db.name.clone(), db.path.clone());
            let mut separator = db.field_separator.chars();
            table.field_separator = match (separator.next(), separator.next()) {
                (Some(character), None) => character,
                _ => {
                    return Err(Error::MalformedData(format!(
                        "\n\nThe run plan gives table {:?} the field separator {:?}, which is not a single character.\n\n",
                        db.name, db.field_separator
                    )))
                }
            };
            table.qacc_col = db.qacc_column;
            table.sacc_col = db.sacc_column;
            table.stitle_col = db.stitle_column;
            table.blacklist_regexs = compile_all(&db.blacklist_regexs, "blacklist_regexs")?;
            table.filter_regexs = compile_all(&db.filter_regexs, "filter_regexs")?;
            table.capture_replace_pairs =
                compile_pairs(&db.capture_replace_pairs, "capture_replace_pairs")?;
            tables.push(table);
        }
        process.seq_sim_search_tables = tables;

        if let Some(families) = &plan.families {
            process.seq_family_id_genes_separator = families.id_genes_separator.clone();
            process.seq_family_gene_ids_separator = families.gene_ids_separator.clone();
            process.annotate_lonely_queries = families.annotate_non_family_queries;
            process.parse_seq_families_file(&families.path)?;
        }
        Ok(process)
    }
}

/// Compiles one regular expression out of a plan, saying which field it came from if it will not.
fn compile(source: &str, field: &str) -> Result<regex::Regex, Error> {
    regex::Regex::new(source).map_err(|e| {
        Error::MalformedData(format!(
            "\n\nThe run plan's {} is not a valid regular expression: {}\n\n",
            field, e
        ))
    })
}

/// The same, for a list of them.
fn compile_all(sources: &[String], field: &str) -> Result<Vec<regex::Regex>, Error> {
    sources.iter().map(|source| compile(source, field)).collect()
}

/// The same, for a list of capture-replace pairs, which use the extended fancy-regex syntax.
fn compile_pairs(
    sources: &[(String, String)],
    field: &str,
) -> Result<Vec<(fancy_regex::Regex, String)>, Error> {
    sources
        .iter()
        .map(|(source, replacement)| {
            fancy_regex::Regex::new(source)
                .map(|regex| (regex, replacement.clone()))
                .map_err(|e| {
                    Error::MalformedData(format!(
                        "\n\nThe run plan's {} holds {:?}, which is not a valid regular expression: {}\n\n",
                        field, source, e
                    ))
                })
        })
        .collect()
}

impl Plan {
    /// Fills `${NAME}` placeholders in this plan's paths from `variables`.
    ///
    /// Paths only. `${name}` is also how fancy-regex names a capture group, and the
    /// capture-replace pairs are full of it, so a substitution over the whole file would quietly
    /// rewrite the very expressions the plan exists to record faithfully.
    ///
    /// Both halves of a mismatch are errors: a placeholder nothing filled in would go on to open a
    /// file called `${sample}`, and a variable that filled nothing in is a misspelling that would
    /// otherwise do nothing and say nothing.
    ///
    /// # Arguments
    ///
    /// * `variables` - The `--var NAME=VALUE` arguments.
    pub fn interpolate(&mut self, variables: &[(String, String)]) -> Result<(), Error> {
        let mut used: Vec<&str> = Vec::new();
        let mut paths: Vec<&mut String> = vec![&mut self.run.output];
        for db in &mut self.db {
            paths.push(&mut db.path);
        }
        if let Some(families) = &mut self.families {
            paths.push(&mut families.path);
        }
        for path in paths.iter_mut() {
            for (name, value) in variables {
                let placeholder = format!("${{{}}}", name);
                if path.contains(&placeholder) {
                    **path = path.replace(&placeholder, value);
                    used.push(name);
                }
            }
        }
        // A misspelled variable before an unfilled placeholder: when both are true, the
        // misspelling is the mistake and the placeholder is only its symptom.
        if let Some((name, _)) = variables.iter().find(|(name, _)| !used.contains(&name.as_str())) {
            return Err(Error::Usage(format!(
                "\n\nCannot run the plan, because --var {}=... names a placeholder that appears in none of its paths. The paths a --var can fill in are the input tables, the gene families file and the output.\n\n",
                name
            )));
        }
        for path in paths {
            if let Some(start) = path.find("${") {
                return Err(Error::Usage(format!(
                    "\n\nCannot run the plan, because the path {:?} still holds the placeholder {:?} and no --var filled it in.\n\n",
                    path,
                    &path[start..path[start..].find('}').map_or(path.len(), |end| start + end + 1)]
                )));
            }
        }
        Ok(())
    }
}
