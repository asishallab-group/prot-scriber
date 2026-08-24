//! prot-scriber's command line interface. The structs below *are* the interface: `clap` derives
//! the parser, the help pages and the value conversions from them, so every argument is declared
//! in exactly one place instead of being defined here and then looked up by name, and re-parsed,
//! in `AnnotationProcess::from`.
//!
//! Annotating is what prot-scriber is for and stays what it does when no verb is given, so
//! `prot-scriber -s hits.tsv -o out.tsv` means what it has always meant and will keep meaning it.
//! A verb, on the other hand, has to be usable without `-o` and `-s`, which are required of an
//! annotation run and meaningless to anything else: `subcommand_negates_reqs` says so to the
//! parser, and the annotation arguments are additionally wrapped in an `Option`, because without
//! that the derived `FromArgMatches` still has a `String` field to fill and reports the argument
//! missing itself, after the parser has already decided not to require it.
//!
//! Beware of doc comments on the structs below: `clap` puts everything after their first paragraph
//! into the long help, where it would reach users rather than readers of the source.

pub use crate::assets::DefaultList;
pub use clap::{Parser, ValueEnum};
use clap::Subcommand;
use regex::Regex;


/// A per-table option's value: which table it is meant for, and what it says.
///
/// This is the whole point of the redesign. The five per-table options used to be matched to their
/// tables *by position* -- the first `-l` belonged to the first `-s` -- which is impossible to see
/// in a long command line and impossible to check, because a list of filter regular expressions is
/// valid for any table. On 07.08.2026 a benchmark run gave every one of its inputs the NCBI-NR
/// filter list that way and succeeded, putting boilerplate into 22.6 % of the descriptions it
/// generated. Naming the table makes the mistake unrepresentable rather than merely detectable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedValue {
    /// The name the table was declared under with `--db`.
    pub name: String,
    /// What this option says about that table.
    pub value: String,
}

/// Whether `candidate` can be a table name: letters, digits, underscores and dashes, and at least
/// one of them. Deliberately narrow, so that `NAME=VALUE` can be told from a bare path with no
/// guessing -- a name can never contain a path separator or a dot.
fn is_table_name(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parses a `NAME=VALUE` argument, as every `--db-*` option takes.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_named_value(arg: &str) -> Result<NamedValue, String> {
    match arg.split_once('=') {
        Some((name, value)) if is_table_name(name) && !value.is_empty() => Ok(NamedValue {
            name: name.to_string(),
            value: value.to_string(),
        }),
        Some((name, _)) if !is_table_name(name) => Err(format!(
            "{:?} is not the name of a table. A name is made of letters, digits, underscores and \
             dashes, and is the name a table was given with --db",
            name
        )),
        _ => Err(String::from(
            "expected NAME=VALUE, naming the --db table this applies to",
        )),
    }
}

/// Parses a `--db` argument, which is either `NAME=PATH` or a bare `PATH` whose file name becomes
/// the name.
///
/// A path is only read as `NAME=PATH` when what stands before the first `=` could be a name, so a
/// file whose own name contains an `=` is still a path; write `./odd=name.tsv` if the first
/// component of a relative path would otherwise look like a name.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_table_declaration(arg: &str) -> Result<NamedValue, String> {
    if let Some((name, path)) = arg.split_once('=') {
        if is_table_name(name) && !path.is_empty() {
            return Ok(NamedValue {
                name: name.to_string(),
                value: path.to_string(),
            });
        }
    }
    if arg.is_empty() {
        return Err(String::from("is the empty string, not the path of a table"));
    }
    let name = std::path::Path::new(arg)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| {
            format!(
                "{:?} has no file name to take a table name from; write NAME={} instead",
                arg, arg
            )
        })?;
    Ok(NamedValue {
        name,
        value: arg.to_string(),
    })
}


/// Quotes `value` for a shell if it needs it, so that what is printed can be pasted and run.
///
/// # Arguments
///
/// * `value` - The argument value to quote.
fn shell_quote(value: &str) -> String {
    let safe = value.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-' | '=' | '+' | ':' | '@' | ',')
    });
    if safe && !value.is_empty() {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

/// The arguments a positional command line would be written with today: what to take out, and what
/// to put in its place. `None` if no positional per-table option was used.
///
/// Deliberately *not* a whole command line. A whole command line would have to reproduce every
/// argument, including the ones this knows nothing about -- and when it did not, it dropped them
/// silently while claiming to say the same thing: a run with `--seq-families` was handed a command
/// that annotates single sequences. What is offered instead is the exchange itself, which is
/// complete for what it claims and cannot leave anything out, because it never had it.
///
/// This outlives the options it translates, on purpose: a script found in 2030 should get its
/// replacement printed rather than "unexpected argument".
///
/// # Arguments
///
/// * `args` - The parsed command line.
pub fn translate_positional_form(args: &Args) -> Option<(String, String)> {
    let positional: [(&str, &str, &Vec<String>); 5] = [
        ("-e", "--db-header", &args.header),
        ("-p", "--db-sep", &args.field_separator),
        ("-b", "--db-blacklist", &args.blacklist_regexs),
        ("-l", "--db-filter", &args.filter_regexs),
        ("-c", "--db-capture-replace", &args.capture_replace_pairs),
    ];
    if positional.iter().all(|(_, _, given)| given.is_empty()) {
        return None;
    }

    let mut replace = Vec::new();
    let mut with = Vec::new();
    for table in &args.seq_sim_table {
        replace.push(String::from("-s"));
        replace.push(shell_quote(&table.value));
        with.push(String::from("--db"));
        with.push(shell_quote(&format!("{}={}", table.name, table.value)));
    }
    for (old_option, new_option, given) in positional {
        for (i, value) in given.iter().enumerate() {
            replace.push(String::from(old_option));
            replace.push(shell_quote(value));
            // The i-th value belongs to the i-th table -- the rule being translated away, and the
            // reason a mistake in it was invisible:
            if let Some(table) = args.seq_sim_table.get(i) {
                with.push(String::from(new_option));
                with.push(shell_quote(&format!("{}={}", table.name, value)));
            }
        }
    }
    Some((replace.join(" "), with.join(" ")))
}

/// What prot-scriber was asked to do: a verb, or -- given none -- an annotation run.
#[derive(Parser, Debug)]
#[command(
    name = "prot-scriber",
    // clap 4 dropped its dependency on `textwrap` and wraps to the detected terminal width, which
    // means no wrapping at all when the help is piped or redirected -- as it is when it gets
    // pasted into README.md. Cap it, so the long help stays readable everywhere:
    max_term_width = 100,
    version = "version 0.1.6",
    about = "\nPLEASE USE '--help' FOR MORE DETAILS!\n\nprot-scriber assigns human readable descriptions (HRD) to query biological sequences or sets of them (a.k.a gene-families).\n",
    after_long_help = concat!("\n\n", include_str!("../MANUAL.txt")),
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    // `None` exactly when a verb was given: with no verb this is an annotation run, and the
    // parser has already insisted on the arguments one needs.
    #[command(flatten)]
    pub annotate: Option<Args>,
}

/// The verbs prot-scriber understands in addition to annotating.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Assign human readable descriptions to queries or families of them. The default.
    #[command(
        long_about = "Assign human readable descriptions to queries or to families of them. This is what prot-scriber does, and what it does when no verb is given at all, so 'prot-scriber annotate --db hits.tsv -o out.tsv' and 'prot-scriber --db hits.tsv -o out.tsv' are the same run. Writing the verb costs nothing and says what is meant; leaving it out keeps every command line that was written before verbs existed working."
    )]
    Annotate(Box<Args>),

    /// Print one of prot-scriber's built-in regular expression lists.
    #[command(
        long_about = "Print one of prot-scriber's built-in regular expression lists, exactly as prot-scriber itself uses it. Without a name, the available lists are listed.\n\nThese are the lists to start from when you want to change how descriptions are processed: write one to a file, edit it, and give it back with the option named beside it. Nothing needs downloading, and there is no version of a list other than the one this binary applies.\n\n  prot-scriber defaults filter-regexs > my_filters.txt\n  prot-scriber defaults filter-regexs | diff - my_filters.txt\n\nThe table goes to standard output, so it can be redirected or piped."
    )]
    Defaults {
        /// Which list to print. Omit to see what there is.
        #[arg(value_name = "NAME")]
        name: Option<DefaultList>,
    },
}


/// Parses the `--center-inverse-word-information-content-at-quantile` argument, which is valid
/// only as a quantile in `[0, 1]` or as the literal 50, meaning "center at the mean instead".
/// Handing this to `clap` means an invalid value is reported as a command line error before any
/// work starts, rather than as a panic once the annotation process is already running.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_center_at_quantile(arg: &str) -> Result<f64, String> {
    let tau: f64 = arg
        .trim()
        .parse()
        .map_err(|_| format!("{:?} is not a real number", arg))?;
    if tau == 50.0 || (0.0..=1.0).contains(&tau) {
        Ok(tau)
    } else {
        Err(String::from(
            "must be between zero and one (both inclusive), or the literal 50 to center at the mean",
        ))
    }
}

/// Parses the `--n-threads` argument, of which prot-scriber requires at least two: one thread
/// parses an input table while another consumes what it sends. Handing this to `clap` means an
/// invalid value is reported as a command line error before any work starts.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_n_threads(arg: &str) -> Result<usize, String> {
    let n_threads: usize = arg
        .trim()
        .parse()
        .map_err(|_| format!("{:?} is not a positive integer", arg))?;
    if n_threads >= 2 {
        Ok(n_threads)
    } else {
        Err(String::from("must be at least two (2)"))
    }
}

/// Every argument prot-scriber accepts. Arguments that may be repeated, once per input sequence
/// similarity search result table, are `Vec`s; optional scalar arguments are `Option`s; flags are
/// `bool`s. A field's type therefore states how often its argument may be given, and hands the
/// rest of the program a value that is already parsed and validated.
#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(
        short = 'o',
        long,
        help = "Filename in which the tabular output will be stored. Use '-' for standard output.",
        long_help = "Filename in which the tabular output will be stored. Give a single dash ('-') to write the table to standard output instead of to a file. Progress messages, warnings and errors always go to standard error, so the standard output carries the table and nothing else and 'prot-scriber ... -o - | head' shows you its first rows."
    )]
    pub output: String,

    #[arg(
        short = 's',
        long = "db",
        visible_alias = "seq-sim-table",
        required = true,
        value_name = "[NAME=]PATH",
        value_parser = parse_table_declaration,
        help = "A database's sequence similarity search results, in tabular format. Give it a name with NAME=PATH.",
        long_help = "File in which to find sequence similarity search results in tabular format (SSST). Use e.g. Blast or Diamond to produce them. Required columns are: 'qacc sacc stitle' (Blast) or 'qseqid sseqid stitle' (Diamond). (See section '2. prot-scriber input preparation' for more details.) If the required columns, or more, appear in different order than shown here you must use the --header (-e) argument. If any of the input SSSTs uses a different field-separator than the '<TAB>' character, you must provide the --field-separator (-p) argument. You can provide multiple SSSTs, simply by repeating the -s argument, e.g. '-s queries_vs_swissprot_diamond_out.txt -s queries_vs_trembl_diamond_out.txt'. Providing multiple --seq-sim-table (-s) arguments might imply the order in which you give other arguments like --header (-e) and --field-separator (-p). See there for more details. All rows belonging to one query must stand together in the table, which is what Blast and Diamond produce on their own; concatenating tables or shuffling one does not preserve it, and prot-scriber stops with an error rather than annotate a query twice. 'sort -k <qacc-col-no> <table>' restores it.\n\nGive a table a name with NAME=PATH, e.g. '--db nr=at_vs_nr.tsv', and the --db-header, --db-sep, --db-blacklist, --db-filter and --db-capture-replace options can then say which table they are for by that name instead of by the order they are written in. Without a name a table is called after its file, so '--db at_vs_nr.tsv' is the table 'at_vs_nr'."
    )]
    pub seq_sim_table: Vec<NamedValue>,

    #[arg(
        short = 'e',
        long,
        help = "Header of the --seq-sim-table (-s) arg.",
        long_help = "Header of the --seq-sim-table (-s) arg. Separated by space (' ') the names of the columns in order of appearance in the respective table. Required and default columns are 'qacc sacc stitle'. Blast and Diamond terminology are both understood: write 'qacc' and 'sacc', or Diamond's 'qseqid' and 'sseqid', whichever your search actually produced. 'stitle' is 'stitle' in both. You can have additional columns, which will be ignored, and the required ones may appear in any order: what this argument does is tell prot-scriber which column is which. Consider this example: 'qacc sacc evalue bitscore stitle'. If multiple --seq-sim-table (-s) args are provided make sure the --header (-e) args appear in the correct order, e.g. the first -e arg will be used for the first -s arg, the second -e will be used for the second -s and so on. Set to 'default' to use the hard coded default."
    )]
    pub header: Vec<String>,

    #[arg(
        short = 'b',
        long,
        help = "A file with regular expressions used to exclude matching Blast Hit descriptions.",
        long_help = "A file with regular expressions (Rust syntax), one per line. Any match to any of these regular expressions causes sequence similarity search result descriptions ('stitle' in Blast terminology) to be discarded from the prot-scriber annotation process. If multiple --seq-sim-table (-s) args are provided make sure the --blacklist-regexs (-b) args appear in the correct order, e.g. the first -b arg will be used for the first -s arg, the second -b will be used for the second -s and so on. Set to 'default' to use the hard coded default. Write the default out to start from it, with 'prot-scriber defaults blacklist-regexs > my_blacklist_regexs.txt'; nothing needs downloading, and what you get is the list this binary applies. - Note that this is an expert option."
    )]
    pub blacklist_regexs: Vec<String>,

    #[arg(
        short = 'l',
        long,
        help = "A file with regular expressions used to delete parts of Blast Hit descriptions.",
        long_help = "A file with regular expressions (Rust syntax), one per line. Any match to any of these regular expressions causes the matched sub-string to be deleted, i.e. filtered out. Filtering is used to process descriptions ('stitle' in Blast terminology) and prepare the descriptions for the prot-scriber annotation process. In case of UniProt sequence similarity search results (Blast result tables), this removes the Blast Hit identifier (`sacc`) from the description (`stitle`) and also removes the taxonomic information starting with e.g. 'OS=' at the end of the `stitle` strings. If multiple --seq-sim-table (-s) args are provided make sure the --filter-regexs (-l) args appear in the correct order, e.g. the first -l arg will be used for the first -s arg, the second -l will be used for the second -s and so on. Set to 'default' to use the hard coded default. Write the default out to start from it, with 'prot-scriber defaults filter-regexs > my_filter_regexs.txt'; nothing needs downloading, and what you get is the list this binary applies. Sequence similarity search results from NCBI's non-redundant database and from the UniRef databases have description formats of their own and need a tailored list; those ship too, as 'prot-scriber defaults filter-regexs-ncbi-nr' and 'prot-scriber defaults filter-regexs-uniref'. - Note that this is an expert option."
    )]
    pub filter_regexs: Vec<String>,

    #[arg(
        short = 'c',
        long,
        help = "A file with line pairs of regex and capture group replacement; used to transform matching parts of Blast Hit descriptions.",
        long_help = "A file with pairs of lines. Within each pair the first line is a regular expressions (fancy-regex syntax) defining one or more capture groups. The second line of a pair is the string used to replace the match in the regular expression with. This means the second line contains the capture groups (fancy-regex syntax). These pairs are used to further filter the sequence similarity search result descriptions ('stitle' in Blast terminology). In contrast to the --filter-regex (-l) matches are not deleted, but replaced with the second line of the pair. Filtering is used to process descriptions ('stitle' in Blast terminology) and prepare the descriptions for the prot-scriber annotation process. If multiple --seq-sim-table (-s) args are provided make sure the --capture-replace-pairs (-c) args appear in the correct order, e.g. the first -c arg will be used for the first -s arg, the second -c will be used for the second -s and so on. Set to 'default' to use the hard coded default. Write the default out to start from it, with 'prot-scriber defaults capture-replace-pairs > my_capture_replace_pairs.txt'; nothing needs downloading, and what you get is the list this binary applies. - Note that this is an expert option."
    )]
    pub capture_replace_pairs: Vec<String>,

    #[arg(
        short = 'p',
        long,
        help = "Field-Separator of the --seq-sim-table (-s) arg.",
        long_help = "Field-Separator of the --seq-sim-table (-s) arg. The default value is the '<TAB>' character. Consider this example: '-p @'. If multiple --seq-sim-table (-s) args are provided make sure the --field-separator (-p) args appear in the correct order, e.g. the first -p arg will be used for the first -s arg, the second -p will be used for the second -s and so on. A field separator is a single character; write '\\t' or 'tab' for the TAB character, '\\s' for a space and '\\0' for the null byte, since a shell makes those awkward to type literally. You can provide '-p default' to use the hard coded default (TAB)."
    )]
    pub field_separator: Vec<String>,

    #[arg(
        long = "db-header",
        value_name = "NAME=SPEC",
        value_parser = parse_named_value,
        conflicts_with = "header",
        help = "The header of one --db table, as NAME=SPEC.",
        long_help = "The header of one --db table, as NAME=SPEC, e.g. '--db-header nr=\"qacc sacc evalue stitle\"'. The same thing --header (-e) says, but about the table it names rather than about the table in the same position, which is the whole reason this option exists. Cannot be combined with --header (-e)."
    )]
    pub db_header: Vec<NamedValue>,

    #[arg(
        long = "db-sep",
        value_name = "NAME=CHAR",
        value_parser = parse_named_value,
        conflicts_with = "field_separator",
        help = "The field separator of one --db table, as NAME=CHAR.",
        long_help = "The field separator of one --db table, as NAME=CHAR, e.g. '--db-sep nr=\\t'. The same thing --field-separator (-p) says, but about the table it names. Cannot be combined with --field-separator (-p)."
    )]
    pub db_sep: Vec<NamedValue>,

    #[arg(
        long = "db-blacklist",
        value_name = "NAME=SOURCE",
        value_parser = parse_named_value,
        conflicts_with = "blacklist_regexs",
        help = "The blacklist regular expressions for one --db table, as NAME=SOURCE.",
        long_help = "The blacklist regular expressions for one --db table, as NAME=SOURCE. The same thing --blacklist-regexs (-b) says, but about the table it names. Cannot be combined with --blacklist-regexs (-b). The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list at all."
    )]
    pub db_blacklist: Vec<NamedValue>,

    #[arg(
        long = "db-filter",
        value_name = "NAME=SOURCE",
        value_parser = parse_named_value,
        conflicts_with = "filter_regexs",
        help = "The filter regular expressions for one --db table, as NAME=SOURCE.",
        long_help = "The filter regular expressions for one --db table, as NAME=SOURCE, e.g. '--db-filter nr=@filter-regexs-ncbi-nr'. The same thing --filter-regexs (-l) says, but about the table it names -- and this is the option the whole redesign is for: it can only ever mean the table declared '--db nr=...', whatever order the arguments are written in. Cannot be combined with --filter-regexs (-l). The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list at all."
    )]
    pub db_filter: Vec<NamedValue>,

    #[arg(
        long = "db-capture-replace",
        value_name = "NAME=SOURCE",
        value_parser = parse_named_value,
        conflicts_with = "capture_replace_pairs",
        help = "The capture-replace pairs for one --db table, as NAME=SOURCE.",
        long_help = "The capture-replace pairs for one --db table, as NAME=SOURCE. The same thing --capture-replace-pairs (-c) says, but about the table it names. Cannot be combined with --capture-replace-pairs (-c). The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list at all."
    )]
    pub db_capture_replace: Vec<NamedValue>,

    #[arg(
        short = 'f',
        long,
        help = "A file in which families of biological sequences are stored, one family per line.",
        long_help = "A file in which families of biological sequences are stored, one family per line. Each line must have format 'fam-name TAB gene1,gene2,gene3'. Make sure no gene appears in more than one family."
    )]
    pub seq_families: Option<String>,

    #[arg(
        short = 'i',
        long,
        requires = "seq_families",
        help = "A string used as separator in the argument --seq-families (-f) gene families file.",
        long_help = "A string used as separator in the argument --seq-families (-f) gene families file. This string separates the gene-family-identifier (name) from the gene-identifier list that family comprises. Default is '<TAB>' (\"\\t\")."
    )]
    pub seq_family_id_genes_separator: Option<String>,

    #[arg(
        short = 'g',
        long,
        requires = "seq_families",
        help = "A regular expression used to split the list of gene-IDs in a gene-family file.",
        long_help = "A regular expression (Rust syntax) used to split the list of gene-identifiers in the argument --seq-families (-f) gene families file. Default is '(\\s*,\\s*|\\s+)'."
    )]
    pub seq_family_gene_ids_separator: Option<String>,

    #[arg(
        short = 'a',
        long,
        requires = "seq_families",
        help = "If given sequences that are not members of any family will also receive a HRD.",
        long_help = "Use this option only in combination with --seq-families (-f), i.e. when prot-scriber is used to generate human readable descriptions for gene families. If in that context this flag is given, queries for which there are sequence similarity search (Blast) results but that are NOT member of a sequence family will receive an annotation (human readable description) in the output file, too. Default value of this setting is 'OFF' (false)."
    )]
    pub annotate_non_family_queries: bool,

    #[arg(
        short = 'r',
        long,
        help = "A regular expression used to split Blast Hit descriptions into words.",
        long_help = "A regular expression in Rust syntax to be used to split descriptions (`stitle` in Blast terminology) into words. Default is '([()~_\\-/|\\\\;,':.\\s]+)'. Note that this is an expert option."
    )]
    pub description_split_regex: Option<Regex>,

    #[arg(
        short = 'q',
        long,
        value_parser = parse_center_at_quantile,
        help = "Either a number element [0,1] or 50. The quantile or mean to be used for centering.",
        long_help = "The quantile (percentile) to be subtracted from calculated inverse word information content to center these values. Consequently, this must be a value between zero and one or literal 50, which is interpreted as mean instead of a quantile. Default is 50, implying centering at the mean. Note that this is an expert option."
    )]
    pub center_inverse_word_information_content_at_quantile: Option<f64>,

    #[arg(
        short = 'v',
        long,
        long_help = "Print informative messages about the annotation process."
    )]
    pub verbose: bool,

    #[arg(
        short = 'w',
        long,
        help = "File of regular expressions used to identify non informative words.",
        long_help = "The path to a file in which regular expressions (regexs) are stored, one per line. These regexs are used to recognize non-informative words, which will only receive a minimun score in the prot-scriber process that generates human readable description. There is a default list hard-coded into prot-scriber. Write the default out to start from it, with 'prot-scriber defaults non-informative-words-regexs > my_non_informative_words_regexs.txt'; nothing needs downloading, and what you get is the list this binary applies. - Note that this is an expert option."
    )]
    pub non_informative_words_regexs: Option<String>,

    #[arg(
        short = 'd',
        long,
        help = "A file with line pairs of regex and capture group replacement; used in the last step ('polishing') when generating human readable description. Set to 'none' if you want to skip the polishing step.",
        long_help = "The last step of the process generating human readable descriptions (HRDs) for the queries (proteins or sequence families) is to 'polish' the selected HRDs. Polishing is done by iterative application of regular expressions (fancy-regex) and replace instructions (capture-replace-pairs). If you do not want to use the default polishing capture replace pairs specify a file in which pairs of lines are given. Of each pair the first line hold a regular expression (fancy-regex syntax) and the second the replacement instructions providing access to capture groups. Set to 'none' or provide an empty file, if you want to suppress polishing. If you want a template for your custom polishing capture-replace-pairs, write the default out with 'prot-scriber defaults polish-capture-replace-pairs > my_polish_pairs.txt'. - Note that this an expert option."
    )]
    pub polish_capture_replace_pairs: Option<String>,

    #[arg(
        short = 'n',
        long,
        value_parser = parse_n_threads,
        help = "The maximum number of parallel threads to use.",
        long_help = "The maximum number of parallel threads to use. Default is the number of logical cores. Required minimum is two (2). Note that at most one thread is used per input sequence similarity search result (Blast table) file. After parsing these annotation may use up to this number of threads to generate human readable descriptions."
    )]
    pub n_threads: Option<usize>,

    #[arg(
        short = 'x',
        long,
        help = "Exclude results from the output table that could not be annotated.",
        long_help = "Exclude results from the output table that could not be annotated, i.e. 'unknown protein' or 'unknown sequence family', respectively."
    )]
    pub exclude_not_annotated_queries: bool,

}

#[cfg(test)]
mod tests {
    use super::Cli;
    use crate::default::SPLIT_DESCRIPTION_REGEX;
    use clap::CommandFactory;

    /// The long help of an argument that quotes its own default has to quote the real one. This
    /// one had drifted: the help omitted the parentheses and the escaped backslash that the
    /// compiled expression has, so a user copying it out to adapt it got an expression that splits
    /// descriptions differently from the default they meant to start from.
    #[test]
    fn the_help_of_the_split_regex_states_the_default_it_has() {
        let command = Cli::command();
        let argument = command
            .get_arguments()
            .find(|a| a.get_id() == "description_split_regex")
            .expect("--description-split-regex is not among the arguments");
        let help = argument
            .get_long_help()
            .expect("--description-split-regex has no long help")
            .to_string();
        assert!(
            help.contains(SPLIT_DESCRIPTION_REGEX.as_str()),
            "the long help does not state the default it has, {}:\n{}",
            SPLIT_DESCRIPTION_REGEX.as_str(),
            help
        );
    }
}
