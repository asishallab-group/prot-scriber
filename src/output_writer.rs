use std::collections::HashMap;
use std::fs::write;
use std::io::{self, Write};

/// The `--output` (-o) value that asks for the table on standard output instead of in a file. A
/// single dash is what every unix tool that has this at all uses for it, and it cannot collide
/// with a real file name: a shell expands `-` to itself, and a path meant literally can still be
/// written as `./-`.
pub const STDOUT_PATH: &str = "-";

/// Parse human_readable_descriptions into string and save it, either to the named file or -- if
/// `file_path` is `STDOUT_PATH` -- to standard output.
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
/// * `file_path: String` - The file path for saving output, or `STDOUT_PATH` for standard output.
/// * `human_readable_descriptions: HashMap<String, String>` - The generated human readable descriptions.
pub fn write_output_table(
    file_path: String,
    human_readable_descriptions: HashMap<String, String>,
) -> io::Result<()> {
    let output = format_output_table(human_readable_descriptions);
    if file_path == STDOUT_PATH {
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
/// * `human_readable_descriptions: HashMap<String, String>` - The generated human readable descriptions.
fn format_output_table(human_readable_descriptions: HashMap<String, String>) -> String {
    let mut annotations: Vec<(String, String)> = human_readable_descriptions.into_iter().collect();
    annotations.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

    let mut output = String::from("Annotee-Identifier\tHuman-Readable-Description");
    // The whole table is built in memory and written in one go; nothing here streams.
    for (annotee_name, annotation) in annotations {
        output.push_str(&(format!("\n{}\t{}", annotee_name, annotation)));
    }
    // add trailing newline for the last annotation
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use crate::output_writer::{format_output_table, write_output_table, STDOUT_PATH};
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;
    use crate::test_support::scratch_file;

    #[test]
    fn writer_test() {
        let mut human_readable_descriptions: HashMap<String, String> = HashMap::new();
        human_readable_descriptions.insert(
            "Seq-Family-1".to_string(),
            "alien devouring protein".to_string(),
        );
        human_readable_descriptions.insert(
            "Protein-123".to_string(),
            "human devouring protein".to_string(),
        );
        let out_file = scratch_file("result.txt");
        assert!(write_output_table(
            out_file.to_string_lossy().into_owned(),
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

    #[test]
    fn writes_the_header_when_there_is_nothing_to_report() {
        let out_file = scratch_file("empty_result.txt");
        let _ = std::fs::remove_file(&out_file);
        assert!(write_output_table(out_file.to_string_lossy().into_owned(), HashMap::new()).is_ok());
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
        let mut human_readable_descriptions: HashMap<String, String> = HashMap::new();
        human_readable_descriptions.insert("Protein-123".to_string(), "a kinase".to_string());
        assert!(write_output_table(
            STDOUT_PATH.to_string(),
            human_readable_descriptions.clone()
        )
        .is_ok());
        assert_eq!(
            format_output_table(human_readable_descriptions),
            "Annotee-Identifier\tHuman-Readable-Description\nProtein-123\ta kinase\n"
        );
        assert!(
            !std::path::Path::new(STDOUT_PATH).exists(),
            "a file named {:?} was created in the working directory",
            STDOUT_PATH
        );
    }
}
