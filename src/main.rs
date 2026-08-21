#[macro_use]
extern crate lazy_static;

use annotation_process::{run as run_annotation_process, AnnotationProcess};

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

use cli::{ArgMatches, get_command};

/// The famous `main` - entry point of `prot-scriber`. It parses the command line arguments, starts
/// the `prot-scriber` annotation process and writes the results into the respective output file.
fn main() {
    let matches: ArgMatches = get_command().get_matches();
    run(matches);
}

fn run(matches: ArgMatches) {
    let out_filename = matches.value_of("output").expect("'output' must be a mandatory argument").to_string();

    // Create a new AnnotationProcess instance and provide it with the necessary input data:
    let mut annotation_process = AnnotationProcess::from(&matches);

    // Set the number of parallel processes to be used by `rayon` (see
    // `AnnotationProcess::process_rest_data`).
    // As rayon will init this automatically once e.g. par_iter is being called, this manual setup won't be done for tests.
    #[cfg(not(test))]
    rayon::ThreadPoolBuilder::new()
        .num_threads(annotation_process.n_threads)
        .build_global()
        .expect("Could not set the number of parallel processes to be used to generate human readable descriptions (AnnotationProcess::process_rest_data).");

    // Execute the Annotation-Process:
    annotation_process = run_annotation_process(annotation_process);

    // Save output:
    match output_writer::write_output_table(
        out_filename.clone(),
        annotation_process.human_readable_descriptions,
    ) {
        Ok(()) => {
            if annotation_process.verbose {
                println!("output written to file {:?}.", out_filename);
            }
        }
        Err(e) => eprintln!(
            "We are sorry, an error occurred when attempting to write output to file {:?} \n{:?}",
            out_filename, e
        ),
    };
}


#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    use std::fs::File;
    use std::io::{BufRead, BufReader};

    fn read_and_sort(path: &str) -> std::io::Result<Vec<String>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut lines: Vec<String> = reader
            .lines()
            .collect::<Result<_, _>>()?;

        lines.sort(); // ascending lexicographic sort

        Ok(lines)
    }

    #[test]
    fn test_annotate_biological_sequences() {
        const OUT_FILE: &str = "misc/tmp_Twelve_Proteins_HRDs.test";
        let matches: ArgMatches = get_command().get_matches_from(["prot-scriber", "-s", "misc/Twelve_Proteins_vs_Swissprot_blastp.txt", "-s", "misc/Twelve_Proteins_vs_trembl_blastp.txt", "--output", OUT_FILE]);
        run(matches);

        // created with prot-scriber from Commit b89cb7574cd06db26d30d9107f26b808887a30f6
        const EXPECTED_FILE: &str = "misc/Twelve_Proteins_HRDs.txt";

        let expected_content: Vec<String> = read_and_sort(EXPECTED_FILE).unwrap();
        let result_content: Vec<String> = read_and_sort(OUT_FILE).unwrap();

        assert_eq!(result_content, expected_content);
        assert!(std::fs::remove_file(OUT_FILE).is_ok(), "Could not remove test file '{file}'", file = OUT_FILE);
    }

    #[test]
    fn test_annotate_gene_families() {
        const OUT_FILE: &str = "misc/tmp_family_HRDs.test";
        let matches: ArgMatches = get_command().get_matches_from(["prot-scriber", "-s", "misc/Twelve_Proteins_vs_Swissprot_blastp.txt", "-s", "misc/Twelve_Proteins_vs_trembl_blastp.txt", "-f", "misc/families.txt", "--output", OUT_FILE]);
        run(matches);

        // created with prot-scriber from Commit b89cb7574cd06db26d30d9107f26b808887a30f6
        const EXPECTED_FILE: &str = "misc/family_HRDs.txt";

        let expected_content: Vec<String> = read_and_sort(EXPECTED_FILE).unwrap();
        let result_content: Vec<String> = read_and_sort(OUT_FILE).unwrap();

        assert_eq!(result_content, expected_content);
        assert!(std::fs::remove_file(OUT_FILE).is_ok(), "Could not remove test file '{file}'", file = OUT_FILE);
    }
}