#[macro_use]
extern crate lazy_static;

use annotation_process::AnnotationProcess;

/// Declare modules:
mod annotation_process;
mod assets;
mod cli;
mod corpus;
mod default;
mod description;
mod error;
mod explain;
mod hrd;
mod input;
mod model;
mod output_writer;
mod plan;
mod stats;
mod trace;
#[cfg(test)]
mod test_support;

use cli::{Args, Cli, Command, CorpusCommand, DefaultList, Parser, ValueEnum};
use error::{Error, EXIT_INTERNAL_ERROR, ISSUES_URL};
// `TryFrom` is in the prelude only from edition 2021 on, and this crate is on edition 2018:
use std::convert::TryFrom;
use std::io::{self, Write};
use std::process::ExitCode;

/// The famous `main` - entry point of `prot-scriber`. It parses the command line arguments, starts
/// the `prot-scriber` annotation process and writes the results into the respective output file.
///
/// It returns an `ExitCode` rather than exiting from inside the run, because the exit status is
/// the one report every caller reads: a shell's `&&`, a `Makefile` rule, a workflow step and a
/// scheduler all decide what happens next by it. Whatever prot-scriber could not do has to arrive
/// there. Failures are diagnosed where they occur -- that is where the file name and the rest of
/// the context are -- and reach `main` as an error to be classified.
fn main() -> ExitCode {
    report_panics_as_bugs();
    restore_default_sigpipe();
    match dispatch(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            report(&e);
            ExitCode::from(e.exit_code())
        }
    }
}

/// Writes an error to standard error, labelled and styled as `clap` labels and styles its own.
///
/// A usage error `clap` catches and a usage error prot-scriber raises are the same thing to the
/// person reading them, and they used to look different: clap wrote a bold red `error:` and this
/// wrote the bare sentence. Which of the two layers happened to catch a mistake is not something
/// the user knows or should be able to tell.
///
/// The style is taken from `clap`'s own default rather than restated, and the writing goes through
/// `anstream`, which is what clap writes through -- so terminal detection, `NO_COLOR` and
/// `CLICOLOR_FORCE` are decided once, for both halves of prot-scriber's error output, instead of by
/// a second implementation that could drift from the first.
///
/// # Arguments
///
/// * `error` - What could not be done.
fn report(error: &Error) {
    let styles = clap::builder::Styles::default();
    let label = styles.get_error();
    // The messages carry their own blank lines, which read well after a bare sentence and not at
    // all after a label. The label takes the place of the leading ones:
    let message = format!("{}", error);
    anstream::eprintln!(
        "\n{label}error:{label:#} {}\n",
        message.trim(),
        label = label
    );
}

/// Restores the default disposition of `SIGPIPE`, which the Rust runtime ignores before `main`.
///
/// With `SIGPIPE` ignored, a write to a pipe whose reader has gone away comes back as `EPIPE`
/// instead of ending the process -- so `prot-scriber ... -o - | head` would report a failed write
/// and exit 74, blaming the user for the perfectly ordinary act of looking at the first few rows.
/// Every unix tool at the left of such a pipe dies silently there, and now this one does too.
///
/// # Safety
///
/// `signal` is called with `SIG_DFL`, before any thread has been started, so no other thread can
/// observe the disposition while it is being changed.
#[cfg(unix)]
fn restore_default_sigpipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

/// Nothing to restore: `SIGPIPE` is a unix signal, and a broken pipe is reported as an ordinary
/// write failure elsewhere.
#[cfg(not(unix))]
fn restore_default_sigpipe() {}

/// Replaces the standard panic hook, so that a panic says what a panic now means.
///
/// Everything a user can get wrong is a typed error, reported and given an exit status of its own.
/// What is left for a panic to be is a violated invariant, i.e. a bug -- and the standard hook
/// answers that with a source location and an invitation to set `RUST_BACKTRACE`, which asks the
/// user to debug prot-scriber. This one asks them to report it instead, and keeps the location,
/// which is the useful half of a bug report.
///
/// The hook ends the process itself rather than letting the panic unwind, because a panic on one
/// of the parsing or annotation threads would otherwise kill only that thread: the run would carry
/// on without the data that thread was producing and report success at the end of it.
fn report_panics_as_bugs() {
    std::panic::set_hook(Box::new(|panic_info| {
        // Labelled and styled as every other error is, so that a bug does not announce itself in a
        // different voice from a mistake in the command line:
        let label = clap::builder::Styles::default();
        let label = label.get_error();
        anstream::eprintln!(
            "\n{label}error:{label:#} prot-scriber stopped, because of an internal error:\n{}",
            panic_info,
            label = label
        );
        anstream::eprintln!(
            "This is a bug in prot-scriber, please report it, together with the command line you ran, at\n{}\n",
            ISSUES_URL
        );
        std::process::exit(i32::from(EXIT_INTERNAL_ERROR));
    }));
}

/// Carries out whatever the command line asked for.
///
/// Annotating is what prot-scriber is for, so it is what a command line with no verb at all
/// means -- the form every published methods section and every pipeline uses.
///
/// # Arguments
///
/// * `cli` - The parsed command line.
fn dispatch(cli: Cli) -> Result<(), Error> {
    match cli.command {
        Some(Command::Annotate(args)) => run(*args),
        Some(Command::Defaults { name }) => print_defaults(name),
        Some(Command::Explain(what)) => explain::explain_stitles(&what),
        Some(Command::Corpus(CorpusCommand::Build(what))) => corpus::build::build(&what),
        Some(Command::Corpus(CorpusCommand::Merge(what))) => corpus::build::merge(&what),
        Some(Command::Corpus(CorpusCommand::Diff(what))) => corpus::build::diff(&what),
        Some(Command::Corpus(CorpusCommand::Show(what))) => corpus::build::show(&what),
        None => run(
            cli.annotate
                .expect("with no verb given, clap has required the arguments of an annotation run"),
        ),
    }
}

/// Writes one of the built-in regular expression lists to standard output, or -- given no name --
/// a table of what there is.
///
/// The lists go to standard output because they are data: `prot-scriber defaults filter-regexs-uniprot >
/// my_filters.txt` is the first step of changing how descriptions are processed, and piping the
/// same command through `diff -` is how you find out whether a file you already have has fallen
/// behind. Neither needs the network, and neither can hand back a list other than the one this
/// binary applies.
///
/// # Arguments
///
/// * `name` - Which list to print, or `None` to list them.
fn print_defaults(name: Option<DefaultList>) -> Result<(), Error> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match name {
        Some(list) => write!(out, "{}", list.content()),
        None => {
            let mut result = writeln!(
                out,
                "prot-scriber's built-in regular expression lists. Print one with\n\n    prot-scriber defaults <NAME>\n"
            );
            let width = DefaultList::value_variants()
                .iter()
                .map(|l| l.name().len())
                .max()
                .unwrap_or(0);
            for list in DefaultList::value_variants() {
                let (option, what) = list.what();
                result = result.and_then(|()| {
                    writeln!(out, "    {:width$}  {}\n    {:width$}  {}", list.name(), option, "", what, width = width)
                });
            }
            result
        }
    }
    // The table has to reach the reader, and a full disk behind a redirection must not look like
    // success; a `File` would be flushed on drop and report nothing:
    .and_then(|()| out.flush())
    .map_err(|e| Error::Io(format!("\n\nCould not write the built-in list: {}\n\n", e)))
}

/// Writes what a run would do, without doing any of it.
///
/// Everything that can be known before the input is read has been established by the time this is
/// called: that every argument could be paired with the table it is for, that every file of
/// regular expressions exists and parses, that a header names the columns prot-scriber needs. What
/// is added here is the input tables themselves -- that they exist, and how large they are -- and
/// a statement of the settings each one would be parsed with, so that a mistake costs a second
/// rather than an hour of a scheduler's time.
///
/// It goes to standard output, because for this run it *is* the output.
///
/// # Arguments
///
/// * `args` - The parsed command line, for what it asks for beyond the annotation itself.
/// * `annotation_process` - The annotation process the command line resolved to.
/// * `output` - Where the output table would go.
/// * `families_path` - The gene families file, if there is one.
fn report_dry_run(
    args: &Args,
    annotation_process: &AnnotationProcess,
    output: &str,
    families_path: Option<&String>,
) -> Result<(), Error> {
    // Built in full before any of it is written. A dry run that stops on a missing input table
    // would otherwise have left half a report on standard output, and a run that fails writes
    // nothing there.
    let mut report = String::new();
    let mut write = |line: String| -> Result<(), Error> {
        report.push_str(&line);
        report.push('\n');
        Ok(())
    };

    write(format!(
        "prot-scriber {} -- dry run, nothing was read and nothing was written\n",
        env!("CARGO_PKG_VERSION")
    ))?;
    write(format!(
        "mode:    {}",
        match annotation_process.mode() {
            annotation_process::AnnotationProcessMode::SequenceAnnotation =>
                "annotate query sequences",
            annotation_process::AnnotationProcessMode::FamilyAnnotation =>
                "annotate sequence families",
        }
    ))?;
    if let Some(families) = families_path {
        write(format!(
            "         families from {:?}{}",
            families,
            if annotation_process.annotate_lonely_queries {
                ", and queries belonging to none of them"
            } else {
                ", and queries belonging to none of them are left out"
            }
        ))?;
    }
    write(format!(
        "output:  {}",
        if output == output_writer::STDOUT_PATH {
            String::from("standard output")
        } else {
            format!("{:?}", output)
        }
    ))?;
    if !args.explain.is_empty() {
        write(format!(
            "explain: {}, to {}",
            args.explain
                .iter()
                .map(|annotee| format!("{:?}", annotee))
                .collect::<Vec<String>>()
                .join(", "),
            match args.explain_out.as_deref() {
                Some(path) => format!("{:?}", path),
                None => String::from("standard output"),
            }
        ))?;
    }
    write(format!("threads: {}", annotation_process.n_threads))?;
    if annotation_process.buffer_unsorted_input {
        write(String::from(
            "         input is held until it has all been read (--unsorted-input)",
        ))?;
    }

    write(String::from("\ninput tables:"))?;
    for table in &annotation_process.seq_sim_search_tables {
        // The one thing a resolved command line cannot tell us. A table that is not there is worth
        // hearing about now rather than from a parsing thread an hour into a run:
        let size = std::fs::metadata(&table.path)
            .map_err(|e| Error::opening(&table.path, format!("No such file {:?}", table.path), &e))?
            .len();
        write(format!(
            "  {} = {:?} ({} bytes)",
            table.name, table.path, size
        ))?;
        write(format!(
            "      separator {}, columns qacc={} sacc={} stitle={}",
            match table.field_separator {
                '\t' => String::from("TAB"),
                ' ' => String::from("SPACE"),
                other => format!("{:?}", other),
            },
            table.qacc_col,
            table.sacc_col,
            table.stitle_col
        ))?;
        for (what, count, is_default) in [
            (
                "blacklist",
                table.blacklist_regexs.len(),
                same_regexs(&table.blacklist_regexs, &default::BLACKLIST_STITLE_REGEXS),
            ),
            (
                "filter",
                table.filter_regexs.len(),
                same_regexs(&table.filter_regexs, &default::FILTER_REGEXS),
            ),
        ] {
            write(format!(
                "      {:<10} {} expressions{}",
                what,
                count,
                if is_default { ", the default" } else { "" }
            ))?;
        }
        write(format!(
            "      {:<10} {} {}{}",
            "rewrite",
            table.capture_replace_pairs.len(),
            if table.capture_replace_pairs.len() == 1 { "pair" } else { "pairs" },
            if table
                .capture_replace_pairs
                .iter()
                .map(|(r, s)| (r.as_str(), s.as_str()))
                .eq(default::CAPTURE_REPLACE_DESCRIPTION_PAIRS
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str())))
            {
                ", the default"
            } else {
                ""
            }
        ))?;
    }

    write(String::from("\nscoring:"))?;
    write(format!(
        "  split words on   {}",
        annotation_process.description_split_regex.as_str()
    ))?;
    write(format!(
        "  non-informative  {} expressions{}",
        annotation_process.non_informative_words_regexs.len(),
        if same_regexs(
            &annotation_process.non_informative_words_regexs,
            &default::NON_INFORMATIVE_WORDS_REGEXS
        ) {
            ", the default"
        } else {
            ""
        }
    ))?;
    write(format!(
        "  centre scores at {}",
        if annotation_process.center_iic_at_quantile == 50.0 {
            String::from("the mean")
        } else {
            format!("quantile {}", annotation_process.center_iic_at_quantile)
        }
    ))?;
    write(format!(
        "  polish with      {} {}",
        annotation_process.polish_capture_replace_pairs.len(),
        if annotation_process.polish_capture_replace_pairs.len() == 1 { "pair" } else { "pairs" }
    ))?;

    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(report.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| Error::Io(format!("\n\nCould not write the dry run report: {}\n\n", e)))
}

/// Whether two lists of regular expressions are the same list, which is how the dry run report
/// says that a setting is prot-scriber's own rather than the user's.
///
/// # Arguments
///
/// * `given` - The list the run would use.
/// * `default` - The compiled in default to compare it against.
fn same_regexs(given: &[regex::Regex], default: &[regex::Regex]) -> bool {
    given.len() == default.len()
        && given
            .iter()
            .zip(default.iter())
            .all(|(a, b)| a.as_str() == b.as_str())
}

/// Writes the record of what this run resolved to, unless the user asked for none.
///
/// Written after the annotation because the hashes of the input tables are taken as they are read,
/// and written before the output table is reported so that a plan and its result appear together
/// or not at all.
///
/// # Arguments
///
/// * `args` - The parsed command line.
/// * `annotation_process` - The process that ran.
/// * `tables` - The input tables it was given.
fn prepare_run_plan(
    args: &Args,
    annotation_process: &AnnotationProcess,
    tables: &[input::seq_sim_table::SeqSimTable],
    output: &str,
    families_path: Option<&String>,
) -> Result<Option<(String, String)>, Error> {
    // A replay has a plan already -- the one it was given -- and writing it back would be at best
    // a no-op and at worst an overwrite of the record being replayed.
    if args.plan.is_some() {
        return Ok(None);
    }
    let path = match args.plan_out.as_deref() {
        Some("none") => return Ok(None),
        Some(path) => path.to_string(),
        // A table on standard output has no file name to hang a plan on, and inventing one in the
        // working directory would be a surprise. Ask for it by name instead.
        None if output == output_writer::STDOUT_PATH => return Ok(None),
        None => format!("{}.plan.toml", output),
    };
    let plan = plan::Plan::of(annotation_process, tables, output, families_path);
    Ok(Some((path, plan.to_toml()?)))
}

/// Runs one complete annotation process and stores its result. Returns the error that prevented
/// the result from being produced or written, if any; `main` turns it into a diagnostic and an
/// exit status.
///
/// # Arguments
///
/// * `args` - The parsed command line arguments.
fn run(args: Args) -> Result<(), Error> {
    // Either the command line describes the run, or a plan does. Never both: --plan conflicts with
    // every option that would configure anything, so there is no precedence rule to know.
    let (mut annotation_process, out_filename, families_path) = match &args.plan {
        Some(path) => {
            let mut recorded = plan::Plan::from_toml(&plan::read(path)?, path)?;
            recorded.interpolate(
                &args
                    .var
                    .iter()
                    .map(|variable| (variable.name.clone(), variable.value.clone()))
                    .collect::<Vec<(String, String)>>(),
            )?;
            let output = recorded.run.output.clone();
            let families = recorded.families.as_ref().map(|f| f.path.clone());
            (AnnotationProcess::try_from(&recorded)?, output, families)
        }
        None => (
            AnnotationProcess::try_from(&args)?,
            args.output
                .clone()
                .expect("without --plan, clap has required --output"),
            args.seq_families.clone(),
        ),
    };

    // Nothing is read and nothing is written: the command line has been resolved and checked by
    // now, which is what a dry run is for.
    if args.dry_run {
        return report_dry_run(&args, &annotation_process, &out_filename, families_path.as_ref());
    }

    // Where an account of each description goes, if one was asked for. Opened before the run, so
    // that a path that cannot be written is reported now rather than after the annotation:
    annotation_process.traces = trace::sinks(
        &args.explain,
        args.explain_out.as_deref(),
        args.format,
        &out_filename,
    )?;

    // Set the number of parallel processes to be used by `rayon` (see
    // `AnnotationProcess::process_rest_data`).
    // As rayon will init this automatically once e.g. par_iter is being called, this manual setup won't be done for tests.
    // A thread count the system will not give is the user's number meeting the user's environment,
    // not a defect: RLIMIT_NPROC of a few hundred is ordinary in containers and on shared login
    // nodes, and the default here is the core count, so a large machine can reach it with no
    // argument at all. Reporting that through the panic hook would tell them to file an issue
    // about their own ulimit and bury the one thing they can act on.
    #[cfg(not(test))]
    if let Err(e) = rayon::ThreadPoolBuilder::new()
        .num_threads(annotation_process.n_threads)
        .build_global()
    {
        return Err(Error::Usage(format!(
            "\n\nCannot run Annotation-Process, because the {} parallel threads asked for by --n-threads (-n) could not be started: {}. This is usually a per-user process limit -- see 'ulimit -u' -- rather than a shortage of memory or cores. Please ask for fewer threads; more of them than the machine has cores cannot speed up the annotation anyway.\n\n",
            annotation_process.n_threads, e
        )));
    }

    // The tables are needed for the plan, and the process gives them up while it runs:
    let tables = annotation_process.seq_sim_search_tables.clone();

    // Execute the Annotation-Process:
    annotation_process.run()?;


    // Rendered before the table is written, because writing it moves the descriptions out of the
    // process, and written afterwards, because a plan beside no table would describe a run whose
    // result never landed:
    let recorded = prepare_run_plan(
        &args,
        &annotation_process,
        &tables,
        &out_filename,
        families_path.as_ref(),
    )?;

    // Save output, and only then the record of how it was made: a plan beside no table would
    // describe a run whose result never landed. In the `jsonl` format there is nothing left to
    // save: each row was written as the annotation that produced it finished, which is the whole
    // point of that format.
    let written = if args.format == cli::OutputFormat::Jsonl {
        Ok(())
    } else {
        output_writer::write_output_table(
            out_filename.clone(),
            args.format,
            annotation_process.human_readable_descriptions,
        )
    };
    match written {
        Ok(()) => {
            if let Some((path, toml)) = &recorded {
                std::fs::write(path, toml).map_err(|e| {
                    Error::Io(format!(
                        "\n\nCould not write the run plan {:?}: {}\n\n",
                        path, e
                    ))
                })?;
                if annotation_process.verbose {
                    eprintln!("run plan written to file {:?}.", path);
                }
            }
            if annotation_process.verbose {
                // "written to file '-'" would be a lie about where the table went, and the one
                // place it must not be told is the stream the table is not on:
                if out_filename == output_writer::STDOUT_PATH {
                    eprintln!("output written to standard output.");
                } else {
                    eprintln!("output written to file {:?}.", out_filename);
                }
            }
            Ok(())
        }
        Err(e) => Err(Error::Io(format!(
            "We are sorry, an error occurred when attempting to write output to file {:?} \n{:?}",
            out_filename, e
        ))),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    use std::fs::read_to_string;

    #[test]
    fn test_annotate_biological_sequences() {
        let out_path = test_support::scratch_file("Twelve_Proteins_HRDs.test");
        let out_file = out_path.to_string_lossy().into_owned();
        let out_file = out_file.as_str();
        dispatch(Cli::parse_from(["prot-scriber", "-s", "misc/Twelve_Proteins_vs_Swissprot_blastp.txt", "-s", "misc/Twelve_Proteins_vs_trembl_blastp.txt", "--output", out_file])).expect("could not write the output table");

        // created with prot-scriber from Commit b89cb7574cd06db26d30d9107f26b808887a30f6
        const EXPECTED_FILE: &str = "misc/Twelve_Proteins_HRDs.txt";

        let expected_content = read_to_string(EXPECTED_FILE).unwrap();
        let result_content = read_to_string(out_file).unwrap();

        assert_eq!(result_content, expected_content);
        assert!(std::fs::remove_file(out_file).is_ok(), "Could not remove test file '{file}'", file = out_file);
    }

    #[test]
    fn test_annotate_gene_families() {
        let out_path = test_support::scratch_file("family_HRDs.test");
        let out_file = out_path.to_string_lossy().into_owned();
        let out_file = out_file.as_str();
        dispatch(Cli::parse_from(["prot-scriber", "-s", "misc/Twelve_Proteins_vs_Swissprot_blastp.txt", "-s", "misc/Twelve_Proteins_vs_trembl_blastp.txt", "-f", "misc/families.txt", "--output", out_file])).expect("could not write the output table");

        // created with prot-scriber from Commit b89cb7574cd06db26d30d9107f26b808887a30f6
        const EXPECTED_FILE: &str = "misc/family_HRDs.txt";

        let expected_content = read_to_string(EXPECTED_FILE).unwrap();
        let result_content = read_to_string(out_file).unwrap();

        assert_eq!(result_content, expected_content);
        assert!(std::fs::remove_file(out_file).is_ok(), "Could not remove test file '{file}'", file = out_file);
    }
}