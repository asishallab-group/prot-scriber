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
use crate::default::SSSR_TABLE_FIELD_SEPARATOR;
use crate::input::seq_sim_table::Header;
pub use crate::output_writer::OutputFormat;
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
pub struct NamedValue<T = String> {
    /// The name the table was declared under with `--db`.
    pub name: String,
    /// What this option says about that table, in whatever form its value parser checked it into.
    /// `String` where nothing can be decided until a file is read; `char` for `--db-sep`, whose
    /// whole question -- is this one character -- is answerable from the argument alone.
    pub value: T,
}

/// The columns a `--db-header` or `--header` argument names.
///
/// A value parser, so clap reports a header that does not name the three required columns before
/// any table is opened, and names the option itself. `Header::parse` knows WHICH columns a table
/// must have and that Diamond spells two of them differently; that knowledge stays with the table.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_header(arg: &str) -> Result<Header, String> {
    Header::parse(arg)
}

/// One `--db-header NAME=SPEC` argument: which table, and which of its columns hold what.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_named_header(arg: &str) -> Result<NamedValue<Header>, String> {
    let named = parse_named_value(arg)?;
    Ok(NamedValue {
        name: named.name,
        value: Header::parse(&named.value)?,
    })
}

/// The single character a `--db-sep` or `--field-separator` argument names.
///
/// A value parser, so that clap reports a bad one BEFORE any table is opened and names the option
/// itself -- which is the reason this lives here rather than in `input::seq_sim_table`, where it
/// used to be and where it had to be told the name of the flag to blame. There are two flags for
/// this one setting, `--db-sep` under `annotate` and `--field-separator` under `explain`, and a
/// message written down there could only ever name one of them.
///
/// Exactly one character, and it says so when given more. What this used to do was take the first
/// character and drop the rest without a word, which made `'@@'` mean `'@'` and, far worse, made
/// `'\t'` mean the backslash: a TAB is awkward to type into a shell, so the escape is what people
/// reach for, and the table was then never split at all. The spellings a shell makes difficult are
/// therefore understood rather than merely rejected.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_separator(arg: &str) -> Result<char, String> {
    if arg.trim().eq_ignore_ascii_case("default") {
        return Ok(SSSR_TABLE_FIELD_SEPARATOR);
    }
    match arg {
        "\\t" | "tab" | "TAB" => return Ok('\t'),
        "\\s" => return Ok(' '),
        "\\0" => return Ok('\0'),
        _ => {}
    }
    let mut characters = arg.chars();
    match (characters.next(), characters.next()) {
        (Some(separator), None) => Ok(separator),
        (None, _) => Err(String::from(
            "is the empty string. Give the character that separates the fields of the table, or \
             'default' for the TAB character",
        )),
        (Some(_), Some(_)) => Err(format!(
            "is {} characters long, and a field separator is a single character. Write '\\t' or \
             'tab' for the TAB character, '\\s' for a space, '\\0' for the null byte, or 'default' \
             for the TAB character",
            arg.chars().count()
        )),
    }
}

/// One `--db-sep NAME=CHAR` argument: which table, and the character it splits on.
///
/// # Arguments
///
/// * `arg` - The argument value as given on the command line.
fn parse_named_separator(arg: &str) -> Result<NamedValue<char>, String> {
    let named = parse_named_value(arg)?;
    Ok(NamedValue {
        name: named.name,
        value: parse_separator(&named.value)?,
    })
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




/// What prot-scriber was asked to do: a verb, or -- given none -- an annotation run.
#[derive(Parser, Debug)]
#[command(
    name = "prot-scriber",
    // clap 4 dropped its dependency on `textwrap` and wraps to the detected terminal width, which
    // means no wrapping at all when the help is piped or redirected -- as it is when it gets
    // pasted into README.md. Cap it, so the long help stays readable everywhere:
    max_term_width = 100,
    // Put each option's description on its own line rather than beside it. The longest option is
    // `--center-inverse-word-information-content-at-quantile <...>`, and every description was
    // being wrapped into the eight columns left over next to it -- one word per line.
    next_line_help = true,
    // One source of truth: a hand-written string here said 0.1.6 while Cargo.toml said 0.1.5, and
    // the released binary reported the one the package did not have.
    version = concat!("version ", env!("CARGO_PKG_VERSION")),
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
        long_about = "Print one of prot-scriber's built-in regular expression lists, exactly as prot-scriber itself uses it. Without a name, the available lists are listed.\n\nThese are the lists to start from when you want to change how descriptions are processed: write one to a file, edit it, and give it back with the option named beside it. Nothing needs downloading, and there is no version of a list other than the one this binary applies.\n\n  prot-scriber defaults filter-regexs-uniprot > my_filters.txt\n  prot-scriber defaults filter-regexs-uniprot | diff - my_filters.txt\n\nThe table goes to standard output, so it can be redirected or piped."
    )]
    Defaults {
        /// Which list to print. Omit to see what there is.
        #[arg(value_name = "NAME")]
        name: Option<DefaultList>,
    },

    /// Show what prot-scriber makes of a sequence title, step by step.
    #[command(
        long_about = "Show what prot-scriber makes of a sequence title, step by step: which blacklist expression discards it, if one does; which filter expressions delete which parts of it; which capture-replace pairs rewrite it; and what words are left to be scored, with the non-informative ones marked.\n\nThe work is done by the same code an annotation run does it with, so this is a question that can be asked rather than reasoned about.\n\n  prot-scriber explain --stitle \'sp|P12345|ADH1_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana OX=3702 GN=ADH1 PE=1 SV=2\'\n\n  cut -f 3 at_vs_nr.tsv | prot-scriber explain --stitle - --filter @filter-regexs-ncbi-nr\n\nThe rule lists default to prot-scriber\'s own. Give a file, or \'@NAME\' for one of the built-in lists, or \'none\', exactly as the annotation options take them."
    )]
    Explain(Box<ExplainWhat>),

}

/// How a report over a database is written out.
#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplainFormat {
    /// Sections, prose and ranked rows, for a person reading from the top.
    Report,
    /// Every row keyed by its section in the first field, as samtools `stats` is: untruncated and
    /// unranked, because a pipeline wants all of it.
    Tsv,
}

/// What `prot-scriber explain` was asked about, and with which rule lists.
#[derive(clap::Args, Debug)]
pub struct ExplainWhat {
    #[arg(
        long = "stitle",
        value_name = "STITLE",
        help = "A sequence title to explain, or '-' to read them from standard input.",
        long_help = "A sequence title ('stitle' in Blast terminology) to explain, as the third column of a search result table carries it. Repeat the option for more than one. Give a single dash ('-') to read them from standard input, one per line, so that 'cut -f 3 hits.tsv | prot-scriber explain --stitle -' puts a whole search result through the expressions being considered.\n\nA title given here is TRACED, step by step. To ask about a whole database rather than about one title, give --fasta or --table, which report instead."
    )]
    pub stitle: Vec<String>,

    #[arg(
        long = "fasta",
        value_name = "PATH",
        help = "A reference database FASTA to report on, or '-' for standard input.",
        long_help = "A reference database FASTA, every '>' line of which is a title to put through the rules. Repeat the option for more, and give '-' for standard input, so that 'zcat nr.gz | prot-scriber explain --fasta - --filter @filter-regexs-ncbi-nr' asks a whole database what its list makes of it.\n\nThe FASTA rather than a search result is what a list should be judged against: a search result holds only the sequences that got a hit, which is a sample of the database biased towards whatever the query proteome resembles."
    )]
    pub fasta: Vec<String>,

    #[arg(
        long = "table",
        value_name = "PATH",
        help = "A search result table to report on, counted once per subject sequence.",
        long_help = "A sequence similarity search result table whose descriptions are put through the rules, counted ONCE PER SUBJECT SEQUENCE rather than once per row -- a reference sequence's description is one description however many queries found it, and counting per row would make the report a record of the query set rather than of the database. Repeat the option for more; '-' is standard input. One set of seen accessions is kept across all of them, so tables that overlap do not count what they share twice."
    )]
    pub table: Vec<String>,

    #[arg(
        long = "try",
        value_name = "STAGE:EXPRESSION",
        help = "Measure one candidate expression without editing a list. STAGE is blacklist, filter or capture-replace.",
        long_help = "Put one candidate expression through the same pass, layered on top of the list it belongs to, and report what it would do -- what it removed that was meant, what it removed that was not, and what it CREATED, a rule making words as readily as it removes them.\n\nSTAGE is 'blacklist', 'filter' or 'capture-replace', and a capture-replace candidate is written EXPRESSION=>REPLACEMENT. Repeatable; several candidates share the one pass.\n\nThe candidate goes at the END of its list, which is where an edit would most likely put it, and the report says so: the lists are folds, and an expression's position is part of its meaning.\n\nNothing is written. This replaces the build-edit-rebuild-diff loop, which costs two full passes over the reference database and attributes nothing to the rule that caused it."
    )]
    pub try_rule: Vec<String>,

    #[arg(
        long = "baseline",
        value_name = "STAGE=SOURCE",
        help = "A list as it was, run over the same input in the same pass, so an edit can be read as a difference.",
        long_help = "A whole rule list as it stood before, put through the same pass as the one in use, so that what an edit did is one command and one read of the data. STAGE is 'blacklist', 'filter' or 'capture-replace', and SOURCE is a file, an '@NAME' or 'none', exactly as the list options take them.\n\nA difference read this way names the WORDS that moved, which is what an edit is judged by. Comparing the two lists as text says only that they differ, and comparing two separate runs says only that the results differ, with nothing joining a word to the rule that moved it."
    )]
    pub baseline: Vec<String>,

    #[arg(
        long = "format",
        value_name = "FORMAT",
        value_enum,
        default_value_t = ExplainFormat::Report,
        help = "'report' for a person to read, 'tsv' for something else to."
    )]
    pub format: ExplainFormat,

    #[arg(
        long = "sample",
        value_name = "N",
        default_value_t = 1,
        help = "How many titles to show under each row, so a class can be recognised and not guessed."
    )]
    pub sample: usize,

    #[arg(
        long = "rows",
        value_name = "N",
        default_value_t = 25,
        help = "How many rows to print per section. Truncation always says what it hid."
    )]
    pub rows: usize,

    #[arg(
        short = 'o',
        long = "output",
        value_name = "PATH",
        default_value = "-",
        help = "Where to write the report. '-' is standard output."
    )]
    pub output: String,

    #[arg(
        long = "header",
        value_name = "SPEC",
        default_value = "default",
        value_parser = parse_header,
        help = "The columns of --table, named in order, as --db-header takes them."
    )]
    pub header: Header,

    #[arg(
        long = "field-separator",
        value_name = "CHAR",
        default_value = "default",
        value_parser = parse_separator,
        help = "The field separator of --table, as --db-sep takes it."
    )]
    pub field_separator: char,

    #[arg(
        long = "blacklist",
        value_name = "SOURCE",
        default_value = "default",
        help = "The blacklist regular expressions to apply. A file, '@NAME', or 'none'."
    )]
    pub blacklist: String,

    #[arg(
        long = "filter",
        value_name = "SOURCE",
        default_value = "default",
        help = "The filter regular expressions to apply. A file, '@NAME', or 'none'."
    )]
    pub filter: String,

    #[arg(
        long = "capture-replace",
        value_name = "SOURCE",
        default_value = "default",
        help = "The capture-replace pairs to apply. A file, '@NAME', or 'none'."
    )]
    pub capture_replace: String,

    #[arg(
        long = "non-informative-words-regexs",
        value_name = "SOURCE",
        help = "The expressions that recognise a word carrying no information. A file, '@NAME', or 'none'."
    )]
    pub non_informative_words_regexs: Option<String>,

    #[arg(
        long = "description-split-regex",
        value_name = "REGEX",
        help = "The regular expression that splits a description into words."
    )]
    pub description_split_regex: Option<Regex>,
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

/// The heading the gene-family options are gathered under in the help.
///
/// They belong together and nothing else did say so: `--seq-families` (`-f`) is what decides
/// whether a run annotates single sequences or whole families, and the other three describe or
/// extend that -- `clap` already refuses them without it. Grouping them says in the help what the
/// `requires` rules say at the command line.
const FAMILY_HEADING: &str = "Gene families";

/// Every argument prot-scriber accepts. Arguments that may be repeated, once per input sequence
/// similarity search result table, are `Vec`s; optional scalar arguments are `Option`s; flags are
/// `bool`s. A field's type therefore states how often its argument may be given, and hands the
/// rest of the program a value that is already parsed and validated.
#[derive(clap::Args, Debug)]
pub struct Args {
    #[arg(
        short = 'o',
        long,
        required_unless_present = "plan",
        value_name = "PATH",
        help = "Filename in which the tabular output will be stored. Use '-' for standard output.",
        long_help = "Filename in which the tabular output will be stored. Give a single dash ('-') to write the table to standard output instead of to a file. Progress messages, warnings and errors always go to standard error, so the standard output carries the table and nothing else and 'prot-scriber ... -o - | head' shows you its first rows."
    )]
    pub output: Option<String>,

    #[arg(
        short = 's',
        long = "db",
        visible_alias = "seq-sim-table",
        required_unless_present = "plan",
        value_name = "[NAME=]PATH",
        value_parser = parse_table_declaration,
        help = "A database's sequence similarity search results, in tabular format. Give it a name with NAME=PATH.",
        long_help = "File in which to find sequence similarity search results in tabular format (SSST). Use e.g. Blast or Diamond to produce them. Required columns are: 'qacc sacc stitle' (Blast) or 'qseqid sseqid stitle' (Diamond). (See section '2. prot-scriber input preparation' for more details.) If the required columns, or more, appear in different order than shown here you must use the --db-header argument. If any of the input SSSTs uses a different field-separator than the '<TAB>' character, you must provide the --db-sep argument. You can provide multiple SSSTs, simply by repeating the -s argument, e.g. '-s queries_vs_swissprot_diamond_out.txt -s queries_vs_trembl_diamond_out.txt'. All rows belonging to one query must stand together in the table, which is what Blast and Diamond produce on their own; concatenating tables or shuffling one does not preserve it, and prot-scriber stops with an error rather than annotate a query twice. 'sort -s -t\"<TAB>\" -k1,1 <table>' restores it, and being a stable sort on the query column alone it leaves the order of each query's hits alone; --unsorted-input reads such a table as it is instead, at the cost of memory.\n\nGive a table a name with NAME=PATH, e.g. '--db nr=at_vs_nr.tsv', and the --db-header, --db-sep, --db-blacklist, --db-filter and --db-capture-replace options can then say which table they are for by that name instead of by the order they are written in. Without a name a table is called after its file, so '--db at_vs_nr.tsv' is the table 'at_vs_nr'."
    )]
    pub seq_sim_table: Vec<NamedValue>,

    #[arg(
        long = "db-header",
        value_name = "NAME=SPEC",
        value_parser = parse_named_header,
        help = "The header of one --db table, as NAME=SPEC.",
        long_help = "The header of one --db table, as NAME=SPEC, e.g. '--db-header nr=\"qacc sacc evalue stitle\"'. Separated by spaces, the names of the columns in the order they appear in that table. The required columns are 'qacc sacc stitle'; Blast and Diamond terminology are both understood, so write 'qacc' and 'sacc' or Diamond's 'qseqid' and 'sseqid', whichever your search produced. Additional columns are ignored and the required ones may appear in any order -- what this argument does is say which column is which."
    )]
    pub db_header: Vec<NamedValue<Header>>,

    #[arg(
        long = "db-sep",
        value_name = "NAME=CHAR",
        value_parser = parse_named_separator,
        help = "The field separator of one --db table, as NAME=CHAR.",
        long_help = "The field separator of one --db table, as NAME=CHAR, e.g. '--db-sep nr=\\t'. The default is the TAB character. A separator is a single character; write '\\t' or 'tab' for TAB, '\\s' for a space and '\\0' for the null byte, since a shell makes those awkward to type literally."
    )]
    pub db_sep: Vec<NamedValue<char>>,

    #[arg(
        long = "db-blacklist",
        value_name = "NAME=SOURCE",
        value_parser = parse_named_value,
        help = "The blacklist regular expressions for one --db table, as NAME=SOURCE.",
        long_help = "The blacklist regular expressions for one --db table, as NAME=SOURCE. A hit description matching any of them is discarded whole. The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list at all."
    )]
    pub db_blacklist: Vec<NamedValue>,

    #[arg(
        long = "db-filter",
        value_name = "NAME=SOURCE",
        value_parser = parse_named_value,
        help = "The filter regular expressions for one --db table, as NAME=SOURCE.",
        long_help = "The filter regular expressions for one --db table, as NAME=SOURCE, e.g. '--db-filter nr=@filter-regexs-ncbi-nr'. Substrings matching any of them are deleted from a hit description before it is scored. It names the table it belongs to, so it can only ever mean the table declared '--db nr=...', whatever order the arguments are written in. The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list at all."
    )]
    pub db_filter: Vec<NamedValue>,

    #[arg(
        long = "db-capture-replace",
        value_name = "NAME=SOURCE",
        value_parser = parse_named_value,
        help = "The capture-replace pairs for one --db table, as NAME=SOURCE.",
        long_help = "The capture-replace pairs for one --db table, as NAME=SOURCE: pairs of lines, an expression and the replacement below it, rewriting a hit description before it is scored. The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list at all."
    )]
    pub db_capture_replace: Vec<NamedValue>,

    #[arg(
        short = 'f',
        long,
        value_name = "PATH",
        help_heading = FAMILY_HEADING,
        help = "A file in which families of biological sequences are stored, one family per line.",
        long_help = "A file in which families of biological sequences are stored, one family per line. Each line must have format 'fam-name TAB gene1,gene2,gene3'. Make sure no gene appears in more than one family."
    )]
    pub seq_families: Option<String>,

    #[arg(
        short = 'i',
        long,
        value_name = "STRING",
        help_heading = FAMILY_HEADING,
        requires = "seq_families",
        help = "A string used as separator in the argument --seq-families (-f) gene families file.",
        long_help = "A string used as separator in the argument --seq-families (-f) gene families file. This string separates the gene-family-identifier (name) from the gene-identifier list that family comprises. Default is '<TAB>' (\"\\t\")."
    )]
    pub seq_family_id_genes_separator: Option<String>,

    #[arg(
        short = 'g',
        long,
        value_name = "REGEX",
        help_heading = FAMILY_HEADING,
        requires = "seq_families",
        help = "A regular expression used to split the list of gene-IDs in a gene-family file.",
        long_help = "A regular expression (Rust syntax) used to split the list of gene-identifiers in the argument --seq-families (-f) gene families file. Default is '(\\s*,\\s*|\\s+)'."
    )]
    pub seq_family_gene_ids_separator: Option<Regex>,

    #[arg(
        short = 'a',
        long,
        help_heading = FAMILY_HEADING,
        requires = "seq_families",
        help = "If given sequences that are not members of any family will also receive a HRD.",
        long_help = "Use this option only in combination with --seq-families (-f), i.e. when prot-scriber is used to generate human readable descriptions for gene families. If in that context this flag is given, queries for which there are sequence similarity search (Blast) results but that are NOT member of a sequence family will receive an annotation (human readable description) in the output file, too. Default value of this setting is 'OFF' (false)."
    )]
    pub annotate_non_family_queries: bool,

    #[arg(
        short = 'r',
        long,
        value_name = "REGEX",
        help = "A regular expression used to split Blast Hit descriptions into words.",
        long_help = "A regular expression in Rust syntax to be used to split descriptions (`stitle` in Blast terminology) into words. Default is '([()\\[\\]{}<>+*^_\\-/|\\\\;,':.\\s]+)'. Note that this is an expert option."
    )]
    pub description_split_regex: Option<Regex>,

    #[arg(
        short = 'q',
        long,
        value_name = "QUANTILE",
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
        value_name = "SOURCE",
        help = "Regular expressions used to identify non informative words. A file, '@NAME', or 'none'.",
        long_help = "Regular expressions (regexs) used to recognize non-informative words, which will only receive a minimun score in the prot-scriber process that generates human readable description. The value is a file with one expression per line, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME' is the same list -- or 'none' to hold no word non-informative at all. There is a default list hard-coded into prot-scriber. Write it out to start from it, with 'prot-scriber defaults non-informative-words-regexs > my_non_informative_words_regexs.txt'; nothing needs downloading, and what you get is the list this binary applies. - Note that this is an expert option."
    )]
    pub non_informative_words_regexs: Option<String>,

    #[arg(
        short = 'd',
        long,
        value_name = "SOURCE",
        help = "A file with line pairs of regex and capture group replacement; used in the last step ('polishing') when generating human readable description. Set to 'none' if you want to skip the polishing step.",
        long_help = "The last step of the process generating human readable descriptions (HRDs) for the queries (proteins or sequence families) is to 'polish' the selected HRDs. Polishing is done by iterative application of regular expressions (fancy-regex) and replace instructions (capture-replace-pairs). If you do not want to use the default polishing capture replace pairs specify a file in which pairs of lines are given. Of each pair the first line hold a regular expression (fancy-regex syntax) and the second the replacement instructions providing access to capture groups. Set to 'none' or provide an empty file, if you want to suppress polishing, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is. If you want a template for your custom polishing capture-replace-pairs, write the default out with 'prot-scriber defaults polish-capture-replace-pairs > my_polish_pairs.txt'. - Note that this an expert option."
    )]
    pub polish_capture_replace_pairs: Option<String>,

    #[arg(
        short = 'n',
        long,
        value_name = "N",
        value_parser = parse_n_threads,
        help = "The maximum number of parallel threads to use.",
        long_help = "The maximum number of parallel threads to use. Default is the number of logical cores. Required minimum is two (2). Note that at most one thread is used per input sequence similarity search result (Blast table) file. After parsing these annotation may use up to this number of threads to generate human readable descriptions."
    )]
    pub n_threads: Option<usize>,

    #[arg(
        long = "plan",
        value_name = "PATH",
        conflicts_with_all = [
            "seq_sim_table", "db_header", "db_sep", "db_blacklist",
            "db_filter", "db_capture_replace", "seq_families", "seq_family_id_genes_separator",
            "seq_family_gene_ids_separator", "annotate_non_family_queries",
            "description_split_regex", "center_inverse_word_information_content_at_quantile",
            "non_informative_words_regexs", "polish_capture_replace_pairs", "n_threads",
            "exclude_not_annotated_queries", "unsorted_input", "output", "plan_out",
        ],
        help = "Run again exactly what a run plan records.",
        long_help = "Run again exactly what the given run plan records: the same input tables, the same regular expressions written out in it, the same everything. It cannot be combined with any option that would configure the run, because then there would be two answers to one question and a rule about which of them wins -- and a precedence rule is a thing you have to know to read a command line. Edit the plan if you want something else; that is what it is for."
    )]
    pub plan: Option<String>,

    #[arg(
        long = "var",
        value_name = "NAME=VALUE",
        value_parser = parse_named_value,
        requires = "plan",
        help = "Fill in a ${NAME} placeholder in a run plan's paths.",
        long_help = "Fill in a ${NAME} placeholder in a run plan's paths, e.g. '--var sample=at' for a plan whose table is '${sample}/hits.tsv'. One plan then serves a whole set of datasets that are annotated the same way, which is the usual reason to have written a plan down at all.\n\nOnly paths are filled in: the input tables, the gene families file and the output. Not the regular expressions, where '${name}' is the ordinary way fancy-regex names a capture group and a blind substitution would quietly rewrite them.\n\nA placeholder left with nothing to fill it, and a --var that filled nothing in, are both errors: the first would read a file called '${sample}' and the second is a misspelling that would otherwise do nothing at all."
    )]
    pub var: Vec<NamedValue>,

    #[arg(
        long = "plan-out",
        value_name = "PATH",
        help = "Where to write the record of this run. Default is the output file with '.plan.toml' after it.",
        long_help = "Where to write the record of this run: every setting it resolved to, the regular expressions written out rather than named, and a BLAKE3 hash of every byte it read. Replay it with --plan. By default it is the --output (-o) file with '.plan.toml' after it; give 'none' to write no plan at all, and note that no plan is written by default when the table goes to standard output, there being no file name to derive one from.\n\nA command line is not a record of a run: it names files whose contents change, it says '@filter-regexs-ncbi-nr' where what matters is the expressions that name stood for on the day, and it leaves out everything that was defaulted. A plan is what you commit beside a result."
    )]
    pub plan_out: Option<String>,

    #[arg(
        long = "dry-run",
        help = "Resolve and check the command line, report what would be done, and stop.",
        long_help = "Resolve and check the command line, report what would be done, and stop without annotating anything. Everything that can be found out before reading the input tables is found out: that every argument can be paired with the table it is for, that every file of regular expressions exists and parses, that every input table exists and how large it is. The report says which settings each table would be parsed with, and whether each of them is prot-scriber's default or came from the command line. Meant to be the step before submitting a long run, so that an hour is not spent discovering a mistake that was visible at the start."
    )]
    pub dry_run: bool,

    #[arg(
        long = "unsorted-input",
        help = "Read input tables whose rows are not grouped by query.",
        long_help = "Read input tables whose rows are not grouped by query. prot-scriber normally annotates each query as soon as its rows are behind it, which is what keeps the memory a run needs independent of how large the input is: a query's hits are dropped the moment it is annotated. That requires a query's rows to stand together, which is what Blast and Diamond produce and what concatenating tables destroys. Given this flag, prot-scriber holds every query until all input has been read instead, and so needs memory in proportion to the whole input rather than to one query. Prefer grouping the table -- 'sort -s -t\"<TAB>\" -k1,1 table' does it, and preserves the order of each query's hits -- and keep this for when that is not possible."
    )]
    pub unsorted_input: bool,

    #[arg(
        long = "format",
        value_name = "FORMAT",
        value_enum,
        default_value = "tsv",
        help = "The shape of the output table.",
        long_help = "The shape of the output table.\n\n'tsv' is the identifier and the description, which is what prot-scriber has always written.\n\n'tsv-scored' adds what the description scored, how many hit descriptions it was chosen from and how many distinct phrases were proposed, so that a result can be sorted or thresholded by how well founded it is.\n\n'jsonl' writes one JSON object per annotee holding the whole account of how its description was chosen -- the same thing --explain writes for a few annotees, for all of them, and machine readable. It is written as the run produces it, which is what keeps the memory a run needs independent of the size of its input; its rows are therefore in the order the annotations happened rather than sorted by identifier, and 'sort' after the fact gives a byte-stable file, each row standing on its own."
    )]
    pub format: OutputFormat,

    #[arg(
        long = "explain",
        value_name = "ID",
        value_delimiter = ',',
        help = "Say why these queries or families got the description they got.",
        long_help = "Say why these queries or families got the description they got, and not another one. Give the identifiers that appear in the output table: query identifiers ('qacc' in the input tables), or -- with --seq-families (-f) -- the names of families. Repeat the option or separate them with commas.\n\nWhat is written is the whole of the choice: every hit description that was scored and the words it was split into, what each informative word was worth and how often it appeared, every phrase that was proposed and its score, and which of them won. It is written as each annotee is finished and goes to standard output; --explain-out sends it to a file instead.\n\nAn identifier that was never annotated is an error rather than a silence, a misspelled one being otherwise indistinguishable from a query prot-scriber could say nothing about."
    )]
    pub explain: Vec<String>,

    #[arg(
        long = "explain-out",
        value_name = "PATH",
        requires = "explain",
        help = "Write the --explain output to this file instead of to standard output.",
        long_help = "Write the --explain output to this file instead of to standard output. The file is created when the run starts rather than when it ends, so a path that cannot be written is reported before the annotation rather than after it."
    )]
    pub explain_out: Option<String>,

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
    use super::{parse_named_separator, parse_separator, SSSR_TABLE_FIELD_SEPARATOR};
    use crate::default::SPLIT_DESCRIPTION_REGEX;

    #[test]
    fn a_separator_is_exactly_one_character() {
        assert_eq!(parse_separator("@").unwrap(), '@');
        // A character outside the basic multilingual plane is still one character:
        assert_eq!(parse_separator("\u{1F600}").unwrap(), '\u{1F600}');
        for rejected in ["@@", "  ", "\\t\\t", "qacc sacc"] {
            let refused = parse_separator(rejected).unwrap_err();
            assert!(
                refused.contains("single character"),
                "{:?} was refused without saying why: {}",
                rejected,
                refused
            );
            // clap prefixes this with `invalid value '@@' for '--db-sep <NAME=CHAR>':`, which is
            // the whole reason the check lives here: there are two flags for this one setting and
            // neither the table nor this function has to know which was written.
            assert!(
                !refused.contains("--"),
                "{:?}'s message names an option, which is clap's to say: {}",
                rejected,
                refused
            );
        }
        assert!(parse_separator("").unwrap_err().contains("empty string"));
    }

    #[test]
    fn the_awkward_separators_can_be_spelled_out() {
        for (spelling, expected) in [
            ("\\t", '\t'),
            ("tab", '\t'),
            ("TAB", '\t'),
            ("\\s", ' '),
            ("\\0", '\0'),
            ("default", SSSR_TABLE_FIELD_SEPARATOR),
            ("Default", SSSR_TABLE_FIELD_SEPARATOR),
        ] {
            assert_eq!(parse_separator(spelling).unwrap(), expected, "{:?}", spelling);
        }
    }

    #[test]
    fn a_named_separator_carries_the_table_it_is_for() {
        let named = parse_named_separator("nr=@").unwrap();
        assert_eq!(named.name, "nr");
        assert_eq!(named.value, '@');
        // The NAME is checked before the CHAR, so a malformed pair is not reported as a bad
        // separator:
        assert!(parse_named_separator("nr").unwrap_err().contains("NAME=VALUE"));
        assert!(parse_named_separator("nr=@@").unwrap_err().contains("single character"));
    }

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
