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
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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

/// Runs the binary as `prot_scriber` does, but gives up after `limit` and returns `None` if it had
/// to. A test that simply called `prot_scriber` for input the binary cannot finish reading would
/// hang `cargo test` itself rather than failing it.
///
/// # Arguments
///
/// * `limit` - How long the run is allowed to take.
/// * `args` - The command line arguments.
fn prot_scriber_within(limit: Duration, args: &[&OsStr]) -> Option<Output> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_prot-scriber"))
        .args(args)
        .current_dir(crate_root())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to execute the prot-scriber binary");

    let started = Instant::now();
    loop {
        match child.try_wait().expect("could not poll the prot-scriber binary") {
            Some(_) => {
                return Some(
                    child
                        .wait_with_output()
                        .expect("could not collect the output of the prot-scriber binary"),
                )
            }
            None => {
                if started.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
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
        self.write_bytes(file_name, content.as_bytes())
    }

    /// Creates a file inside the scratch directory from raw bytes, which is how a test writes
    /// input that is deliberately not valid UTF-8.
    ///
    /// # Arguments
    ///
    /// * `file_name` - The name of the file within the scratch directory.
    /// * `content` - The bytes to write into it.
    fn write_bytes(&self, file_name: &str, content: &[u8]) -> PathBuf {
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

    // No arguments whatsoever. The option is named --db now, with --seq-sim-table kept as a
    // visible alias, so what a user reads in an error is the name the help gives:
    let nothing = prot_scriber(&[]);
    assert_eq!(nothing.status.code(), Some(2));
    assert!(
        stderr(&nothing).contains("--db"),
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

// ---------------------------------------------------------------------------------------------
// `--output -`: the table on standard output, so that prot-scriber can stand in a pipeline.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_single_dash_output_writes_the_table_to_standard_output() {
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-s"),
        trembl.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);

    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    // The same bytes the file would have held, down to the trailing newline:
    assert_eq!(stdout(&result), read(&fixture("Twelve_Proteins_HRDs.txt")));
    assert!(
        !crate_root().join("-").exists(),
        "a file literally named '-' was created in the working directory"
    );
}

#[test]
fn writing_to_standard_output_keeps_the_diagnostics_on_standard_error() {
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // The point of `-o -` is that the table can be piped, and a table with progress reports mixed
    // into it cannot. Verbose is the hardest case, because it has the most to say.
    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-v"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);

    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));

    let table = stdout(&result);
    assert!(
        table.starts_with("Annotee-Identifier\tHuman-Readable-Description\n"),
        "standard output did not begin with the table header:\n{}",
        table
    );
    assert!(
        !table.contains("Finished parsing"),
        "a progress message was written into the table:\n{}",
        table
    );

    let diagnostics = stderr(&result);
    assert!(
        diagnostics.contains("Finished parsing"),
        "the progress messages went somewhere other than standard error:\n{}",
        diagnostics
    );
    // "output written to file '-'" would name a file that was never written.
    assert!(
        diagnostics.contains("output written to standard output"),
        "the verbose run described where its output went as if it were a file:\n{}",
        diagnostics
    );
}

// Only `piping_the_table_into_head_ends_quietly` uses this, and that test is unix only; without
// the same gate this is dead code on other targets, which `-D warnings` turns into a failure.
#[cfg(unix)]
/// A sequence similarity search result table with `queries` distinct queries, each with one hit,
/// written into the given scratch directory. Its annotation is far larger than a pipe buffer,
/// which is what `prot-scriber ... -o - | head` needs in order to reach a broken pipe at all.
///
/// # Arguments
///
/// * `scratch` - Where to write the table.
/// * `queries` - How many queries to give it.
fn write_large_table(scratch: &Scratch, queries: usize) -> PathBuf {
    let mut table = String::new();
    for i in 0..queries {
        table.push_str(&format!(
            "Query-{:06}\tHit-{:06}\tcytochrome p450 monooxygenase family protein\n",
            i, i
        ));
    }
    scratch.write("many_queries.tsv", &table)
}

#[test]
#[cfg(unix)]
fn piping_the_table_into_head_ends_quietly() {
    let scratch = Scratch::new("piped-into-head");
    let table = write_large_table(&scratch, 20_000);

    // The Rust runtime ignores SIGPIPE before `main`, so without restoring it a write to a pipe
    // whose reader has gone comes back as EPIPE: prot-scriber would report that it could not
    // write its output and exit 74, for the ordinary act of looking at the first three rows.
    let pipeline = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{:?} -s {:?} -o - | head -3",
            env!("CARGO_BIN_EXE_prot-scriber"),
            table
        ))
        .current_dir(crate_root())
        .output()
        .expect("failed to run the pipeline");

    assert_eq!(
        stdout(&pipeline).lines().count(),
        3,
        "the pipeline did not deliver three lines:\n{}",
        stdout(&pipeline)
    );
    assert_eq!(
        stderr(&pipeline),
        "",
        "closing the pipe early was reported to the user as a failure"
    );
    assert_eq!(
        pipeline.status.code(),
        Some(0),
        "the pipeline did not succeed"
    );
}

#[test]
fn an_input_table_that_is_a_directory_ends_instead_of_looping() {
    let scratch = Scratch::new("input-table-is-a-directory");
    let table = scratch.path("a_directory");
    fs::create_dir(&table).expect("could not create the directory to be passed as an input table");
    let out = scratch.path("hrds.tsv");

    // Opening a directory succeeds on Linux; only the first read fails, with EISDIR. `io::Lines`
    // hands that error back without advancing, so every subsequent read fails identically -- and
    // `parse_table` prints "Continuing anyway!" and asks for the next line again. The run never
    // ends and never reports anything, while stderr grows without bound: measured at roughly 17 MB
    // per second, which on a cluster fills the job log until the walltime or the quota stops it.
    let result = prot_scriber_within(
        Duration::from_secs(20),
        &[
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
        ],
    )
    .expect(
        "prot-scriber never finished reading a directory given as an input table; it loops on the \
         read error instead of reporting it",
    );

    assert_eq!(
        result.status.code(),
        Some(74),
        "a table that cannot be read is an I/O error:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("a_directory"),
        "the error does not name the table that could not be read:\n{}",
        stderr(&result)
    );
}

#[test]
fn a_description_that_is_not_utf_8_keeps_its_hit_and_is_reported() {
    let scratch = Scratch::new("latin-1-description");
    let out = scratch.path("hrds.tsv");

    // 0xE9 is `e` acute in latin-1, and BLAST and DIAMOND titles do carry such bytes -- which is
    // why every reader of prot-scriber's output in the benchmark opens latin-1 rather than UTF-8.
    // A hit is data the user paid for a sequence similarity search to obtain; a byte prot-scriber
    // cannot decode is not a reason to throw the row away, nor to abandon the run.
    let mut table: Vec<u8> = Vec::new();
    table.extend_from_slice(b"Query-1\tHit-1\tprot");
    table.push(0xE9);
    table.extend_from_slice(b"in kinase superfamily protein\n");
    table.extend_from_slice(b"Query-1\tHit-2\tprot");
    table.push(0xE9);
    table.extend_from_slice(b"in kinase family protein\n");
    let table = scratch.write_bytes("latin1_hits.tsv", &table);

    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    assert_eq!(
        result.status.code(),
        Some(0),
        "a description prot-scriber cannot decode is not a failed run:\n{}",
        stderr(&result)
    );

    let written = read(&out);
    assert!(
        written.contains("Query-1"),
        "the query lost its only hits to a byte that could not be decoded, and vanished from the \
         output entirely:\n{}",
        written
    );
    assert!(
        written.contains("kinase"),
        "the surviving description carries none of the words the hit actually held:\n{}",
        written
    );

    // Silently is the one thing it must not be. The count is what tells a user reading a job log
    // whether one byte was mangled or a million were.
    let err = stderr(&result);
    assert!(
        err.contains('2') && err.to_lowercase().contains("utf-8"),
        "nothing told the user that 2 lines could not be decoded:\n{}",
        err
    );
    assert!(
        err.contains("latin1_hits.tsv"),
        "the report does not name the table the undecodable lines were in:\n{}",
        err
    );
}

// The failure needs a process limit lower than the number of threads asked for, which is a unix
// concept, so this test is unix only.
#[test]
#[cfg(unix)]
fn a_thread_count_the_system_refuses_is_not_reported_as_a_bug() {
    use std::os::unix::process::CommandExt;

    let scratch = Scratch::new("n-threads-refused");
    let out = scratch.path("hrds.tsv");
    let table = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    let mut command = Command::new(env!("CARGO_BIN_EXE_prot-scriber"));
    command
        .args([
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-n"),
            OsStr::new("500"),
            OsStr::new("-o"),
            out.as_os_str(),
        ])
        .current_dir(crate_root());

    // A process limit of 200 against 500 threads asked for. RLIMIT_NPROC of a few hundred is
    // ordinary in containers and on shared login nodes, and prot-scriber's default is the core
    // count, so a large machine reaches this with no argument at all. Setting it in the child
    // rather than through a shell keeps the test from depending on `ulimit -u`, which dash --
    // /bin/sh on Debian and Ubuntu -- does not have.
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit {
                rlim_cur: 200,
                rlim_max: 200,
            };
            if libc::setrlimit(libc::RLIMIT_NPROC, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let result = command
        .output()
        .expect("failed to execute the prot-scriber binary");

    // Asking for more threads than the system will give is the user's number meeting the user's
    // environment. Reporting it as an internal error tells them to open an issue about their own
    // ulimit, and buries the one thing they can actually act on.
    assert_no_panic_reached_the_user(&result);
    assert_eq!(
        result.status.code(),
        Some(2),
        "a thread count the system refuses is a usage error, not a crash:\n{}",
        stderr(&result)
    );
    let err = stderr(&result);
    assert!(
        err.contains("500") && err.contains("--n-threads"),
        "the error names neither the thread count asked for nor the argument that asked for it:\n{}",
        err
    );
}

/// `defaults` prints the built-in lists, which is what makes prot-scriber self-sufficient: the
/// help text used to send the reader to raw.githubusercontent.com seven times, for files the
/// binary already contained -- and, until `add6d40`, contained in a different version.
#[test]
fn defaults_prints_a_built_in_list_on_standard_output() {
    let output = prot_scriber(&[OsStr::new("defaults"), OsStr::new("filter-regexs")]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        fs::read_to_string(crate_root().join("assets/filter_stitle_regexs.txt")).unwrap(),
        "what `defaults` prints must be the file that is compiled in, byte for byte, so that \
         piping it through `diff -` answers whether a list on disk has fallen behind"
    );
    // Nothing but the list: the output is meant to be redirected into a file and given back.
    assert_eq!(stderr(&output), "");
}

/// Every name `defaults` accepts prints a list, and the two database-specific ones are among them
/// -- they had no name at all before, existing only as files in the repository.
#[test]
fn every_named_list_can_be_printed() {
    for name in [
        "blacklist-regexs",
        "filter-regexs",
        "filter-regexs-ncbi-nr",
        "filter-regexs-uniref",
        "capture-replace-pairs",
        "non-informative-words-regexs",
        "polish-capture-replace-pairs",
    ] {
        let output = prot_scriber(&[OsStr::new("defaults"), OsStr::new(name)]);
        assert!(output.status.success(), "{}: {}", name, stderr(&output));
        assert!(
            stdout(&output).ends_with('\n'),
            "{} is not newline terminated, so appending to it would join two expressions",
            name
        );
    }
}

/// Given no name, `defaults` says what there is rather than failing.
#[test]
fn defaults_without_a_name_lists_what_there_is() {
    let output = prot_scriber(&[OsStr::new("defaults")]);
    assert!(output.status.success(), "{}", stderr(&output));
    let listing = stdout(&output);
    for name in ["filter-regexs-ncbi-nr", "polish-capture-replace-pairs"] {
        assert!(listing.contains(name), "{:?} is not listed:\n{}", name, listing);
    }
}

/// A misspelled name is the user's mistake, not a crash and not an empty list.
#[test]
fn a_misspelled_list_name_is_a_usage_error() {
    let output = prot_scriber(&[OsStr::new("defaults"), OsStr::new("filter-regex")]);
    assert_eq!(output.status.code(), Some(2));
    assert_no_panic_reached_the_user(&output);
    let message = stderr(&output);
    assert!(
        message.contains("filter-regexs"),
        "the near miss is not offered:\n{}",
        message
    );
}

/// The verb must not have cost the flat command line anything: it is what every published methods
/// section and every pipeline uses, and `-o` and `-s` are still required of it.
#[test]
fn a_command_line_without_a_verb_is_still_an_annotation_run() {
    let output = prot_scriber(&[OsStr::new("--seq-sim-table"), OsStr::new("whatever.txt")]);
    assert_eq!(output.status.code(), Some(2));
    let message = stderr(&output);
    assert!(
        message.contains("--output"),
        "the missing argument is not named:\n{}",
        message
    );
}

/// The peak resident memory of one `prot-scriber` run, in KiB.
///
/// `VmHWM` in `/proc/<pid>/status` is a high-water mark, so it only ever rises; polling it while
/// the run lasts and keeping the largest reading gives the peak, without the run having to
/// cooperate. `getrusage(RUSAGE_CHILDREN)` would be simpler but reports the maximum over *every*
/// child this test binary has ever reaped, and `cargo test` runs these tests in one process.
///
/// # Arguments
///
/// * `table` - The input table to annotate.
/// * `out` - Where the run should write its output.
#[cfg(target_os = "linux")]
fn peak_resident_kib(table: &Path, out: &Path) -> u64 {
    let mut child = Command::new(env!("CARGO_BIN_EXE_prot-scriber"))
        .args([
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
        ])
        .current_dir(crate_root())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to execute the prot-scriber binary");

    let status_path = format!("/proc/{}/status", child.id());
    let mut peak = 0;
    while child
        .try_wait()
        .expect("could not poll the prot-scriber binary")
        .is_none()
    {
        if let Ok(status) = fs::read_to_string(&status_path) {
            for line in status.lines() {
                if let Some(value) = line.strip_prefix("VmHWM:") {
                    if let Some(kib) = value
                        .split_whitespace()
                        .next()
                        .and_then(|k| k.parse::<u64>().ok())
                    {
                        peak = peak.max(kib);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        child.wait().expect("could not reap the run").success(),
        "the run whose memory was being measured did not succeed"
    );
    peak
}

/// What a query costs in memory must stay small, because it is the one term that is paid for every
/// query in the input and never given back: a query's *hits* are dropped as soon as it is
/// annotated, but its human readable description is held until the table is written.
///
/// This is the property that makes prot-scriber usable on a whole proteome, and it is easy to lose
/// by accident -- returning a trace per query instead of a description, say, which is what the
/// planned `--explain` work does. Losing it would not fail any other test: the run would still be
/// correct, just no longer possible on a real input.
///
/// Measured when this was written: about 225 bytes per query, the same in a debug and a release
/// build, against a baseline of some 13 MiB. The limit below leaves a factor of four, so ordinary
/// variation between allocators and machines cannot trip it but a change of kind will.
#[test]
#[cfg(target_os = "linux")]
fn a_query_costs_a_bounded_amount_of_memory() {
    const FEWER: usize = 4_000;
    const MORE: usize = 16_000;
    const LIMIT_BYTES_PER_QUERY: u64 = 1_024;

    let small = Scratch::new("memory-per-query-fewer");
    let large = Scratch::new("memory-per-query-more");
    let fewer_peak = peak_resident_kib(
        &write_large_table(&small, FEWER),
        &small.path("annotations.tsv"),
    );
    let more_peak = peak_resident_kib(
        &write_large_table(&large, MORE),
        &large.path("annotations.tsv"),
    );

    let bytes_per_query =
        (more_peak.saturating_sub(fewer_peak) * 1024) / (MORE - FEWER) as u64;
    assert!(
        bytes_per_query < LIMIT_BYTES_PER_QUERY,
        "annotating {} queries instead of {} cost {} KiB instead of {} KiB, i.e. {} bytes per \
         query against a limit of {}. Something is now kept for every query in the input rather \
         than for the query being annotated.",
        MORE,
        FEWER,
        more_peak,
        fewer_peak,
        bytes_per_query,
        LIMIT_BYTES_PER_QUERY
    );
}

/// `-i` and `-g` configure how the `--seq-families` (`-f`) file is read, so giving one without
/// `-f` cannot mean anything. `-a`, which is in exactly the same position, has said so since
/// `1c9272a`; these two accepted the argument and ignored it, which reads as "understood" and is
/// the harder mistake to notice -- the run succeeds and simply does not do what was asked.
#[test]
fn a_gene_family_option_without_the_gene_family_file_is_a_usage_error() {
    let scratch = Scratch::new("family-option-without-file");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    for option in ["--seq-family-id-genes-separator", "--seq-family-gene-ids-separator"] {
        let output = prot_scriber(&[
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-o"),
            OsStr::new("-"),
            OsStr::new(option),
            OsStr::new(":"),
        ]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{} was accepted without --seq-families:\n{}",
            option,
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("seq-families"),
            "{} did not say what it needs:\n{}",
            option,
            stderr(&output)
        );
    }
}

/// The separator between a family's name and its gene list is taken as given. It used to be
/// `.trim()`ed, which silently emptied exactly the separators most worth spelling out -- a literal
/// TAB, which is the default and what MANUAL.txt section 2.1 exists to teach, or a space.
#[test]
fn a_whitespace_separator_in_the_families_file_survives() {
    let scratch = Scratch::new("whitespace-family-separator");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\nq2\ts2\ta kinase protein\n");
    let families = scratch.write("families.txt", "fam1 q1,q2\n");
    let output = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("--seq-family-id-genes-separator"),
        OsStr::new(" "),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(
        output.status.success(),
        "a single space as separator was not usable:\n{}",
        stderr(&output)
    );
    assert!(
        stdout(&output).contains("fam1\t"),
        "the family was not annotated; the separator did not survive:\n{}{}",
        stdout(&output),
        stderr(&output)
    );
}

/// `--field-separator` (`-p`) takes one character. It used to take the *first* character of
/// whatever it was given and drop the rest without a word, so `-p '@@'` silently became `-p '@'`
/// and a table that really is separated by something else was read as one long field.
#[test]
fn a_field_separator_of_more_than_one_character_is_a_usage_error() {
    let scratch = Scratch::new("multi-character-separator");
    let table = scratch.write("hits.tsv", "q1@s1@a kinase protein\n");
    let output = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-p"),
        OsStr::new("@@"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "'@@' was accepted as a field separator:\n{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_no_panic_reached_the_user(&output);
}

/// A TAB cannot be typed into most shells without a fight, and `-p '\t'` is what everyone reaches
/// for. It used to mean the backslash character, so the table was never split at all.
#[test]
fn a_field_separator_can_be_written_as_an_escape() {
    let scratch = Scratch::new("escaped-separator");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    for spelling in ["\\t", "tab"] {
        let output = prot_scriber(&[
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-p"),
            OsStr::new(spelling),
            OsStr::new("-o"),
            OsStr::new("-"),
        ]);
        assert!(
            output.status.success(),
            "-p {:?} did not name the TAB character:\n{}",
            spelling,
            stderr(&output)
        );
        assert!(
            stdout(&output).contains("q1\ta kinase protein"),
            "-p {:?} did not split the table:\n{}{}",
            spelling,
            stdout(&output),
            stderr(&output)
        );
    }
}

/// A query that belongs to no family is annotated only when `--annotate-non-family-queries` (`-a`)
/// asks for it. Whether it *was* depended on something it has nothing to do with: the annotation
/// mode is re-derived from the family map, and that map is drained as families are annotated, so
/// once the last one is gone a still-completing query is treated as a plain sequence.
///
/// The two runs below differ only in an unrelated second family that never completes and so keeps
/// the map non-empty. Neither is given `-a`; both must leave the lonely query out.
#[test]
fn a_query_in_no_family_is_annotated_only_when_asked() {
    let scratch = Scratch::new("lonely-query-mode");
    let table = scratch.write(
        "hits.tsv",
        "q1\ts1\ta kinase protein\nq2\ts2\ta kinase protein\nq3\ts3\ta lonely hydrolase\n",
    );
    let all_complete = scratch.write("families.txt", "fam1\tq1,q2\n");
    let one_incomplete = scratch.write("families_plus.txt", "fam1\tq1,q2\nfam2\tq9\n");

    for families in [&all_complete, &one_incomplete] {
        let output = prot_scriber(&[
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-f"),
            families.as_os_str(),
            OsStr::new("-o"),
            OsStr::new("-"),
        ]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(
            stdout(&output).contains("fam1\t"),
            "the family was not annotated with {:?}:\n{}",
            families,
            stdout(&output)
        );
        assert!(
            !stdout(&output).contains("q3\t"),
            "a query in no family was annotated without -a, with {:?}:\n{}",
            families,
            stdout(&output)
        );
    }

    // And it is annotated when asked for, either way round.
    let output = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-f"),
        all_complete.as_os_str(),
        OsStr::new("-a"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("q3\t"),
        "-a did not annotate the query outside every family:\n{}",
        stdout(&output)
    );
}

/// The point of naming tables: a per-table option means the table it names, whatever order the
/// arguments are written in. The two runs below give the same arguments in different orders and
/// must produce the same table, and the third gives the two filter lists the other way round and
/// must produce a different one -- otherwise the first two agreeing would prove nothing.
#[test]
fn a_named_per_table_option_does_not_depend_on_argument_order() {
    let sprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");
    let uniref = crate_root().join("assets/filter_stitle_regexs_UniRef.txt");
    let ncbi = crate_root().join("assets/filter_stitle_regexs_NCBI_NR.txt");

    let run = |args: &[&OsStr]| -> String {
        let output = prot_scriber(args);
        assert!(output.status.success(), "{}", stderr(&output));
        stdout(&output)
    };

    let declare_sprot = format!("sprot={}", sprot.display());
    let declare_trembl = format!("trembl={}", trembl.display());
    let sprot_uniref = format!("sprot={}", uniref.display());
    let trembl_ncbi = format!("trembl={}", ncbi.display());
    let sprot_ncbi = format!("sprot={}", ncbi.display());
    let trembl_uniref = format!("trembl={}", uniref.display());

    let one_order = run(&[
        OsStr::new("--db"),
        OsStr::new(&declare_sprot),
        OsStr::new("--db-filter"),
        OsStr::new(&sprot_uniref),
        OsStr::new("--db"),
        OsStr::new(&declare_trembl),
        OsStr::new("--db-filter"),
        OsStr::new(&trembl_ncbi),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    // Every argument moved, and both filter lists now stand before the tables they belong to:
    let other_order = run(&[
        OsStr::new("--db-filter"),
        OsStr::new(&trembl_ncbi),
        OsStr::new("--db-filter"),
        OsStr::new(&sprot_uniref),
        OsStr::new("-o"),
        OsStr::new("-"),
        OsStr::new("--db"),
        OsStr::new(&declare_trembl),
        OsStr::new("--db"),
        OsStr::new(&declare_sprot),
    ]);
    assert_eq!(
        one_order, other_order,
        "the order the arguments were written in changed the annotation"
    );

    let swapped = run(&[
        OsStr::new("--db"),
        OsStr::new(&declare_sprot),
        OsStr::new("--db-filter"),
        OsStr::new(&sprot_ncbi),
        OsStr::new("--db"),
        OsStr::new(&declare_trembl),
        OsStr::new("--db-filter"),
        OsStr::new(&trembl_uniref),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_ne!(
        one_order, swapped,
        "giving each table the other's filter list changed nothing, so this test cannot tell \
         whether the lists reach the tables they name at all"
    );
}

/// A per-table option naming a table that was never declared is the user's mistake, and the one
/// the whole redesign exists to make visible. It used to be unrepresentable *as a mistake*: the
/// option simply belonged to whichever table stood in the same position.
#[test]
fn a_per_table_option_naming_an_undeclared_table_is_a_usage_error() {
    let scratch = Scratch::new("undeclared-table");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let declaration = format!("sprot={}", table.display());
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("nr=assets/filter_stitle_regexs.txt"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("\"nr\""), "{}", message);
    assert!(
        message.contains("\"sprot\""),
        "the message did not say what tables there are:\n{}",
        message
    );
}

/// Two tables under one name would make that name useless for saying which is meant.
#[test]
fn two_tables_of_the_same_name_are_a_usage_error() {
    let scratch = Scratch::new("repeated-table-name");
    let one = scratch.write("one.tsv", "q1\ts1\ta kinase protein\n");
    let two = scratch.write("two.tsv", "q2\ts2\ta kinase protein\n");
    let first = format!("db={}", one.display());
    let second = format!("db={}", two.display());
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&first),
        OsStr::new("--db"),
        OsStr::new(&second),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stderr(&output).contains("\"db\""), "{}", stderr(&output));
}

/// The named and the positional forms of the same setting cannot both be given: they would be two
/// answers to one question, and picking either would be a guess.
#[test]
fn the_named_and_positional_forms_cannot_be_mixed() {
    let scratch = Scratch::new("mixed-forms");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let declaration = format!("hits={}", table.display());
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("-l"),
        OsStr::new("assets/filter_stitle_regexs.txt"),
        OsStr::new("--db-filter"),
        OsStr::new("hits=assets/filter_stitle_regexs.txt"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("cannot be used with"),
        "{}",
        stderr(&output)
    );
}

/// A table declared as a bare path is named after its file, so the trivial case pays no naming
/// tax and the named options still work.
#[test]
fn a_table_declared_as_a_bare_path_is_named_after_its_file() {
    let scratch = Scratch::new("bare-path-name");
    let table = scratch.write("my_hits.tsv", "q1\ts1\ta kinase protein\n");
    let output = prot_scriber(&[
        OsStr::new("--db"),
        table.as_os_str(),
        OsStr::new("--db-sep"),
        OsStr::new("my_hits=\\t"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("q1\ta kinase protein"), "{}", stdout(&output));
}

/// A rule list can be named rather than found. `@filter-regexs-ncbi-nr` is the same list
/// `prot-scriber defaults filter-regexs-ncbi-nr` prints, by construction, so using the NCBI-NR
/// filters no longer means keeping a downloaded copy that can go stale.
#[test]
fn a_rule_list_can_be_a_built_in_name() {
    let scratch = Scratch::new("builtin-source");
    let table = scratch.write("hits.tsv", "q1\ts1\tsp|Q9XYZ1|AAKG3 a kinase protein OS=Zea mays\n");
    let declaration = format!("db={}", table.display());

    let named = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("db=@filter-regexs"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(named.status.success(), "{}", stderr(&named));

    // The same list, given as the file it also is:
    let from_file = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("db=assets/filter_stitle_regexs.txt"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(
        stdout(&named),
        stdout(&from_file),
        "'@filter-regexs' and the file it is compiled from gave different annotations"
    );
}

/// `none` is how a list is switched off, which no per-table option could say before: the only
/// way to filter nothing was to pass an empty file.
#[test]
fn a_rule_list_can_be_switched_off() {
    let scratch = Scratch::new("none-source");
    let table = scratch.write("hits.tsv", "q1\ts1\tsp|Q9XYZ1|AAKG3 a kinase protein OS=Zea mays\n");
    let declaration = format!("db={}", table.display());
    let empty = scratch.write("empty.txt", "");

    let switched_off = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("db=none"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(switched_off.status.success(), "{}", stderr(&switched_off));
    assert!(
        stdout(&switched_off).contains("sp"),
        "nothing was left unfiltered, so the list was not switched off:\n{}",
        stdout(&switched_off)
    );

    let with_empty_file = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new(&format!("db={}", empty.display())),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(stdout(&switched_off), stdout(&with_empty_file));
}

/// A misspelled built-in name says what the names are, rather than looking for a file called
/// `@something` and reporting that it is missing.
#[test]
fn a_misspelled_built_in_name_is_a_usage_error() {
    let scratch = Scratch::new("misspelled-builtin");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let declaration = format!("db={}", table.display());
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("db=@ncbi"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("filter-regexs-ncbi-nr"),
        "the message did not say what the names are:\n{}",
        stderr(&output)
    );
}

/// A command line in the positional form prints the named one that says the same thing, and that
/// named one, pasted into a shell and run, must produce exactly the same table. The translation is
/// run here the way a user would run it -- through `sh` -- so its quoting is tested too, which is
/// the part a test that split the string on spaces would quietly skip.
#[test]
#[cfg(unix)]
fn the_printed_translation_reproduces_the_command_it_translates() {
    let sprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");

    let positional = prot_scriber(&[
        OsStr::new("-s"),
        sprot.as_os_str(),
        OsStr::new("-s"),
        trembl.as_os_str(),
        OsStr::new("-l"),
        OsStr::new("assets/filter_stitle_regexs.txt"),
        OsStr::new("-l"),
        OsStr::new("assets/filter_stitle_regexs_UniRef.txt"),
        // A value with a space in it, so the quoting has something to do:
        OsStr::new("-e"),
        OsStr::new("qacc sacc stitle"),
        OsStr::new("-e"),
        OsStr::new("qacc sacc stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(positional.status.success(), "{}", stderr(&positional));

    let note = stderr(&positional);
    let translation = note
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("prot-scriber annotate "))
        .unwrap_or_else(|| panic!("no translation was printed:\n{}", note));

    // What the user would paste, with the name of the binary this test built:
    let pasted = translation.replacen(
        "prot-scriber",
        &format!("{:?}", env!("CARGO_BIN_EXE_prot-scriber")),
        1,
    );
    let rerun = Command::new("sh")
        .arg("-c")
        .arg(&pasted)
        .current_dir(crate_root())
        .output()
        .expect("could not run the translated command line");
    assert!(
        rerun.status.success(),
        "the translation did not run:\n{}\n{}",
        pasted,
        stderr(&rerun)
    );
    assert_eq!(
        stdout(&positional),
        stdout(&rerun),
        "the translation produced a different table than the command it translates:\n{}",
        pasted
    );
    assert!(
        !stderr(&rerun).contains("Note: this command line"),
        "the translation itself still uses the positional form:\n{}",
        stderr(&rerun)
    );
}

/// A command line that uses no positional per-table option says nothing, because there is nothing
/// to translate.
#[test]
fn a_command_line_with_nothing_to_translate_says_nothing() {
    let scratch = Scratch::new("nothing-to-translate");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let declaration = format!("db={}", table.display());
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("db=@filter-regexs"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        !stderr(&output).contains("Note:"),
        "a named command line was told to name its tables:\n{}",
        stderr(&output)
    );
}

/// Diamond's own column names work in `--header`. A Diamond user's `-f 6 qseqid sseqid stitle` is
/// the obvious thing to paste, and it used to be refused -- the help apologised for it rather than
/// the code accepting it.
#[test]
fn a_header_may_use_diamond_column_names() {
    let sprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let blast = prot_scriber(&[
        OsStr::new("-s"),
        sprot.as_os_str(),
        OsStr::new("-e"),
        OsStr::new("qacc sacc stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    let diamond = prot_scriber(&[
        OsStr::new("-s"),
        sprot.as_os_str(),
        OsStr::new("-e"),
        OsStr::new("qseqid sseqid stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(diamond.status.success(), "{}", stderr(&diamond));
    assert_eq!(stdout(&blast), stdout(&diamond));

    // And a header genuinely missing a column still says so, in both dialects:
    let incomplete = prot_scriber(&[
        OsStr::new("-s"),
        sprot.as_os_str(),
        OsStr::new("-e"),
        OsStr::new("qseqid stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(incomplete.status.code(), Some(2), "{}", stderr(&incomplete));
    assert!(
        stderr(&incomplete).contains("sseqid"),
        "the message did not offer the Diamond spelling:\n{}",
        stderr(&incomplete)
    );
}
