use crate::default::STREAM_PATH;
use std::collections::HashMap;
use std::fs::write;
use std::io::{self, Write};

/// What a run has to say about one annotee: the description it chose, and the few numbers behind
/// it, for whoever asked to see them.
///
/// One of these is kept per annotee until the run ends, because the table is sorted by identifier
/// and sorting needs all of the rows at once. That is why it holds counts and a score rather than
/// the phrases and words they were computed from: those are written out as they are produced and
/// forgotten (see `crate::trace`), and keeping them here would make the memory a run needs grow
/// with the size of its input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Annotated {
    /// The description, polished, exactly as it appears in the table.
    pub description: String,
    /// What the phrase it was made from scored. Zero when there was no phrase and the description
    /// is an "unknown protein".
    pub score: f64,
    /// How many hit descriptions it was chosen from.
    pub hits: usize,
    /// How many distinct phrases were proposed, the chosen one included.
    pub phrases: usize,
}

/// The shape of the output table.
///
/// The doc comments below reach users: `clap` prints them as the possible values of `--format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Two columns, the identifier and the description
    Tsv,
    /// The same two, and the score, the number of hit descriptions and the number of phrases
    TsvScored,
    /// One JSON object per annotee, holding the whole account of how its description was chosen
    Jsonl,
}

/// Parse human_readable_descriptions into string and save it, either to the named file or -- if
/// `file_path` is `STREAM_PATH` -- to standard output.
///
/// The rows are written sorted by annotee identifier. A `HashMap` has no order of its own and
/// yields its entries differently in every process, so without this the same analysis re-run over
/// the same inputs produces a file that differs line by line from the one before it.
///
/// The output is written whether or not there is anything to report; an empty result is the header
/// line alone. Writing nothing would leave a successful run and a failed one looking the same to
/// whatever reads the output next.
///
/// # Arguments
///
/// * `file_path: String` - The file path for saving output, or `STREAM_PATH` for standard output.
/// * `format` - Which columns to write.
/// * `human_readable_descriptions` - The generated human readable descriptions.
pub fn write_output_table(
    file_path: String,
    format: OutputFormat,
    human_readable_descriptions: HashMap<String, Annotated>,
) -> io::Result<()> {
    let output = format_output_table(format, human_readable_descriptions);
    if file_path == STREAM_PATH {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        handle.write_all(output.as_bytes())?;
        // A `File` flushes what is left in it when it is dropped and reports nothing if that
        // fails; the handle on standard output has to be flushed here, so that a full disk behind
        // a redirection is an error and not a quietly truncated table:
        handle.flush()
    } else {
        write(file_path, output)
    }
}

/// Renders the generated human readable descriptions as the tabular output table, header line
/// included, sorted by annotee identifier.
///
/// # Arguments
///
/// * `format` - Which columns to write.
/// * `human_readable_descriptions` - The generated human readable descriptions.
fn format_output_table(
    format: OutputFormat,
    human_readable_descriptions: HashMap<String, Annotated>,
) -> String {
    let mut annotations: Vec<(String, Annotated)> =
        human_readable_descriptions.into_iter().collect();
    annotations.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

    let mut output = String::from("Annotee-Identifier\tHuman-Readable-Description");
    if format == OutputFormat::TsvScored {
        output.push_str("\tScore\tHit-Descriptions\tPhrases");
    }
    // The whole table is built in memory and written in one go; nothing here streams.
    for (annotee_name, annotated) in annotations {
        output.push_str(&format!("\n{}\t{}", annotee_name, annotated.description));
        if format == OutputFormat::TsvScored {
            output.push_str(&format!(
                "\t{:.4}\t{}\t{}",
                annotated.score, annotated.hits, annotated.phrases
            ));
        }
    }
    // add trailing newline for the last annotation
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use crate::output_writer::{
        format_output_table, write_output_table, Annotated, OutputFormat, STREAM_PATH,
    };
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use crate::test_support::scratch_file;

    /// An annotee with nothing but its description, which is all the default table shows.
    ///
    /// # Arguments
    ///
    /// * `description` - What was written for it.
    fn annotated(description: &str) -> Annotated {
        Annotated {
            description: description.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn writer_test() {
        let mut human_readable_descriptions: HashMap<String, Annotated> = HashMap::new();
        human_readable_descriptions.insert(
            "Seq-Family-1".to_string(),
            annotated("alien devouring protein"),
        );
        human_readable_descriptions.insert(
            "Protein-123".to_string(),
            annotated("human devouring protein"),
        );
        let out_file = scratch_file("result.txt");
        assert!(write_output_table(
            out_file.to_string_lossy().into_owned(),
            OutputFormat::Tsv,
            human_readable_descriptions
        )
        .is_ok());
        // 'Protein-123' was inserted second but is written first:
        assert_eq!(
            std::fs::read_to_string(&out_file).unwrap(),
            "Annotee-Identifier\tHuman-Readable-Description\n\
             Protein-123\thuman devouring protein\n\
             Seq-Family-1\talien devouring protein\n"
        );
    }

    /// The scored table is the same table with the numbers behind each description added to it,
    /// so that a result can be sorted or thresholded by how well founded it is without anything
    /// having to re-derive that from the input.
    #[test]
    fn the_scored_table_states_what_a_description_was_worth() {
        let mut human_readable_descriptions: HashMap<String, Annotated> = HashMap::new();
        human_readable_descriptions.insert(
            "Protein-123".to_string(),
            Annotated {
                description: String::from("a kinase"),
                score: 1.5,
                hits: 7,
                phrases: 3,
            },
        );
        human_readable_descriptions
            .insert("Protein-9".to_string(), annotated("unknown protein"));
        assert_eq!(
            format_output_table(OutputFormat::TsvScored, human_readable_descriptions),
            "Annotee-Identifier\tHuman-Readable-Description\tScore\tHit-Descriptions\tPhrases\n\
             Protein-123\ta kinase\t1.5000\t7\t3\n\
             Protein-9\tunknown protein\t0.0000\t0\t0\n"
        );
    }

    #[test]
    fn writes_the_header_when_there_is_nothing_to_report() {
        let out_file = scratch_file("empty_result.txt");
        let _ = std::fs::remove_file(&out_file);
        assert!(write_output_table(
            out_file.to_string_lossy().into_owned(),
            OutputFormat::Tsv,
            HashMap::new()
        )
        .is_ok());
        assert_eq!(
            std::fs::read_to_string(&out_file).unwrap(),
            "Annotee-Identifier\tHuman-Readable-Description\n"
        );
    }

    #[test]
    fn a_single_dash_is_not_taken_for_a_file_name() {
        // The table itself is the same table either way; what a dash changes is only where it
        // goes. Standard output cannot be captured from inside this process, so what is asserted
        // here is that the two agree, and `tests/cli.rs` runs the binary to see where it lands.
        let mut human_readable_descriptions: HashMap<String, Annotated> = HashMap::new();
        human_readable_descriptions.insert("Protein-123".to_string(), annotated("a kinase"));
        assert!(write_output_table(
            STREAM_PATH.to_string(),
            OutputFormat::Tsv,
            human_readable_descriptions.clone()
        )
        .is_ok());
        assert_eq!(
            format_output_table(OutputFormat::Tsv, human_readable_descriptions),
            "Annotee-Identifier\tHuman-Readable-Description\nProtein-123\ta kinase\n"
        );
        assert!(
            !std::path::Path::new(STREAM_PATH).exists(),
            "a file named {:?} was created in the working directory",
            STREAM_PATH
        );
    }
}
