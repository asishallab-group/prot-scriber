//! End-to-end tests that run the compiled `prot-scriber` binary the way a user does: as a
//! process, with command line arguments, checked by its exit status, its streams and the files it
//! leaves behind. The unit tests inside `src/` call `run(Args::parse_from(..))` in-process and so
//! cannot observe an exit code at all -- which is precisely how a run that panics on a worker
//! thread, or fails to write its output, has been able to report success for years.
//!
//! These are *characterization* tests: they pin down what prot-scriber does today, not what it
//! ought to do. Where today's behaviour is wrong, the test says so in its name and carries a
//! `TODO(stage-0)` naming the change that will invalidate it, so the commit that fixes the bug is
//! easy to find from here and vice versa.
//!
//! No test dependencies are used beyond what the crate already declares; the few helpers below
//! replace them. Every test gets its own scratch directory under Cargo's target directory, keyed
//! by test name and process id, because `cargo test` runs these functions on parallel threads.

use pretty_assertions::assert_eq;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs the binary Cargo built for this test with the given arguments and captures its result.
/// The working directory is pinned to the crate root so that a test's outcome does not depend on
/// where `cargo test` was invoked from.
fn prot_scriber(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_prot-scriber"))
        .args(args)
        .current_dir(crate_root())
        .output()
        .expect("failed to execute the prot-scriber binary")
}

/// The root of this crate, i.e. the directory holding `Cargo.toml`.
fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The absolute path of one of the test fixtures shipped in `misc/`.
///
/// # Arguments
///
/// * `file_name` - The name of the file within `misc/`.
fn fixture(file_name: &str) -> PathBuf {
    crate_root().join("misc").join(file_name)
}

/// A private, empty directory to write outputs into, removed again when the test ends. It lives
/// under Cargo's target directory, so it is cleaned by `cargo clean` and never pollutes `misc/`.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    /// Creates the scratch directory for a single test.
    ///
    /// # Arguments
    ///
    /// * `test_name` - A name unique within this test binary; it becomes the directory name,
    ///   together with the process id, so that tests running in parallel -- and two `cargo test`
    ///   invocations running at once -- cannot write to each other's files.
    fn new(test_name: &str) -> Scratch {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("{}-{}", test_name, std::process::id()));
        // A previous run that was killed before its `Drop` could leave this behind:
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("could not create the scratch directory");
        Scratch { dir }
    }

    /// The path of a file inside the scratch directory. The file itself is not created.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the file within the scratch directory.
    fn path(&self, file_name: &str) -> PathBuf {
        self.dir.join(file_name)
    }

    /// Creates a file inside the scratch directory and returns its path. Input a test constructs
    /// for itself belongs here rather than in `misc/`, which holds the fixtures whose bytes the
    /// characterization tests pin down.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the file within the scratch directory.
    /// * `content` - What to write into it.
    fn write(&self, file_name: &str, content: &str) -> PathBuf {
        let path = self.path(file_name);
        fs::write(&path, content).unwrap_or_else(|e| panic!("could not write {:?}: {}", path, e));
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// The whole content of a text file.
///
/// # Arguments
///
/// * `path` - The file to read.
fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("could not read {:?}: {}", path, e))
}

/// The captured standard error of a run, as a string.
///
/// # Arguments
///
/// * `output` - The result of a `prot_scriber` call.
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The captured standard output of a run, as a string.
///
/// # Arguments
///
/// * `output` - The result of a `prot_scriber` call.
fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Asserts that a run which failed did so as a diagnosed error rather than as a crash: no Rust
/// panic message reached the user, no `RUST_BACKTRACE` note asked them to debug prot-scriber, and
/// nothing was written to standard output, which carries data and only data.
///
/// # Arguments
///
/// * `output` - The result of a `prot_scriber` call that is expected to have failed.
fn assert_no_panic_reached_the_user(output: &Output) {
    let err = stderr(output);
    assert!(
        !err.contains("panicked"),
        "the user was shown a Rust panic:\n{}",
        err
    );
    assert!(
        !err.contains("RUST_BACKTRACE"),
        "the user was asked to set RUST_BACKTRACE:\n{}",
        err
    );
    assert_eq!(
        stdout(output),
        "",
        "a diagnostic was written to standard output, which must carry data only"
    );
}

#[test]
fn swissprot_and_trembl_fixtures_reproduce_the_shipped_protein_hrds() {
    let scratch = Scratch::new("protein-hrds");
    let out = scratch.path("Twelve_Proteins_HRDs.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-s"),
        trembl.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "annotating the two shipped tables failed:\n{}",
        stderr(&result)
    );
    assert_eq!(read(&out), read(&fixture("Twelve_Proteins_HRDs.txt")));
}

#[test]
fn family_mode_reproduces_the_shipped_family_hrds() {
    let scratch = Scratch::new("family-hrds");
    let out = scratch.path("family_HRDs.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");
    let families = fixture("families.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-s"),
        trembl.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "annotating the shipped gene families failed:\n{}",
        stderr(&result)
    );
    assert_eq!(read(&out), read(&fixture("family_HRDs.txt")));
}

#[test]
fn output_rows_are_sorted_by_annotee_identifier() {
    let scratch = Scratch::new("sorted-rows");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");

    // The rows come out of a `HashMap`, and a `HashMap` with few keys can be iterated in sorted
    // order by chance, so one run proves nothing. Each process seeds its own hasher, which makes
    // these runs independent draws: every one of them has to be sorted.
    let mut runs: Vec<String> = Vec::new();
    for run in 1..=5 {
        let out = scratch.path(&format!("hrds_{}.txt", run));
        let result = prot_scriber(&[
            OsStr::new("-s"),
            swissprot.as_os_str(),
            OsStr::new("-s"),
            trembl.as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
        ]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "run {} failed:\n{}",
            run,
            stderr(&result)
        );

        let content = read(&out);
        let mut lines = content.lines();
        assert_eq!(
            lines.next(),
            Some("Annotee-Identifier\tHuman-Readable-Description"),
            "run {} did not begin with the header line",
            run
        );

        let identifiers: Vec<&str> = lines
            .map(|line| line.split('\t').next().unwrap_or(line))
            .collect();
        let mut expected = identifiers.clone();
        expected.sort_unstable();
        assert_eq!(
            identifiers, expected,
            "run {} wrote its rows in an order that is not sorted by annotee identifier",
            run
        );

        runs.push(content);
    }

    // Sorted rows are the same rows in the same places, run after run: the point of fixing this
    // is that a re-run of an unchanged analysis produces the same file.
    for (i, content) in runs.iter().enumerate().skip(1) {
        assert_eq!(
            content, &runs[0],
            "run {} differs from run 1 although nothing about the analysis changed",
            i + 1
        );
    }
}

#[test]
fn help_exits_zero_in_both_its_short_and_long_form() {
    for flag in ["-h", "--help"] {
        let result = prot_scriber(&[OsStr::new(flag)]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "{} did not exit with zero",
            flag
        );
        assert!(
            stdout(&result).contains("Usage: prot-scriber"),
            "{} printed no usage line, stdout was:\n{}",
            flag,
            stdout(&result)
        );
    }
}

#[test]
fn version_exits_zero_and_names_the_program() {
    let result = prot_scriber(&[OsStr::new("--version")]);
    assert_eq!(result.status.code(), Some(0), "--version did not exit zero");
    assert!(
        stdout(&result).starts_with("prot-scriber"),
        "--version printed {:?}",
        stdout(&result)
    );
}

#[test]
fn a_missing_required_argument_is_a_usage_error() {
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // No --output:
    let no_output = prot_scriber(&[OsStr::new("-s"), swissprot.as_os_str()]);
    assert_eq!(no_output.status.code(), Some(2));
    assert!(
        stderr(&no_output).contains("--output"),
        "stderr did not name the missing argument:\n{}",
        stderr(&no_output)
    );

    // No arguments whatsoever:
    let nothing = prot_scriber(&[]);
    assert_eq!(nothing.status.code(), Some(2));
    assert!(
        stderr(&nothing).contains("--seq-sim-table"),
        "stderr did not name the missing argument:\n{}",
        stderr(&nothing)
    );
}

#[test]
fn a_nonexistent_gene_family_file_is_a_usage_error() {
    let scratch = Scratch::new("missing-family-file");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let missing = scratch.path("there_is_no_such_family_file.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-f"),
        missing.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    // A path that is not there is a mistake in the command line, and the user fixes it there:
    // that is a usage error, not an I/O failure of a file prot-scriber found and then could not
    // read. The diagnostic has to name the path, because "no such file" without it is useless in
    // a run with half a dozen file arguments.
    assert_eq!(
        result.status.code(),
        Some(2),
        "a missing gene family file did not exit 2, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("there_is_no_such_family_file.txt"),
        "stderr did not name the missing file:\n{}",
        stderr(&result)
    );
    assert!(!out.exists(), "a failed run left an output file behind");
}

#[test]
fn a_nonexistent_sequence_similarity_table_is_a_usage_error() {
    let scratch = Scratch::new("missing-seq-sim-table");
    let out = scratch.path("hrds.txt");
    let missing = scratch.path("there_is_no_such_table.tsv");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        missing.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    // The worst of the exit code defects and the reason this harness runs the binary as a
    // process: the table is opened on a parser thread, so a panic there kills that thread only,
    // `main` carries on, annotates nothing and exits 0. A caller checking the exit status is told
    // the run succeeded, and -- since an empty result is now a header-only file -- is handed an
    // output table that looks like a legitimately empty analysis. The failure has to reach the
    // exit status.
    assert_eq!(
        result.status.code(),
        Some(2),
        "an input table that does not exist did not exit 2, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("there_is_no_such_table.tsv"),
        "stderr did not name the missing table:\n{}",
        stderr(&result)
    );
    assert!(
        !out.exists(),
        "a run that could not read its input still wrote an output table that looks like an \
         empty analysis"
    );
}

#[test]
fn a_run_that_annotates_nothing_still_writes_a_header_only_output_file() {
    let scratch = Scratch::new("annotates-nothing");
    let out = scratch.path("hrds.txt");

    // An input table that parses -- three columns in the default order, separated by TABs -- but
    // that yields nothing. Every hit description of its single query matches the default
    // blacklist, so no description survives, the query never reaches the annotation process and
    // there is no annotation to report. A real run reaches this state whenever a small query set
    // finds only uninformative hits, or when every row was excluded by a filter list.
    let table = scratch.write(
        "only_blacklisted_hits.tsv",
        "Query-1\tHit-1\thypothetical protein\n\
         Query-1\tHit-2\tuncharacterized protein\n",
    );

    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "annotating a table with no usable descriptions is an empty result, not a failure:\n{}",
        stderr(&result)
    );

    // An empty result and a run that died must not look alike to the caller. A pipeline step that
    // asks `os.path.exists` before reading the table takes the wrong branch if the successful run
    // left nothing behind.
    assert!(
        out.exists(),
        "nothing was annotated and no output file was written at all: {:?} does not exist",
        out
    );
    assert_eq!(
        read(&out),
        "Annotee-Identifier\tHuman-Readable-Description\n",
        "an empty result should be exactly the header line"
    );
}

#[test]
fn an_unwritable_output_path_exits_seventy_four() {
    let scratch = Scratch::new("unwritable-output");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    // A path inside a directory that does not exist. This is the one way to make a write fail
    // that behaves the same on every platform and needs no permission games, which matter on the
    // Windows runner and when the suite runs as root in a container.
    let out = scratch.path("no_such_directory").join("hrds.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    // 74 is EX_IOERR, the I/O code of the exit status taxonomy. A run whose entire product could
    // not be stored has not succeeded, and the caller has to learn that from the exit status --
    // it is the one thing every shell, Make rule and workflow step already checks.
    assert_eq!(
        result.status.code(),
        Some(74),
        "a failed write did not exit 74, stderr was:\n{}",
        stderr(&result)
    );
    assert!(
        stderr(&result).contains("an error occurred when attempting to write output"),
        "stderr was:\n{}",
        stderr(&result)
    );
    assert!(!out.exists());
}

#[test]
fn the_gene_family_separator_is_silently_ignored_without_seq_families() {
    let scratch = Scratch::new("family-separator-ignored");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let plain_out = scratch.path("plain.txt");
    let with_separator_out = scratch.path("with_separator.txt");

    let plain = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-o"),
        plain_out.as_os_str(),
    ]);
    assert_eq!(plain.status.code(), Some(0), "{}", stderr(&plain));

    // -i configures the gene family file format, but no gene family file is given.
    let with_separator = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-i"),
        OsStr::new(";"),
        OsStr::new("-o"),
        with_separator_out.as_os_str(),
    ]);

    // TODO(stage-0): -a already `requires` --seq-families and so is rejected with exit 2; -i and
    // -g are not, and are accepted and then ignored. They should carry the same `requires`, which
    // turns this into a usage error.
    assert_eq!(
        with_separator.status.code(),
        Some(0),
        "-i without -f is no longer accepted -- if it is now exit 2, that is the fix"
    );
    assert_eq!(
        read(&with_separator_out),
        read(&plain_out),
        "-i changed the result of a run that has no gene families"
    );

    // The contrast: the same mistake made with -a is caught.
    let annotate_non_family = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-a"),
        OsStr::new("-o"),
        scratch.path("unused.txt").as_os_str(),
    ]);
    assert_eq!(annotate_non_family.status.code(), Some(2));
}

// ---------------------------------------------------------------------------------------------
// The error classes a user reaches by getting an argument or an input file wrong. Each of them is
// a `panic!`, an `unwrap` or an index out of bounds today, so each is exit 101 -- or, when the
// panic happens on a parsing thread, exit 0 -- with a `panicked at src/...` line and a
// `RUST_BACKTRACE` note. The messages themselves are good; what is wrong is that they arrive as a
// crash. The taxonomy these tests assert is: 2 usage error, 3 malformed input data, 74 I/O error.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_per_table_argument_given_once_for_two_tables_is_a_usage_error() {
    let scratch = Scratch::new("per-table-count-mismatch");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");

    // Two input tables, but only one --filter-regexs. The five per-table arguments are paired
    // with --seq-sim-table by position, so prot-scriber cannot know which of the two tables this
    // one was meant for and refuses to guess. This is the check that upstream issue #31 was
    // about: the message is right, arriving as a panic is not.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-s"),
        trembl.as_os_str(),
        OsStr::new("-l"),
        OsStr::new("default"),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(2),
        "a per-table argument count mismatch did not exit 2, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("--filter-regexs (-l)"),
        "stderr did not name the offending argument:\n{}",
        stderr(&result)
    );
}

#[test]
fn a_header_missing_a_required_column_is_a_usage_error() {
    let scratch = Scratch::new("header-missing-column");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // 'stitle' is missing, and it is the column the whole program exists to read.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-e"),
        OsStr::new("qacc sacc evalue"),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(2),
        "a --header without 'stitle' did not exit 2, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("stitle"),
        "stderr did not name the missing column:\n{}",
        stderr(&result)
    );
}

#[test]
fn an_input_table_not_sorted_by_query_identifier_is_malformed_input() {
    let scratch = Scratch::new("unsorted-input-table");
    let out = scratch.path("hrds.txt");

    // prot-scriber parses in a stream and considers a query finished as soon as the query
    // identifier changes, so it requires its input sorted by that identifier. Here 'Query-1'
    // comes back after 'Query-2' has already been seen, which means results for 'Query-1' would
    // be silently split into two annotations. That is a property of the input file, not of the
    // command line: it is malformed input data.
    let table = scratch.write(
        "unsorted.tsv",
        "Query-1\tHit-1\talpha kinase\n\
         Query-2\tHit-2\tbeta phosphatase\n\
         Query-1\tHit-3\tgamma kinase\n",
    );

    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(3),
        "an unsorted input table did not exit 3, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("sorted by query identifiers"),
        "stderr did not explain that the input must be sorted:\n{}",
        stderr(&result)
    );
}

#[test]
fn a_regex_file_that_does_not_exist_is_a_usage_error() {
    let scratch = Scratch::new("missing-regex-file");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let missing = scratch.path("there_is_no_such_regex_file.txt");

    // Every argument that takes a file of regular expressions reaches the same reader, so they
    // all have to fail the same way. --non-informative-words-regexs (-w) is a global one and
    // --blacklist-regexs (-b) a per-table one, which are two different code paths into it.
    for flag in ["-w", "-b"] {
        let out = scratch.path(&format!("hrds{}.txt", flag));
        let result = prot_scriber(&[
            OsStr::new("-s"),
            swissprot.as_os_str(),
            OsStr::new(flag),
            missing.as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
        ]);

        assert_eq!(
            result.status.code(),
            Some(2),
            "{} naming a file that does not exist did not exit 2, stderr was:\n{}",
            flag,
            stderr(&result)
        );
        assert_no_panic_reached_the_user(&result);
        assert!(
            stderr(&result).contains("there_is_no_such_regex_file.txt"),
            "stderr did not name the missing file:\n{}",
            stderr(&result)
        );
    }
}

#[test]
fn a_malformed_gene_family_line_is_malformed_input() {
    let scratch = Scratch::new("malformed-gene-family");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // The family identifier is separated from its gene list by a TAB, and this line uses a pipe,
    // so the line has one field where two are required. The file exists and is readable: what is
    // wrong is its content.
    let families = scratch.write("families.txt", "OG0000001|gene-1,gene-2\n");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(3),
        "a malformed gene family line did not exit 3, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("families.txt"),
        "stderr did not name the offending file:\n{}",
        stderr(&result)
    );
}

#[test]
fn a_field_separator_that_does_not_split_the_table_is_malformed_input() {
    let scratch = Scratch::new("wrong-field-separator");
    let out = scratch.path("hrds.txt");
    let table = scratch.write("hits.tsv", "Query-1\tHit-1\talpha kinase\n");

    // The table is separated by TABs and the user says semicolon, so every line collapses into a
    // single field and the columns prot-scriber needs are not there. Today this is an index out
    // of bounds on a parsing thread, which leaves the run reporting success.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-p"),
        OsStr::new(";"),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(3),
        "a table without the required columns did not exit 3, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("hits.tsv"),
        "stderr did not name the table it could not parse:\n{}",
        stderr(&result)
    );
}

#[test]
fn an_empty_field_separator_is_a_usage_error() {
    let scratch = Scratch::new("empty-field-separator");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // There is no such thing as an empty field separator; today the empty string is unwrapped as
    // if it had a first character.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-p"),
        OsStr::new(""),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(2),
        "an empty --field-separator did not exit 2, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
}

#[test]
fn a_gene_id_separator_that_is_not_a_regular_expression_is_a_usage_error() {
    let scratch = Scratch::new("bad-gene-id-separator");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let families = scratch.write("families.txt", "OG0000001\tgene-1,gene-2\n");

    // -g takes a regular expression; an unclosed group is not one.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("-g"),
        OsStr::new("("),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(2),
        "an invalid -g regular expression did not exit 2, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
}

#[test]
fn a_successful_run_writes_nothing_to_standard_output() {
    let scratch = Scratch::new("stdout-is-data-only");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // The result of a run is the output table. Progress reports are diagnostics and belong on
    // standard error, so that `prot-scriber ... -o -` can one day write the table itself to
    // standard output, and so that a caller piping prot-scriber never has to filter its data.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-v"),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    assert_eq!(
        stdout(&result),
        "",
        "a verbose run wrote its progress messages to standard output"
    );
    assert!(
        stderr(&result).contains("output written to file"),
        "the verbose progress messages did not go to standard error:\n{}",
        stderr(&result)
    );
}

#[test]
fn input_tables_that_yielded_no_record_at_all_exit_four() {
    let scratch = Scratch::new("no-records-parsed");
    let out = scratch.path("hrds.txt");

    // Nothing in this table can be read as a hit, so the annotation process is handed nothing to
    // work with and the output table can only ever be its header line. That is the shape of the
    // 07.08.2026 incident that started this redesign: a run that is wrong from its first line
    // and reports success anyway. It has to arrive at the exit status.
    //
    // An empty file is how a run reaches this state today, and it is not an exotic one: a search
    // step that died before writing, a truncated copy, a wildcard that expanded to the wrong
    // name. The other route -- a --field-separator or a --header that makes the columns
    // unreadable -- is caught one line earlier and exits 3, because a line that splits into too
    // few fields is malformed input. What is left over for exit 4 is the table that holds no
    // line to misread.
    let empty = scratch.write("no_hits_at_all.tsv", "");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        empty.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(4),
        "a run that parsed no record did not exit 4, stderr was:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("no_hits_at_all.tsv"),
        "stderr did not name the table that yielded nothing:\n{}",
        stderr(&result)
    );
    assert!(
        !out.exists(),
        "a run that read nothing usable still wrote an output table"
    );
}

#[test]
fn one_table_with_records_is_enough_to_carry_a_run_whose_other_tables_are_empty() {
    let scratch = Scratch::new("one-table-with-records");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let empty = scratch.write("nothing_was_found.tsv", "");

    // Searching a second database and finding no hit at all in it is an ordinary outcome, not a
    // misread command line. The run has records, so it is a run.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-s"),
        empty.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "one empty table among two made the whole run fail:\n{}",
        stderr(&result)
    );
    assert!(read(&out).lines().count() > 1, "nothing was annotated");
}

#[test]
fn a_proteome_that_parses_but_cannot_be_annotated_succeeds_with_a_warning() {
    let scratch = Scratch::new("parsed-but-unannotatable");
    let out = scratch.path("hrds.txt");

    // The distinction exit 4 has to make. These two lines are read exactly as the command line
    // describes them: three columns, TAB separated, in the default order. What is missing is not
    // the reading but the content -- every description matches the default blacklist, so no word
    // survives to be scored. A proteome whose hits say nothing is a legitimate result, and a
    // legitimate result exits 0, however empty it is. It does earn a warning, because from the
    // outside an empty table looks the same whether it is the truth or a mistake.
    let table = scratch.write(
        "only_blacklisted_hits.tsv",
        "Query-1\tHit-1\thypothetical protein\n\
         Query-1\tHit-2\tuncharacterized protein\n",
    );

    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "a parsed but un-annotatable proteome is a result, not a failure:\n{}",
        stderr(&result)
    );
    assert_eq!(
        read(&out),
        "Annotee-Identifier\tHuman-Readable-Description\n"
    );
    assert!(
        stderr(&result).contains("no annotation could be generated"),
        "an empty result was reported without a word of warning:\n{}",
        stderr(&result)
    );
}
