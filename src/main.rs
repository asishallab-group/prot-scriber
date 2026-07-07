#[macro_use]
extern crate lazy_static;

use annotation_process::{run, AnnotationProcess};

/// Declare modules:
mod annotation_process;
mod output_writer;
mod default;
mod cli;

/// The famous `main` - entry point of `prot-scriber`. It parses the command line arguments, starts
/// the `prot-scriber` annotation process and writes the results into the respective output file.
fn main() {
    let matches: cli::ArgMatches = cli::get_command();
    let out_filename = matches.value_of("output").unwrap().to_string();

    // Create a new AnnotationProcess instance and provide it with the necessary input data:
    let mut annotation_process = AnnotationProcess::from(matches);

    // Set the number of parallel processes to be used by `rayon` (see
    // `AnnotationProcess::process_rest_data`):
    rayon::ThreadPoolBuilder::new()
        .num_threads(annotation_process.n_threads)
        .build_global()
        .expect("Could not set the number of parallel processes to be used to generate human readable descriptions (AnnotationProcess::process_rest_data).");

    // Execute the Annotation-Process:
    annotation_process = run(annotation_process);

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
