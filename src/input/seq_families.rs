//! Parsing of the gene family input file passed to the `--seq-families` argument, in which each
//! line names a family and lists the biological sequences it comprises.

use crate::model::seq_family::SeqFamily;
use regex::Regex;
use std::fmt;

#[derive(Debug, Clone)]
pub struct MalformattedGeneFamilyError;

impl fmt::Display for MalformattedGeneFamilyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Malformatted gene family")
    }
}

/// Parses a single line read from a respective "gene family input file" (see
/// `parse_seq_families_file`). The argument `family: String` is split into a family identifier
/// (name) and the set of sequence identifiers the family comprises, is made of. Returns a
/// `Result<(String, SeqFamily), std::error::Error>` holding either the parsed gene family or an
/// error.
///
/// # Arguments
///
/// * `family` - The single line (`String`) holding the gene family information
/// * `fam_id_from_gene_id_list_separator` - The character that separates a gene-family's
///   identifier from the list of gene-identifiers the family comprises.
/// * `gene_ids_separator_regex` - The regular expression to be used to split the list of
///   gene-identifiers. It is compiled by the caller, once for the whole file, so that a
///   `--seq-family-gene-ids-separator` (`-g`) that is not a regular expression is reported as the
///   command line mistake it is instead of failing on the first line that uses it.
pub fn parse_seq_family(
    family: String,
    fam_id_from_gene_id_list_separator: &String,
    gene_ids_separator_regex: &Regex,
) -> Result<(String, SeqFamily), MalformattedGeneFamilyError> {
    // Split the line by argument `fam_id_from_gene_id_list_separator`. There should be more than 1
    // element (>=2), panic if not:
    let family_cols: Vec<&str> = family
        .trim()
        .split(fam_id_from_gene_id_list_separator)
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .collect();
    if family_cols.len() < 2 {
        return Err(MalformattedGeneFamilyError);
    }

    let seq_fam_name: String = String::from(family_cols[0]); // the family name
    let mut seq_fam_instance = SeqFamily::new(); // the genes contained in that family

    // split the gene column using the default separator.
    // In case genes were separated by a <TAB> character
    // we need to re-join the remaining elements.
    let gene_cols: Vec<String> = gene_ids_separator_regex
        .split(&family_cols[1..].join("\t"))
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect();

    // set family genes
    seq_fam_instance.query_ids = gene_cols;
    // return OK
    Ok((seq_fam_name, seq_fam_instance))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use crate::default::{SPLIT_GENE_FAMILY_GENES_REGEX, SPLIT_GENE_FAMILY_ID_FROM_GENE_SET};

    #[test]
    fn parses_correct_lines_ok() {
        let line_1: String = "OG0023617 VFABAHed036352,VFABAHed036353".to_string();
        let line_2: String =
            "OG0023617  VFABAHed036490 ,     VFABAHed036491  VFABAHed036353  ".to_string();
        let line_3: String = "OG0023617 VFABAHed036352  VFABAHed036353, VFABAHed036354".to_string();

        match parse_seq_family(
            line_1,
            &(*SPLIT_GENE_FAMILY_ID_FROM_GENE_SET).to_string(),
            &Regex::new(SPLIT_GENE_FAMILY_GENES_REGEX).unwrap(),
        ) {
            Ok((seq_fam_name, seq_fam_instance)) => {
                assert_eq!(seq_fam_name, "OG0023617");
                assert_eq!(seq_fam_instance.query_ids.len(), 2);
            }
            Err(e) => println!("{}", e),
        }

        match parse_seq_family(
            line_2,
            &(*SPLIT_GENE_FAMILY_ID_FROM_GENE_SET).to_string(),
            &Regex::new(SPLIT_GENE_FAMILY_GENES_REGEX).unwrap(),
        ) {
            Ok((seq_fam_name, seq_fam_instance)) => {
                assert_eq!(seq_fam_name, "OG0023617");
                assert_eq!(seq_fam_instance.query_ids.len(), 3);
            }
            Err(e) => println!("{}", e),
        }

        match parse_seq_family(
            line_3,
            &(*SPLIT_GENE_FAMILY_ID_FROM_GENE_SET).to_string(),
            &Regex::new(SPLIT_GENE_FAMILY_GENES_REGEX).unwrap(),
        ) {
            Ok((seq_fam_name, seq_fam_instance)) => {
                assert_eq!(seq_fam_name, "OG0023617");
                assert_eq!(seq_fam_instance.query_ids.len(), 3);
            }
            Err(e) => println!("{}", e),
        }
    }

    #[test]
    fn parse_faulty_line_malformatted() {
        let line = "OG0023619|VFABAHed036490,VFABAHed036491".to_string();
        assert!(parse_seq_family(
            line,
            &(*SPLIT_GENE_FAMILY_ID_FROM_GENE_SET).to_string(),
            &Regex::new(SPLIT_GENE_FAMILY_GENES_REGEX).unwrap(),
        )
        .is_err())
    }
}
