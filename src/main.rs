#[macro_use]
extern crate lazy_static;

use annotation_process::AnnotationProcess;

/// Declare modules:
mod annotation_process;
mod cli;
mod default;
mod description;
mod hrd;
mod input;
mod model;
mod output_writer;
mod stats;

use cli::{Args, Parser};
use std::process::ExitCode;

/// The exit status of a run whose output could not be written: `EX_IOERR` of `sysexits(3)`, the
/// I/O code of prot-scriber's exit status taxonomy.
const EXIT_IO_ERROR: u8 = 74;

/// The famous `main` - entry point of `prot-scriber`. It parses the command line arguments, starts
/// the `prot-scriber` annotation process and writes the results into the respective output file.
///
/// It returns an `ExitCode` rather than exiting from inside the run, because the exit status is
/// the one report every caller reads: a shell's `&&`, a `Makefile` rule, a workflow step and a
/// scheduler all decide what happens next by it. Whatever prot-scriber could not do has to arrive
/// there. Failures are diagnosed where they occur -- that is where the file name and the rest of
/// the context are -- and reach `main` as an error to be classified.
fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(EXIT_IO_ERROR),
    }
}

/// Runs one complete annotation process and stores its result. Returns the error that prevented
/// the output from being written, if any; `main` turns it into an exit status.
///
/// # Arguments
///
/// * `args` - The parsed command line arguments.
fn run(args: Args) -> std::io::Result<()> {
    let out_filename = args.output.clone();

    // Create a new AnnotationProcess instance and provide it with the necessary input data:
    let mut annotation_process = AnnotationProcess::from(&args);

    // Set the number of parallel processes to be used by `rayon` (see
    // `AnnotationProcess::process_rest_data`).
    // As rayon will init this automatically once e.g. par_iter is being called, this manual setup won't be done for tests.
    #[cfg(not(test))]
    rayon::ThreadPoolBuilder::new()
        .num_threads(annotation_process.n_threads)
        .build_global()
        .expect("Could not set the number of parallel processes to be used to generate human readable descriptions (AnnotationProcess::process_rest_data).");

    // Execute the Annotation-Process:
    annotation_process.run();

    // Save output:
    match output_writer::write_output_table(
        out_filename.clone(),
        annotation_process.human_readable_descriptions,
    ) {
        Ok(()) => {
            if annotation_process.verbose {
                println!("output written to file {:?}.", out_filename);
            }
            Ok(())
        }
        Err(e) => {
            eprintln!(
                "We are sorry, an error occurred when attempting to write output to file {:?} \n{:?}",
                out_filename, e
            );
            Err(e)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    use std::fs::read_to_string;

    #[test]
    fn test_annotate_biological_sequences() {
        const OUT_FILE: &str = "misc/tmp_Twelve_Proteins_HRDs.test";
        run(Args::parse_from(["prot-scriber", "-s", "misc/Twelve_Proteins_vs_Swissprot_blastp.txt", "-s", "misc/Twelve_Proteins_vs_trembl_blastp.txt", "--output", OUT_FILE])).expect("could not write the output table");

        // created with prot-scriber from Commit b89cb7574cd06db26d30d9107f26b808887a30f6
        const EXPECTED_FILE: &str = "misc/Twelve_Proteins_HRDs.txt";

        let expected_content = read_to_string(EXPECTED_FILE).unwrap();
        let result_content = read_to_string(OUT_FILE).unwrap();

        assert_eq!(result_content, expected_content);
        assert!(std::fs::remove_file(OUT_FILE).is_ok(), "Could not remove test file '{file}'", file = OUT_FILE);
    }

    #[test]
    fn test_annotate_gene_families() {
        const OUT_FILE: &str = "misc/tmp_family_HRDs.test";
        run(Args::parse_from(["prot-scriber", "-s", "misc/Twelve_Proteins_vs_Swissprot_blastp.txt", "-s", "misc/Twelve_Proteins_vs_trembl_blastp.txt", "-f", "misc/families.txt", "--output", OUT_FILE])).expect("could not write the output table");

        // created with prot-scriber from Commit b89cb7574cd06db26d30d9107f26b808887a30f6
        const EXPECTED_FILE: &str = "misc/family_HRDs.txt";

        let expected_content = read_to_string(EXPECTED_FILE).unwrap();
        let result_content = read_to_string(OUT_FILE).unwrap();

        assert_eq!(result_content, expected_content);
        assert!(std::fs::remove_file(OUT_FILE).is_ok(), "Could not remove test file '{file}'", file = OUT_FILE);
    }
}