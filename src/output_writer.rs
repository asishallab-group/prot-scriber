use std::collections::HashMap;
use std::fs::write;

/// Parse human_readable_descriptions into string and save to a file.
///
/// The rows are written sorted by annotee identifier. A `HashMap` has no order of its own and
/// yields its entries differently in every process, so without this the same analysis re-run over
/// the same inputs produces a file that differs line by line from the one before it.
///
/// # Arguments
///
/// * `file_path: String` - The file path for saving output.
/// * `human_readable_descriptions: HashMap<String, String>` - The generated human readable descriptions.
pub fn write_output_table(
    file_path: String,
    human_readable_descriptions: HashMap<String, String>,
) -> std::io::Result<()> {
    if !human_readable_descriptions.is_empty() {
        let mut annotations: Vec<(String, String)> =
            human_readable_descriptions.into_iter().collect();
        annotations.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));

        let mut output = String::from("Annotee-Identifier\tHuman-Readable-Description");
        // stream write line after line
        for (annotee_name, annotation) in annotations {
            output.push_str(&(format!("\n{}\t{}", annotee_name, annotation)));
        }
        // add trailing newline for the last annotation
        output.push('\n');
        write(file_path, output)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::output_writer::write_output_table;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

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
        const OUT_FILE: &str = "./target/result.txt";
        assert!(
            write_output_table(OUT_FILE.to_string(), human_readable_descriptions).is_ok()
        );
        // 'Protein-123' was inserted second but is written first:
        assert_eq!(
            std::fs::read_to_string(OUT_FILE).unwrap(),
            "Annotee-Identifier\tHuman-Readable-Description\n\
             Protein-123\thuman devouring protein\n\
             Seq-Family-1\talien devouring protein\n"
        );
    }
}
