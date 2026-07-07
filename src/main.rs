#[macro_use]
extern crate lazy_static;

use annotation_process::{run, AnnotationProcess};
use clap::{Arg, Command};
use model_funcs::parse_regex_file;
use regex::Regex;
use seq_family_reader::parse_seq_families_file;

/// Declare modules:
mod annotation_process;
mod default;
mod generate_hrd_associated_funcs;
mod model_funcs;
mod output_writer;
mod query;
mod seq_family;
mod seq_family_reader;
mod seq_sim_table_reader;

/// The famous `main` - entry point of `prot-scriber`. It parses the command line arguments, starts
/// the `prot-scriber` annotation process and writes the results into the respective output file.
fn main() {
    let matches = Command::new("prot-scriber")
        .version("version 0.1.6")
        .about("\nPLEASE USE '--help' FOR MORE DETAILS!\n\nprot-scriber assigns human readable descriptions (HRD) to query biological sequences or sets of them (a.k.a gene-families).\n")
        .after_help(concat!("\n\n", include_str!("../manual.txt")))
        .arg(
            Arg::new("output")
            .required(true)
            .takes_value(true)
            .short('o')
            .long("output")
            .help("Filename in which the tabular output will be stored.")
            .long_help("Filename in which the tabular output will be stored."),
        )
        .arg(
            Arg::new("seq-sim-table")
            .required(true)
            .short('s')
            .takes_value(true)
            .long("seq-sim-table")
            .multiple_occurrences(true)
            .help("File in which to find sequence similarity search results in tabular format")
            .long_help("File in which to find sequence similarity search results in tabular format (SSST). Use e.g. Blast or Diamond to produce them. Required columns are: 'qacc sacc stitle' (Blast) or 'qseqid sseqid stitle' (Diamond). (See section '2. prot-scriber input preparation' for more details.) If the required columns, or more, appear in different order than shown here you must use the --header (-e) argument. If any of the input SSSTs uses a different field-separator than the '<TAB>' character, you must provide the --field-separator (-p) argument. You can provide multiple SSSTs, simply by repeating the -s argument, e.g. '-s queries_vs_swissprot_diamond_out.txt -s queries_vs_trembl_diamond_out.txt'. Providing multiple --seq-sim-table (-s) arguments might imply the order in which you give other arguments like --header (-e) and --field-separator (-p). See there for more details."),
        )
        .arg(
            Arg::new("header")
            .short('e')
            .takes_value(true)
            .long("header")
            .multiple_occurrences(true)
            .help("Header of the --seq-sim-table (-s) arg.")
            .long_help("Header of the --seq-sim-table (-s) arg. Separated by space (' ') the names of the columns in order of appearance in the respective table. Required and default columns are 'qacc sacc stitle'. Note that this option only understands Blast terminology, i.e. even if you ran Diamond, please provide 'qacc' instead of 'qseqid' and 'sacc' instead of 'sseqid'. Luckily 'stitle' is 'stitle' in Diamond, too. You can have additional columns that will be ignored, as long as the required columns appear in the correct order. Consider this example: 'qacc sacc evalue bitscore stitle'. If multiple --seq-sim-table (-s) args are provided make sure the --header (-e) args appear in the correct order, e.g. the first -e arg will be used for the first -s arg, the second -e will be used for the second -s and so on. Set to 'default' to use the hard coded default."),
        )
        .arg(
            Arg::new("blacklist-regexs")
            .short('b')
            .takes_value(true)
            .long("blacklist-regexs")
            .multiple_occurrences(true)
            .help("A file with regular expressions used to exclude matching Blast Hit descriptions.")
            .long_help("A file with regular expressions (Rust syntax), one per line. Any match to any of these regular expressions causes sequence similarity search result descriptions ('stitle' in Blast terminology) to be discarded from the prot-scriber annotation process. If multiple --seq-sim-table (-s) args are provided make sure the --blacklist-regexs (-b) args appear in the correct order, e.g. the first -b arg will be used for the first -s arg, the second -b will be used for the second -s and so on. Set to 'default' to use the hard coded default. An example file can be downloaded here: https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/blacklist_stitle_regexs.txt - Note that this is an expert option."),
        )
        .arg(
            Arg::new("filter-regexs")
            .short('l')
            .takes_value(true)
            .long("filter-regexs")
            .multiple_occurrences(true)
            .help("A file with regular expressions used to delete parts of Blast Hit descriptions.")
            .long_help("A file with regular expressions (Rust syntax), one per line. Any match to any of these regular expressions causes the matched sub-string to be deleted, i.e. filtered out. Filtering is used to process descriptions ('stitle' in Blast terminology) and prepare the descriptions for the prot-scriber annotation process. In case of UniProt sequence similarity search results (Blast result tables), this removes the Blast Hit identifier (`sacc`) from the description (`stitle`) and also removes the taxonomic information starting with e.g. 'OS=' at the end of the `stitle` strings. If multiple --seq-sim-table (-s) args are provided make sure the --filter-regexs (-l) args appear in the correct order, e.g. the first -l arg will be used for the first -s arg, the second -l will be used for the second -s and so on. Set to 'default' to use the hard coded default. An example file can be downloaded here: https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/filter_stitle_regexs.txt - Note that this is an expert option."),
        )
        .arg(
            Arg::new("capture-replace-pairs")
            .short('c')
            .takes_value(true)
            .long("capture-replace-pairs")
            .multiple_occurrences(true)
            .help("A file with line pairs of regex and capture group replacement; used to transform matching parts of Blast Hit descriptions.")
            .long_help("A file with pairs of lines. Within each pair the first line is a regular expressions (fancy-regex syntax) defining one or more capture groups. The second line of a pair is the string used to replace the match in the regular expression with. This means the second line contains the capture groups (fancy-regex syntax). These pairs are used to further filter the sequence similarity search result descriptions ('stitle' in Blast terminology). In contrast to the --filter-regex (-l) matches are not deleted, but replaced with the second line of the pair. Filtering is used to process descriptions ('stitle' in Blast terminology) and prepare the descriptions for the prot-scriber annotation process. If multiple --seq-sim-table (-s) args are provided make sure the --capture-replace-pairs (-c) args appear in the correct order, e.g. the first -c arg will be used for the first -s arg, the second -c will be used for the second -s and so on. Set to 'default' to use the hard coded default. An example file can be downloaded here: https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/capture_replace_pairs.txt - Note that this is an expert option."),
        )
        .arg(
            Arg::new("field-separator")
            .short('p')
            .takes_value(true)
            .long("field-separator")
            .multiple_occurrences(true)
            .help("Field-Separator of the --seq-sim-table (-s) arg.")
            .long_help("Field-Separator of the --seq-sim-table (-s) arg. The default value is the '<TAB>' character. Consider this example: '-p @'. If multiple --seq-sim-table (-s) args are provided make sure the --field-separator (-p) args appear in the correct order, e.g. the first -p arg will be used for the first -s arg, the second -p will be used for the second -s and so on. You can provide '-p default' to use the hard coded default (TAB)."),
        )
        .arg(
            Arg::new("seq-families")
            .short('f')
            .takes_value(true)
            .long("seq-families")
            .help("A file in which families of biological sequences are stored, one family per line.")
            .long_help("A file in which families of biological sequences are stored, one family per line. Each line must have format 'fam-name TAB gene1,gene2,gene3'. Make sure no gene appears in more than one family."),
        )
        .arg(
            Arg::new("seq-family-id-genes-separator")
            .short('i')
            .takes_value(true)
            .long("seq-family-id-genes-separator")
            .help("A string used as separator in the argument --seq-families (-f) gene families file.")
            .long_help("A string used as separator in the argument --seq-families (-f) gene families file. This string separates the gene-family-identifier (name) from the gene-identifier list that family comprises. Default is '<TAB>' (\"\\t\")."),
        )
        .arg(
            Arg::new("seq-family-gene-ids-separator")
            .short('g')
            .takes_value(true)
            .long("seq-family-gene-ids-separator")
            .help("A regular expression used to split the list of gene-IDs in a gene-family file.")
            .long_help("A regular expression (Rust syntax) used to split the list of gene-identifiers in the argument --seq-families (-f) gene families file. Default is '(\\s*,\\s*|\\s+)'."),
        )
        .arg(
            Arg::new("annotate-non-family-queries")
            .short('a')
            .takes_value(false)
            .long("annotate-non-family-queries")
            .help("If given sequences that are not members of any family will also receive a HRD.")
            .long_help("Use this option only in combination with --seq-families (-f), i.e. when prot-scriber is used to generate human readable descriptions for gene families. If in that context this flag is given, queries for which there are sequence similarity search (Blast) results but that are NOT member of a sequence family will receive an annotation (human readable description) in the output file, too. Default value of this setting is 'OFF' (false)."),
        )
        .arg(
            Arg::new("description-split-regex")
            .short('r')
            .takes_value(true)
            .long("description-split-regex")
            .help("A regular expression used to split Blast Hit descriptions into words.")
            .long_help("A regular expression in Rust syntax to be used to split descriptions (`stitle` in Blast terminology) into words. Default is '([~_\\-/|\\;,':.\\s]+)'. Note that this is an expert option."),
        )
        .arg(
            Arg::new("center-inverse-word-information-content-at-quantile")
            .short('q')
            .takes_value(true)
            .long("center-inverse-word-information-content-at-quantile")
            .help("Either a number element [0,1] or 50. The quantile or mean to be used for centering.")
            .long_help("The quantile (percentile) to be subtracted from calculated inverse word information content to center these values. Consequently, this must be a value between zero and one or literal 50, which is interpreted as mean instead of a quantile. Default is 5o, implying centering at the mean. Note that this is an expert option."),
        )
        .arg(
            Arg::new("verbose")
            .short('v')
            .long("verbose")
            .long_help("Print informative messages about the annotation process."),
        )
        .arg(
            Arg::new("non-informative-words-regexs")
            .short('w')
            .takes_value(true)
            .long("non-informative-words-regexs")
            .help("File of regular expressions used to identify non informative words.")
            .long_help("The path to a file in which regular expressions (regexs) are stored, one per line. These regexs are used to recognize non-informative words, which will only receive a minimun score in the prot-scriber process that generates human readable description. There is a default list hard-coded into prot-scriber. An example file can be downloaded here: https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/non_informative_words_regexs.txt - Note that this is an expert option."),
        )
        .arg(
            Arg::new("polish-capture-replace-pairs")
            .short('d')
            .long("polish-capture-replace-pairs")
            .help("A file with line pairs of regex and capture group replacement; used in the last step ('polishing') when generating human readable description. Set to 'none' if you want to skip the polishing step.")
            .long_help("The last step of the process generating human readable descriptions (HRDs) for the queries (proteins or sequence families) is to 'polish' the selected HRDs. Polishing is done by iterative application of regular expressions (fancy-regex) and replace instructions (capture-replace-pairs). If you do not want to use the default polishing capture replace pairs specify a file in which pairs of lines are given. Of each pair the first line hold a regular expression (fancy-regex syntax) and the second the replacement instructions providing access to capture groups. Set to 'none' or provide an empty file, if you want to suppress polishing. If you want to have a template file for your custom polishing capture-replace-pairs please refer to\nhttps://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/polish_capture_replace_pairs.txt\n- Note that this an expert option."),
        )
        .arg(
            Arg::new("n-threads")
            .short('n')
            .takes_value(true)
            .long("n-threads")
            .help("The maximum number of parallel threads to use.")
            .long_help("The maximum number of parallel threads to use. Default is the number of logical cores. Required minimum is two (2). Note that at most one thread is used per input sequence similarity search result (Blast table) file. After parsing these annotation may use up to this number of threads to generate human readable descriptions."),
        ).arg(
            Arg::new("exclude-not-annotated-queries")
            .short('x')
            .takes_value(false)
            .long("exclude-not-annotated-queries")
            .help("Exclude results from the output table that could not be annotated.")
            .long_help("Exclude results from the output table that could not be annotated, i.e. 'unknown protein' or 'unknown sequence family', respectively."),
        ).get_matches();

    // Create a new AnnotationProcess instance and provide it with the necessary input data:
    let mut annotation_process = AnnotationProcess::new();

    // Does the user want informative messages printed out?
    if matches.is_present("verbose") {
        annotation_process.verbose = true;
    }

    // Set number of parallel processes to use:
    if let Some(n_threads) = matches.value_of("n-threads") {
        annotation_process.n_threads = n_threads
            .trim()
            .parse()
            .expect("Could not parse argument '--n-threads' ('-n') into a positive integer");
    }

    // Set the number of parallel processes to be used by `rayon` (see
    // `AnnotationProcess::process_rest_data`):
    rayon::ThreadPoolBuilder::new()
        .num_threads(annotation_process.n_threads)
        .build_global()
        .expect("Could not set the number of parallel processes to be used to generate human readable descriptions (AnnotationProcess::process_rest_data).");

    // Add biological sequence families information, if provided as input by the user:
    if let Some(seq_families) = matches.value_of("seq-families") {
        // What is the character that separates a gene-family-identifier from its list of
        // gene-identifiers?
        if matches.is_present("seq-family-id-genes-separator") {
            annotation_process.seq_family_id_genes_separator = matches
                .value_of("seq-family-id-genes-separator")
                .unwrap()
                .trim()
                .to_string();
        }

        // What is the regular expression (string representation) that shall be used to split the list
        // of gene-identifiers a gene-family comprises?
        if matches.is_present("seq-family-gene-ids-separator") {
            annotation_process.seq_family_gene_ids_separator = matches
                .value_of("seq-family-gene-ids-separator")
                .unwrap()
                .trim()
                .to_string();
        }

        // Shall non family queries also be annotated?
        if matches.is_present("annotate-non-family-queries") {
            annotation_process.annotate_lonely_queries = true;
        }

        parse_seq_families_file(seq_families, &mut annotation_process);
        if annotation_process.verbose {
            println!(
                "Loaded {:?} sequence families from {:?}",
                &annotation_process.seq_families.len(),
                &seq_families
            );
        }
    }

    // Set the input sequence similarity search result (SSSR) tables (Blast or Diamond):
    annotation_process.seq_sim_search_tables = matches
        .values_of("seq-sim-table")
        .unwrap()
        .map(|x| (*x).to_string())
        .collect();

    // For each of the above to be parsed SSSR tables set their column mappings, if given by the
    // user:
    if matches.is_present("header") {
        for header_arg in matches.values_of("header").unwrap() {
            annotation_process.add_ssst_columns(header_arg);
        }
    }

    // For each of the above to be parsed SSSR tables set their their respective field-separator,
    // if given by the user:
    if matches.is_present("field-separator") {
        for field_separator in matches.values_of("field-separator").unwrap() {
            annotation_process.add_ssst_field_separator(field_separator);
        }
    }

    // For each of the above to be parsed SSSR tables set the blacklist filter, i.e. vectors of
    // regular expressions:
    if matches.is_present("blacklist-regexs") {
        for blacklist_arg in matches.values_of("blacklist-regexs").unwrap() {
            annotation_process.add_ssst_blacklist_regexs(blacklist_arg);
        }
    }

    // For each of the above to be parsed SSSR tables set the filter regexs, i.e. vectors of
    // regular expressions:
    if matches.is_present("filter-regexs") {
        for filter_arg in matches.values_of("filter-regexs").unwrap() {
            annotation_process.add_ssst_filter_regexs(filter_arg);
        }
    }

    // For each of the above to be parsed SSSR tables set the capture-replace-pairs, i.e. vectors
    // of two member tuples, where the first entry is a regular expression and the second is the
    // replace string including capture groups (see
    // `generate_hrd_associated_funcs::split_descriptions` for more details):
    if matches.is_present("capture-replace-pairs") {
        for cr_pairs_arg in matches.values_of("capture-replace-pairs").unwrap() {
            annotation_process.add_ssst_capture_replace_pairs(cr_pairs_arg);
        }
    }

    // Set the capture replace pairs (fancy-regex) used in the last step of the generation of
    // human readable descriptions. Note, that this can be "none" or "default".
    if matches.is_present("polish-capture-replace-pairs") {
        annotation_process.set_polish_capture_replace_pairs(
            matches.value_of("polish-capture-replace-pairs").unwrap(),
        );
    }

    // Did the user supply a custom regular expression to split descriptions (`stitle` in Blast
    // terminology) into words?
    if matches.is_present("description-split-regex") {
        annotation_process.description_split_regex =
            Regex::new(matches.value_of("description-split-regex").unwrap()).unwrap_or_else(|_|
                panic!(
                    "Could not parse --description-split-regex (-r) argument {:?} into a Rust regular expression. Please check the syntax or use the default (see --help for details).",
                    matches.value_of("description-split-regex").unwrap()
                )
            );
    }

    // Did the user supply a custom quantile (percentile) value to be used to center inverse word
    // information content scores?
    if matches.is_present("center-inverse-word-information-content-at-quantile") {
        annotation_process.center_iic_at_quantile = matches
            .value_of("center-inverse-word-information-content-at-quantile")
            .unwrap()
            .trim()
            .parse()
            .expect("Could not parse provided --center-inverse-word-information-content-at-quantile (-q) argument into a real value");
    }

    // Did the user provide an optional file containing regular expressions, one per line, to be
    // used to recognize non-informative words?
    if matches.is_present("non-informative-words-regexs") {
        annotation_process.non_informative_words_regexs =
            parse_regex_file(matches.value_of("non-informative-words-regexs").unwrap());
    }

    // Shall non annotable queries or sequence families be excluded from the output table?
    if matches.is_present("exclude-not-annotated-queries") {
        annotation_process.exclude_not_annotated_from_output = true;
    }

    // Execute the Annotation-Process:
    annotation_process = run(annotation_process);

    // Save output:
    if let Some(o) = matches.value_of("output") {
        match output_writer::write_output_table(
            o.to_string(),
            annotation_process.human_readable_descriptions,
        ) {
            Ok(()) => {
                if annotation_process.verbose {
                    println!("output written to file {:?}.", o);
                }
            }
            Err(e) => eprintln!(
                "We are sorry, an error occurred when attempting to write output to file {:?} \n{:?}",
                o, e
            ),
        };
    }
}
