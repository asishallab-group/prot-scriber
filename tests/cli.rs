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

/// Runs the binary with extra environment variables set, for the few things whose behaviour is
/// decided by the environment rather than by an argument -- `NO_COLOR` and `CLICOLOR_FORCE`, which
/// is also the only way to see coloured output from a test, the harness never being a terminal.
fn prot_scriber_with_env(env: &[(&str, &str)], args: &[&OsStr]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_prot-scriber"));
    command.args(args).current_dir(crate_root());
    for (name, value) in env {
        command.env(name, value);
    }
    command.output().expect("failed to execute the prot-scriber binary")
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
    // that describes nothing. Every hit description of its single query matches the default
    // blacklist, so no description survives to be scored. A real run reaches this state whenever a
    // small query set finds only uninformative hits, or when every row was excluded by a filter
    // list.
    //
    // `-x` is what makes the result empty rather than merely uninformative: such a query IS
    // annotated, as an `unknown protein`, and `--exclude-not-annotated-queries` is the flag that
    // asks for those to be left out of the table. So this is the header-only case, and it is now
    // the only way to reach one.
    let table = scratch.write(
        "only_blacklisted_hits.tsv",
        "Query-1\tHit-1\thypothetical protein\n\
         Query-1\tHit-2\tuncharacterized protein\n",
    );

    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-x"),
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
fn a_header_missing_a_required_column_is_a_usage_error() {
    let scratch = Scratch::new("header-missing-column");
    let out = scratch.path("hrds.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // 'stitle' is missing, and it is the column the whole program exists to read.
    let result = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("sprot={}", swissprot.to_string_lossy())),
        OsStr::new("--db-header"),
        OsStr::new("sprot=qacc sacc evalue"),
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
        stderr(&result).contains("must stand together"),
        "stderr did not explain what an input table has to look like:\n{}",
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
    // --db-blacklist a per-table one, which are two different code paths into it.
    for (i, flag) in ["-w", "--db-blacklist"].iter().enumerate() {
        let out = scratch.path(&format!("hrds{}.txt", i));
        let value = if *flag == "-w" {
            missing.to_string_lossy().to_string()
        } else {
            format!("sprot={}", missing.to_string_lossy())
        };
        let result = prot_scriber(&[
            OsStr::new("--db"),
            OsStr::new(&format!("sprot={}", swissprot.to_string_lossy())),
            OsStr::new(flag),
            OsStr::new(&value),
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
        OsStr::new("--db"),
        OsStr::new(&format!("hits={}", table.to_string_lossy())),
        OsStr::new("--db-sep"),
        OsStr::new("hits=;"),
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
    //
    // With `-x`, because the warning belongs to the header-only table and that is what `-x`
    // produces here: without it the query is reported as an `unknown protein`, which says the
    // same thing on the row itself and leaves nothing ambiguous to warn about.
    let table = scratch.write(
        "only_blacklisted_hits.tsv",
        "Query-1\tHit-1\thypothetical protein\n\
         Query-1\tHit-2\tuncharacterized protein\n",
    );

    let result = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-x"),
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
    let output = prot_scriber(&[OsStr::new("defaults"), OsStr::new("filter-regexs-uniprot")]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        fs::read_to_string(crate_root().join("assets/filter_stitle_regexs_UniProt.txt")).unwrap(),
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
        "filter-regexs-uniprot",
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

/// Every option `defaults` names must be an option the binary accepts.
///
/// The bare listing exists to tell a reader which option each list is the default for -- it is the
/// most discoverable surface this feature has, and the first thing anyone adopting a new database
/// reads. Naming a flag that does not exist sends them to `error: unexpected argument` on their
/// first command, with nothing to say whether they mistyped it or we did.
///
/// The 1.0.0 command line replaced the flat per-run `-l`/`-b`/`-c` with the per-table
/// `--db-filter NAME=SOURCE` family; this listing was not moved with them.
#[test]
fn every_option_named_by_defaults_exists() {
    let help = stdout(&prot_scriber(&[OsStr::new("annotate"), OsStr::new("--help")]));
    // Both surfaces: the bare listing, and the `Possible values` block, which is the same text
    // again from the enum's own doc comments.
    for asked in [
        vec![OsStr::new("defaults")],
        vec![OsStr::new("defaults"), OsStr::new("--help")],
    ] {
        let listing = stdout(&prot_scriber(&asked));
        let mut named: Vec<&str> = listing
            .split_whitespace()
            .filter(|token| token.starts_with("--"))
            .collect();
        named.sort_unstable();
        named.dedup();
        assert!(!named.is_empty(), "names no option at all:\n{}", listing);
        for option in named {
            assert!(
                help.contains(option),
                "`{:?}` names {:?}, which `annotate --help` does not offer:\n{}",
                asked,
                option,
                listing
            );
        }
    }
}

/// The MANUAL and the README must not tell the reader to pass an option that was removed.
///
/// Kept as a check on the three literal spellings rather than on every `--token` in the file,
/// because both documents also quote Blast's and Diamond's command lines, whose flags are theirs.
#[test]
fn the_manual_names_no_option_the_binary_rejects() {
    for file in ["MANUAL.txt", "README.md"] {
        let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file))
            .unwrap_or_else(|e| panic!("cannot read {}: {}", file, e));
        for gone in ["--filter-regexs", "--blacklist-regexs", "--capture-replace-pairs"] {
            assert!(
                !text.contains(gone),
                "{} tells the reader to pass {:?}, which the binary rejects; \
                 the per-table form is `--db-filter <name>=@NAME`",
                file,
                gone
            );
        }
    }
}

/// A table with more columns than the header names is refused, not guessed at.
///
/// `diamond blastp -f 6 qseqid sseqid evalue stitle` is an ordinary invocation, and its table has
/// four columns. The default header names three -- `qacc sacc stitle` -- so `stitle` is read from
/// index 2, which is the e-value, and every description becomes `1e 50`. That ran to completion and
/// exited 0, and the only way to notice was to read the output and disbelieve it.
///
/// The header has to fit the table. A column count that disagrees with the header is the table not
/// being the table the arguments describe, which is the same fault the too-few-fields branch
/// already refuses -- it was simply unreachable whenever the extra columns came first.
#[test]
fn a_table_whose_columns_do_not_fit_the_header_is_refused() {
    let scratch = Scratch::new("header-fit");
    let table = scratch.write(
        "four_columns.tsv",
        "Q1\tS1\t1e-50\tXP_1.1 alcohol dehydrogenase [Arabidopsis]\n\
         Q1\tS2\t1e-40\tXP_2.1 alcohol dehydrogenase 1\n",
    );
    let output = prot_scriber(&[
        OsStr::new("-s"),
        OsStr::new(&format!("db={}", table.display())),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_ne!(
        output.status.code(),
        Some(0),
        "a four-column table went through a three-column header and said nothing:\n{}",
        stdout(&output)
    );
    assert_no_panic_reached_the_user(&output);
    let message = stderr(&output);
    for expected in ["4", "3", "--db-header"] {
        assert!(
            message.contains(expected),
            "the complaint does not mention {:?}:\n{}",
            expected,
            message
        );
    }
    assert!(
        !stdout(&output).contains("1e"),
        "an e-value was annotated as a description:\n{}",
        stdout(&output)
    );
}

/// ...and naming the columns is what makes the same table work.
#[test]
fn a_table_whose_columns_are_named_is_read() {
    let scratch = Scratch::new("header-fit-named");
    let table = scratch.write(
        "four_columns.tsv",
        "Q1\tS1\t1e-50\tXP_1.1 alcohol dehydrogenase [Arabidopsis]\n\
         Q1\tS2\t1e-40\tXP_2.1 alcohol dehydrogenase 1\n",
    );
    let output = prot_scriber(&[
        OsStr::new("-s"),
        OsStr::new(&format!("db={}", table.display())),
        OsStr::new("--db-header"),
        OsStr::new("db=qacc sacc evalue stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let written = stdout(&output);
    assert!(
        written.contains("alcohol dehydrogenase"),
        "the description was not read from the column it was named in:\n{}",
        written
    );
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
/// * `extra` - Any further arguments the run is to be given.
#[cfg(target_os = "linux")]
fn peak_resident_kib(table: &Path, out: &Path, extra: &[&OsStr]) -> u64 {
    let mut arguments: Vec<&OsStr> = vec![
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ];
    arguments.extend_from_slice(extra);
    let mut child = Command::new(env!("CARGO_BIN_EXE_prot-scriber"))
        .args(arguments)
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
        &[],
    );
    let more_peak = peak_resident_kib(
        &write_large_table(&large, MORE),
        &large.path("annotations.tsv"),
        &[],
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
            OsStr::new("--db"),
            OsStr::new(&format!("hits={}", table.to_string_lossy())),
            OsStr::new("--db-sep"),
            OsStr::new(&format!("hits={}", spelling)),
            OsStr::new("-o"),
            OsStr::new("-"),
        ]);
        assert!(
            output.status.success(),
            "--db-sep {:?} did not name the TAB character:\n{}",
            spelling,
            stderr(&output)
        );
        assert!(
            stdout(&output).contains("q1\ta kinase protein"),
            "--db-sep {:?} did not split the table:\n{}{}",
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
        OsStr::new("nr=assets/filter_stitle_regexs_UniProt.txt"),
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
        OsStr::new("db=@filter-regexs-uniprot"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(named.status.success(), "{}", stderr(&named));

    // The same list, given as the file it also is:
    let from_file = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("db=assets/filter_stitle_regexs_UniProt.txt"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(
        stdout(&named),
        stdout(&from_file),
        "'@filter-regexs-uniprot' and the file it is compiled from gave different annotations"
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

/// A command line in the positional form prints the arguments that replace its own, and following
/// that must produce exactly the same table. The replacement is run here the way a user would run
/// it -- through `sh` -- so its quoting is tested too, which is the part a test that split the
/// string on spaces would quietly skip.
#[cfg(unix)]
/// Diamond's own column names work in `--header`. A Diamond user's `-f 6 qseqid sseqid stitle` is
/// the obvious thing to paste, and it used to be refused -- the help apologised for it rather than
/// the code accepting it.
#[test]
fn a_header_may_use_diamond_column_names() {
    let sprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let named = format!("sprot={}", sprot.to_string_lossy());
    let blast = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&named),
        OsStr::new("--db-header"),
        OsStr::new("sprot=qacc sacc stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    let diamond = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&named),
        OsStr::new("--db-header"),
        OsStr::new("sprot=qseqid sseqid stitle"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(diamond.status.success(), "{}", stderr(&diamond));
    assert_eq!(stdout(&blast), stdout(&diamond));

    // And a header genuinely missing a column still says so, in both dialects:
    let incomplete = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&named),
        OsStr::new("--db-header"),
        OsStr::new("sprot=qseqid stitle"),
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

/// Naming the tables means a per-table option may be given for some tables and not others, which
/// the positional form could not express -- it demanded the option either not at all or exactly
/// once per table. That freedom has an edge: giving the same option *twice for the same table* is
/// two answers to one question, and applying both in order and keeping the last is the silent
/// wrong answer this whole interface exists to make impossible.
#[test]
fn a_per_table_option_given_twice_for_one_table_is_a_usage_error() {
    let scratch = Scratch::new("repeated-per-table-option");
    let table = scratch.write("hits.tsv", "q1\ts1\tsp|Q1|AAA a kinase protein OS=Zea mays\n");
    let declaration = format!("a={}", table.display());
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("a=none"),
        OsStr::new("--db-filter"),
        OsStr::new("a=@filter-regexs-uniprot"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(output.status.code(), Some(2), "{}{}", stdout(&output), stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("--db-filter"), "{}", message);
    assert!(message.contains("\"a\""), "{}", message);
    assert_no_panic_reached_the_user(&output);
}

/// The same option for *different* tables is the ordinary case and must stay allowed, so the check
/// above cannot simply count occurrences.
#[test]
fn a_per_table_option_may_name_each_table_once() {
    let scratch = Scratch::new("per-table-option-each-table");
    let one = scratch.write("one.tsv", "q1\ts1\ta kinase protein\n");
    let two = scratch.write("two.tsv", "q2\ts2\ta kinase protein\n");
    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("a={}", one.display())),
        OsStr::new("--db"),
        OsStr::new(&format!("b={}", two.display())),
        OsStr::new("--db-filter"),
        OsStr::new("a=none"),
        OsStr::new("--db-filter"),
        OsStr::new("b=@filter-regexs-uniprot"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
}


/// Following the note must give the same table as the command line it comments on -- including
/// when that command line carries options the note has nothing to say about. It used to print a
/// whole `prot-scriber annotate ...` line built only from the tables and the positional per-table
/// options, so a run with `--seq-families` or an already-named `--db-filter` was handed a command
/// that silently left them out. Pasting it annotated something else.
#[cfg(unix)]
/// `-a` must reach a query outside every family even when that query has no hit in one of the
/// input tables, which is entirely ordinary -- a protein need not be found in every database
/// searched. Such a query is never "complete", so the streaming path never considers it, and it is
/// left for the end of the run.
///
/// Until `a473824` this worked by accident: the annotation mode was re-derived from the family map,
/// the map emptied as families were annotated, and the leftover queries were then swept up as
/// though the run had been a plain sequence annotation all along. Fixing the mode removed the
/// accident and with it the only path that reached these queries.
#[test]
fn a_lonely_query_missing_from_one_table_is_still_annotated_with_a() {
    let scratch = Scratch::new("lonely-query-two-tables");
    let first = scratch.write(
        "A.tsv",
        "q1\ta1\ta kinase protein\nq2\ta2\ta kinase protein\nq3\ta3\ta lonely hydrolase\n",
    );
    let second = scratch.write("B.tsv", "q1\tb1\ta kinase protein\nq2\tb2\ta kinase protein\n");
    let families = scratch.write("families.txt", "fam1\tq1,q2\n");

    let asked = prot_scriber(&[
        OsStr::new("-s"),
        first.as_os_str(),
        OsStr::new("-s"),
        second.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("-a"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(asked.status.success(), "{}", stderr(&asked));
    assert!(
        stdout(&asked).contains("fam1\t"),
        "the family was not annotated:\n{}",
        stdout(&asked)
    );
    assert!(
        stdout(&asked).contains("q3\t"),
        "-a did not reach a query that has no hit in one of the two tables:\n{}",
        stdout(&asked)
    );

    // And without -a it is still left out, whatever the tables did:
    let not_asked = prot_scriber(&[
        OsStr::new("-s"),
        first.as_os_str(),
        OsStr::new("-s"),
        second.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(not_asked.status.success(), "{}", stderr(&not_asked));
    assert!(
        !stdout(&not_asked).contains("q3\t"),
        "a query in no family was annotated without -a:\n{}",
        stdout(&not_asked)
    );
}

/// `--unsorted-input` reads a table whose rows are not grouped by query, and must give exactly
/// what the same table gives once grouped -- tolerating the input is not enough, the scattered
/// rows of a query have to end up in the same query.
#[test]
fn unsorted_input_is_read_and_agrees_with_the_grouped_table() {
    let scratch = Scratch::new("unsorted-input");
    let scattered = scratch.write(
        "scattered.tsv",
        "q1\ts1\talpha kinase protein\n\
         q2\ts2\tbeta hydrolase enzyme\n\
         q1\ts3\talpha kinase domain\n\
         q2\ts4\tbeta hydrolase family\n",
    );
    let grouped = scratch.write(
        "grouped.tsv",
        "q1\ts1\talpha kinase protein\n\
         q1\ts3\talpha kinase domain\n\
         q2\ts2\tbeta hydrolase enzyme\n\
         q2\ts4\tbeta hydrolase family\n",
    );

    // Without the flag the scattered table is refused, and the message offers both ways out:
    let refused = prot_scriber(&[
        OsStr::new("-s"),
        scattered.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(refused.status.code(), Some(3), "{}", stderr(&refused));
    assert!(
        stderr(&refused).contains("--unsorted-input"),
        "the message did not offer the option that reads such a table:\n{}",
        stderr(&refused)
    );
    assert!(
        stderr(&refused).contains("sort -s"),
        "the message did not offer the stable sort that groups it:\n{}",
        stderr(&refused)
    );

    let buffered = prot_scriber(&[
        OsStr::new("-s"),
        scattered.as_os_str(),
        OsStr::new("--unsorted-input"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(buffered.status.success(), "{}", stderr(&buffered));

    let sorted = prot_scriber(&[
        OsStr::new("-s"),
        grouped.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(sorted.status.success(), "{}", stderr(&sorted));
    assert_eq!(
        stdout(&buffered),
        stdout(&sorted),
        "reading the scattered table gave something other than reading the grouped one"
    );
}

/// The same, for gene families: the queries of a family may be scattered too, and `-a` still has
/// to distinguish a query in no family from one in a family whose rows came late.
#[test]
fn unsorted_input_works_for_gene_families_too() {
    let scratch = Scratch::new("unsorted-input-families");
    let scattered = scratch.write(
        "scattered.tsv",
        "q1\ts1\talpha kinase protein\n\
         q3\ts3\tgamma lonely hydrolase\n\
         q2\ts2\talpha kinase domain\n\
         q1\ts4\talpha kinase family\n",
    );
    let families = scratch.write("families.txt", "fam1\tq1,q2\n");
    let output = prot_scriber(&[
        OsStr::new("-s"),
        scattered.as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("--unsorted-input"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("fam1\t"), "{}", stdout(&output));
    assert!(
        !stdout(&output).contains("q3\t"),
        "a query in no family was annotated without -a:\n{}",
        stdout(&output)
    );
}

/// A binary has to agree with itself about which version it is. `--version` was a hand-written
/// string in the `#[command]` attribute and the crate's own version was something else, so
/// prot-scriber reported 0.1.6 while the package it was built from -- the one bioconda reads, and
/// the one every other part of the build knows about -- said 0.1.5.
#[test]
fn the_reported_version_is_the_version_of_the_package() {
    let output = prot_scriber(&[OsStr::new("--version")]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains(env!("CARGO_PKG_VERSION")),
        "prot-scriber reports {:?} but was built from version {:?}",
        stdout(&output).trim(),
        env!("CARGO_PKG_VERSION")
    );
}

/// `--dry-run` resolves and checks the command line, says what would be done, and does none of it.
/// It is the step before a long run, so the things it can be wrong about are the things worth
/// knowing early: a name that pairs with no table, a file of regular expressions that is not
/// there, an input table that is not there.
#[test]
fn a_dry_run_reports_what_would_happen_and_writes_nothing() {
    let scratch = Scratch::new("dry-run");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let out = scratch.path("annotations.tsv");

    let output = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("sprot={}", table.display())),
        OsStr::new("--db-filter"),
        OsStr::new("sprot=@filter-regexs-ncbi-nr"),
        OsStr::new("--dry-run"),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        !out.exists(),
        "a dry run wrote its output file; nothing should have been written"
    );

    let report = stdout(&output);
    for expected in ["sprot", "hits.tsv", "annotate query sequences"] {
        assert!(report.contains(expected), "{:?} is missing from:\n{}", expected, report);
    }
    // The list that came from the command line is not called the default, and the ones that did
    // not come from it are:
    assert!(
        report
            .lines()
            .any(|line| line.trim_start().starts_with("blacklist ")
                && line.ends_with(", the default")),
        "an untouched list was not reported as the default:\n{}",
        report
    );
    assert!(
        report
            .lines()
            .any(|line| line.trim_start().starts_with("filter ") && !line.contains("the default")),
        "a list given on the command line was reported as the default:\n{}",
        report
    );
}

/// The point of resolving before running: a mistake costs a second instead of an hour. A dry run
/// has to fail on everything a real run would have failed on before reading any input.
#[test]
fn a_dry_run_fails_on_what_a_real_run_would_fail_on() {
    let scratch = Scratch::new("dry-run-failures");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let declaration = format!("sprot={}", table.display());

    // A per-table option naming a table that does not exist:
    let undeclared = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&declaration),
        OsStr::new("--db-filter"),
        OsStr::new("nr=@filter-regexs-uniprot"),
        OsStr::new("--dry-run"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(undeclared.status.code(), Some(2), "{}", stderr(&undeclared));

    // An input table that is not there. Only a dry run can catch this before a parsing thread
    // does, which is the whole reason it stats them:
    let missing = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new("nr=no/such/table.tsv"),
        OsStr::new("--dry-run"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_ne!(missing.status.code(), Some(0), "{}", stdout(&missing));
    assert!(
        stderr(&missing).contains("no/such/table.tsv"),
        "the missing table was not named:\n{}",
        stderr(&missing)
    );
    assert_no_panic_reached_the_user(&missing);
}

/// A run records itself, and the record replays to the same table. This is the reproducibility
/// the whole stage is for: a command line names files whose contents change and leaves out
/// everything defaulted, so it is not a record of anything.
#[test]
fn a_run_writes_a_plan_that_replays_to_the_same_table() {
    let scratch = Scratch::new("plan-round-trip");
    let sprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let first = scratch.path("first.tsv");
    let again = scratch.path("again.tsv");

    let original = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("sprot={}", sprot.display())),
        OsStr::new("--db-filter"),
        OsStr::new("sprot=@filter-regexs-ncbi-nr"),
        OsStr::new("-o"),
        first.as_os_str(),
    ]);
    assert!(original.status.success(), "{}", stderr(&original));

    let plan_path = scratch.path("first.tsv.plan.toml");
    assert!(plan_path.exists(), "no run plan was written beside the output");
    let plan = read(&plan_path);

    // The rule list is written out, not named. A plan that said "@filter-regexs-ncbi-nr" would
    // mean whatever a later prot-scriber decided that name meant.
    assert!(
        !plan.contains("@filter-regexs-ncbi-nr"),
        "the plan recorded the name of a list instead of the list:\n{}",
        plan
    );
    assert!(
        plan.contains("uniref") || plan.contains("\\x01"),
        "the plan does not hold the expressions the name stood for:\n{}",
        plan
    );

    // The digest is the hash of the file that was read, so the plan records the data too:
    let digest = plan
        .lines()
        .find_map(|line| line.strip_prefix("digest = "))
        .map(|value| value.trim_matches('"').to_string())
        .unwrap_or_else(|| panic!("the plan holds no digest:\n{}", plan));
    assert_eq!(
        digest,
        blake3::hash(&fs::read(&sprot).unwrap()).to_hex().to_string(),
        "the recorded digest is not the hash of the table that was read"
    );

    // And it replays. The output path is the one the plan records, so point it elsewhere first.
    let replayed_plan = scratch.write(
        "again.plan.toml",
        &plan.replace(
            &format!("output = {:?}", first.display().to_string()),
            &format!("output = {:?}", again.display().to_string()),
        ),
    );
    let replay = prot_scriber(&[OsStr::new("--plan"), replayed_plan.as_os_str()]);
    assert!(replay.status.success(), "{}", stderr(&replay));
    assert_eq!(
        read(&first),
        read(&again),
        "replaying the plan gave a different table"
    );
}

/// `--plan` cannot be combined with anything that configures a run: two answers to one question,
/// and a precedence rule is a thing you would have to know to read the command line.
#[test]
fn a_plan_cannot_be_combined_with_configuration() {
    let scratch = Scratch::new("plan-conflicts");
    let plan = scratch.write("empty.plan.toml", "");
    for option in [
        vec!["-l", "none"],
        vec!["-o", "somewhere.tsv"],
        vec!["--db", "a=hits.tsv"],
        vec!["-n", "4"],
        vec!["--unsorted-input"],
    ] {
        let mut args: Vec<&OsStr> = vec![OsStr::new("--plan"), plan.as_os_str()];
        args.extend(option.iter().map(OsStr::new));
        let output = prot_scriber(&args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{:?} was accepted alongside --plan:\n{}",
            option,
            stderr(&output)
        );
    }
}

/// Nothing is written where there is nothing to write it beside, and nothing at all if asked.
#[test]
fn a_plan_is_not_written_when_it_was_not_asked_for() {
    let scratch = Scratch::new("plan-not-written");
    let table = scratch.write("hits.tsv", "q1\ts1\ta kinase protein\n");
    let out = scratch.path("annotations.tsv");

    let suppressed = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("--plan-out"),
        OsStr::new("none"),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);
    assert!(suppressed.status.success(), "{}", stderr(&suppressed));
    assert!(
        !scratch.path("annotations.tsv.plan.toml").exists(),
        "a plan was written although --plan-out none was given"
    );

    // A table on standard output has no file name to hang a plan on:
    let to_stdout = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(to_stdout.status.success(), "{}", stderr(&to_stdout));
    assert!(
        !crate_root().join("-.plan.toml").exists(),
        "a plan was invented for a table written to standard output"
    );
}

/// `--var` fills placeholders in a plan's *paths*, so one plan serves a set of datasets annotated
/// the same way. It must leave the regular expressions alone: `${name}` is how fancy-regex names a
/// capture group, and the capture-replace pairs are written in exactly that syntax, so a
/// substitution over the whole file would rewrite the expressions the plan exists to record.
#[test]
fn a_plan_variable_fills_in_paths_and_leaves_expressions_alone() {
    let scratch = Scratch::new("plan-variables");
    let sprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let direct = scratch.path("direct.tsv");

    let first = prot_scriber(&[
        OsStr::new("-s"),
        sprot.as_os_str(),
        OsStr::new("-o"),
        direct.as_os_str(),
    ]);
    assert!(first.status.success(), "{}", stderr(&first));
    let recorded = read(&scratch.path("direct.tsv.plan.toml"));

    // The default capture-replace pairs use ${...} for their capture groups, which is precisely
    // what a careless substitution would eat:
    assert!(
        recorded.contains("$first"),
        "this test is not testing what it thinks; the plan holds no capture group syntax:\n{}",
        recorded
    );

    let templated = scratch.write(
        "templated.plan.toml",
        &recorded
            .replacen(
                &format!("path = {:?}", sprot.display().to_string()),
                "path = \"${where}/table.txt\"",
                1,
            )
            .replacen(
                &format!("output = {:?}", direct.display().to_string()),
                "output = \"${where}/out.tsv\"",
                1,
            ),
    );
    let elsewhere = scratch.path("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::copy(&sprot, elsewhere.join("table.txt")).unwrap();

    let filled = prot_scriber(&[
        OsStr::new("--plan"),
        templated.as_os_str(),
        OsStr::new("--var"),
        OsStr::new(&format!("where={}", elsewhere.display())),
    ]);
    assert!(filled.status.success(), "{}", stderr(&filled));
    assert_eq!(
        read(&direct),
        read(&elsewhere.join("out.tsv")),
        "the plan run through --var gave a different table"
    );

    // A placeholder nothing fills in would go on to open a file called "${where}":
    let unfilled = prot_scriber(&[OsStr::new("--plan"), templated.as_os_str()]);
    assert_eq!(unfilled.status.code(), Some(2), "{}", stderr(&unfilled));
    assert!(stderr(&unfilled).contains("${where}"), "{}", stderr(&unfilled));

    // And a --var that fills nothing in is a misspelling, named rather than ignored:
    let misspelled = prot_scriber(&[
        OsStr::new("--plan"),
        templated.as_os_str(),
        OsStr::new("--var"),
        OsStr::new("wehre=somewhere"),
    ]);
    assert_eq!(misspelled.status.code(), Some(2), "{}", stderr(&misspelled));
    assert!(
        stderr(&misspelled).contains("wehre"),
        "the misspelled variable was not named:\n{}",
        stderr(&misspelled)
    );
}

/// A query that belongs to no family is a query, and when nothing could be said about it the row
/// it gets should say so. It says "unknown sequence family" instead -- but only sometimes: a
/// lonely query that appears in every input table is annotated as soon as its rows are behind it
/// and is called an unknown protein, while one that appears in only some of them waits until all
/// input has been read and is called an unknown sequence family there. The same query, the same
/// absence of a description, and a different word for it depending on which tables it happened to
/// have a hit in.
#[test]
fn a_query_of_no_family_that_could_not_be_annotated_is_called_a_protein() {
    let scratch = Scratch::new("lonely-query-fallback");
    let first = scratch.write(
        "first.tsv",
        "fam_gene\thit_a\tsp|P1|P1_ARATH alcohol dehydrogenase\n\
         lonely\thit_l\tprotein gene 12\n",
    );
    // The lonely query has no hit in this one, so it is not complete until all input has been
    // read, and it is annotated among the rest rather than as it is parsed:
    let second = scratch.write(
        "second.tsv",
        "fam_gene\thit_b\tsp|P2|P2_ARATH alcohol dehydrogenase\n",
    );
    let families = scratch.write("families.txt", "fam1\tfam_gene\n");
    let out = scratch.path("out.tsv");

    let output = prot_scriber(&[
        "-s".as_ref(),
        first.as_os_str(),
        "-s".as_ref(),
        second.as_os_str(),
        "-f".as_ref(),
        families.as_os_str(),
        "-a".as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));

    assert_eq!(
        read(&out),
        "Annotee-Identifier\tHuman-Readable-Description\n\
         fam1\talcohol dehydrogenase\n\
         lonely\tunknown protein\n"
    );
}

/// The description a run reports and the account it gives of that description have to be the same
/// description. They are produced at the same moment, by the same call, and the account is written
/// after the last thing that changes the description -- polishing -- rather than before it.
#[test]
fn an_explanation_says_what_the_table_says() {
    let scratch = Scratch::new("explain-agrees-with-table");
    let out = scratch.path("hrds.tsv");
    let annotee = "Soltu.DM.02G015700.1";

    let result = prot_scriber(&[
        OsStr::new("-s"),
        fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
        OsStr::new("-s"),
        fixture("Twelve_Proteins_vs_trembl_blastp.txt").as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
        OsStr::new("--explain"),
        OsStr::new(annotee),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));

    let reported = read(&out)
        .lines()
        .find(|row| row.starts_with(&format!("{}\t", annotee)))
        .map(|row| row.split('\t').nth(1).unwrap().to_string())
        .expect("the annotee is not in the output table");

    let explanation = stdout(&result);
    assert!(
        explanation.starts_with(&format!("== {} (query) ==\n", annotee)),
        "the explanation does not begin by naming what it explains:\n{}",
        explanation
    );
    assert!(
        explanation.contains(&format!("description  {}\n", reported)),
        "the explanation states a description other than the {:?} in the table:\n{}",
        reported,
        explanation
    );
    // The winner, what it beat, and the hits it was taken from -- everything that used to be
    // computed and dropped:
    assert!(
        explanation.contains("<- chosen"),
        "the explanation does not mark the phrase that won:\n{}",
        explanation
    );
    assert!(
        explanation.contains("word scores, best first"),
        "the explanation does not give the word scores:\n{}",
        explanation
    );
    assert!(
        explanation.contains("sp|C0LGF4|FEI1_ARATH"),
        "the explanation does not say which hits the description was chosen from:\n{}",
        explanation
    );
    // And it explains what was asked about, not everything:
    assert_eq!(
        1,
        explanation.matches("== ").count(),
        "one annotee was asked about and more than one was explained:\n{}",
        explanation
    );
}

/// A run that explains a family says which query each hit description came from, that being the
/// question a family raises and a single query does not.
#[test]
fn explaining_a_family_names_the_query_of_each_hit() {
    let scratch = Scratch::new("explain-a-family");
    let out = scratch.path("hrds.tsv");

    // The families shipped in `misc/` are for other queries than the twelve proteins, so the
    // family whose annotation is to be explained is built here:
    let families = scratch.write(
        "families.txt",
        "Family-1\tSoltu.DM.01G022510.1,Soltu.DM.01G045390.1\n",
    );

    let result = prot_scriber(&[
        OsStr::new("-s"),
        fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
        OsStr::new("-f"),
        families.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
        OsStr::new("--explain"),
        OsStr::new("Family-1"),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));

    let explanation = stdout(&result);
    assert!(
        explanation.starts_with("== Family-1 (sequence family) ==\n"),
        "the explanation does not say that a family was annotated:\n{}",
        explanation
    );
    assert!(
        explanation.contains("(hit of "),
        "the explanation does not say which query each hit was found for:\n{}",
        explanation
    );
}

/// A misspelled identifier would otherwise produce an empty explanation of a successful run, which
/// reads exactly like a query prot-scriber had nothing to say about.
#[test]
fn explaining_something_that_was_never_annotated_is_a_usage_error() {
    let scratch = Scratch::new("explain-an-unknown-annotee");
    let out = scratch.path("hrds.tsv");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
        OsStr::new("--explain"),
        OsStr::new("Soltu.DM.02G01570.1"),
    ]);

    assert_eq!(
        result.status.code(),
        Some(2),
        "asking about an identifier that does not exist was not a usage error:\n{}",
        stderr(&result)
    );
    assert_no_panic_reached_the_user(&result);
    assert!(
        stderr(&result).contains("Soltu.DM.02G01570.1"),
        "the error does not name the identifier that was not found:\n{}",
        stderr(&result)
    );
}

/// Two different things cannot both be standard output.
#[test]
fn explaining_while_the_table_goes_to_standard_output_is_a_usage_error() {
    let result = prot_scriber(&[
        OsStr::new("-s"),
        fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
        OsStr::new("--explain"),
        OsStr::new("Soltu.DM.02G015700.1"),
    ]);

    assert_eq!(
        result.status.code(),
        Some(2),
        "sending the table and the explanation to the same stream was allowed:\n{}",
        stderr(&result)
    );
    assert_eq!(
        stdout(&result),
        "",
        "the run wrote to standard output before refusing"
    );
    assert!(
        stderr(&result).contains("--explain-out"),
        "the error does not say how to resolve it:\n{}",
        stderr(&result)
    );
}

/// With `--explain-out` the explanation is a file, and standard output carries nothing at all --
/// which is what lets the table be piped while the account of it is kept.
#[test]
fn an_explanation_can_be_written_to_a_file_of_its_own() {
    let scratch = Scratch::new("explain-to-a-file");
    let explanation = scratch.path("why.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
        OsStr::new("--explain"),
        OsStr::new("Soltu.DM.02G015700.1,Soltu.DM.01G022510.1"),
        OsStr::new("--explain-out"),
        explanation.as_os_str(),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        stdout(&result).starts_with("Annotee-Identifier\t"),
        "standard output does not carry the table:\n{}",
        stdout(&result)
    );

    let written = read(&explanation);
    assert_eq!(
        2,
        written.matches("== ").count(),
        "the file does not hold an account of each annotee asked about:\n{}",
        written
    );
}

/// A table with `queries` distinct queries, each with five hits whose descriptions differ, so that
/// the account of each annotation is substantial: about 2.7 KiB of JSON per query, which is what
/// makes `explaining_every_query_costs_a_bounded_amount_of_memory` able to tell a run that writes
/// its accounts out from one that keeps them.
///
/// # Arguments
///
/// * `scratch` - Where to write the table.
/// * `queries` - How many queries to give it.
#[cfg(target_os = "linux")]
fn write_table_with_several_hits(scratch: &Scratch, queries: usize) -> PathBuf {
    const DESCRIPTIONS: [&str; 5] = [
        "cytochrome p450 monooxygenase family protein",
        "abc transporter g family member",
        "leucine rich repeat receptor like kinase",
        "serine threonine protein kinase atg",
        "ubiquitin carboxyl terminal hydrolase",
    ];
    let mut table = String::new();
    for i in 0..queries {
        for (j, description) in DESCRIPTIONS.iter().enumerate() {
            table.push_str(&format!(
                "Query-{:06}\tHit-{:06}-{}\t{}\n",
                i, i, j, description
            ));
        }
    }
    scratch.write("many_queries_and_hits.tsv", &table)
}

/// The account of an annotation holds every hit description that was scored, so it is very much
/// larger than the description it explains -- about 2.7 KiB against 40 bytes here. A run that
/// collected those accounts and wrote them at the end would need memory in proportion to its whole
/// input, and it would not show up in any test that annotates a handful of queries. So they are
/// written as they are produced, and this is what says so.
#[test]
#[cfg(target_os = "linux")]
fn explaining_every_query_costs_a_bounded_amount_of_memory() {
    const FEWER: usize = 2_000;
    const MORE: usize = 8_000;
    // Well under the ~2,750 bytes an account of one of these queries takes.
    const LIMIT_BYTES_PER_QUERY: u64 = 1_024;

    let small = Scratch::new("memory-per-explained-query-fewer");
    let large = Scratch::new("memory-per-explained-query-more");
    let fewer_peak = peak_resident_kib(
        &write_table_with_several_hits(&small, FEWER),
        &small.path("annotations.jsonl"),
        &[OsStr::new("--format"), OsStr::new("jsonl")],
    );
    let more_peak = peak_resident_kib(
        &write_table_with_several_hits(&large, MORE),
        &large.path("annotations.jsonl"),
        &[OsStr::new("--format"), OsStr::new("jsonl")],
    );

    let bytes_per_query = (more_peak.saturating_sub(fewer_peak) * 1024) / (MORE - FEWER) as u64;
    assert!(
        bytes_per_query < LIMIT_BYTES_PER_QUERY,
        "explaining {} queries instead of {} cost {} KiB instead of {} KiB, i.e. {} bytes per \
         query against a limit of {}. The accounts of the annotations are being kept rather than \
         written out as they are produced.",
        MORE,
        FEWER,
        more_peak,
        fewer_peak,
        bytes_per_query,
        LIMIT_BYTES_PER_QUERY
    );
}

/// The scored table is the ordinary table with the numbers behind each description beside it. The
/// descriptions themselves must not move: it is the same run, reported at more length.
#[test]
fn the_scored_table_adds_columns_and_changes_no_description() {
    let scratch = Scratch::new("format-tsv-scored");
    let plain = scratch.path("plain.tsv");
    let scored = scratch.path("scored.tsv");

    for (out, format) in [(&plain, "tsv"), (&scored, "tsv-scored")] {
        let result = prot_scriber(&[
            OsStr::new("-s"),
            fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
            OsStr::new("--format"),
            OsStr::new(format),
        ]);
        assert!(result.status.success(), "{}", stderr(&result));
    }

    let scored = read(&scored);
    assert!(
        scored.starts_with(
            "Annotee-Identifier\tHuman-Readable-Description\tScore\tHit-Descriptions\tPhrases\n"
        ),
        "the scored table does not name its columns:\n{}",
        scored
    );
    assert_eq!(
        read(&plain),
        scored
            .lines()
            .map(|row| row.split('\t').take(2).collect::<Vec<&str>>().join("\t"))
            .collect::<Vec<String>>()
            .join("\n")
            + "\n",
        "the scored table reports different descriptions than the plain one"
    );
    // And the numbers are the annotation's own, not placeholders:
    let first = scored.lines().nth(1).expect("the scored table has no rows");
    let columns: Vec<&str> = first.split('\t').collect();
    assert!(
        columns[2].parse::<f64>().expect("the score is not a number") > 0.0,
        "the first row scored nothing: {}",
        first
    );
    assert!(
        columns[3].parse::<usize>().expect("the hit count is not a number") > 0,
        "the first row was chosen from no hits: {}",
        first
    );
}

/// Every annotee gets a line of JSON, and the description in it is the description the ordinary
/// table reports for the same annotee. Two ways of saying the same thing about the same run.
#[test]
fn every_annotee_gets_the_same_description_in_both_formats() {
    let scratch = Scratch::new("format-jsonl");
    let plain = scratch.path("plain.tsv");
    let lines = scratch.path("annotations.jsonl");

    for (out, format) in [(&plain, "tsv"), (&lines, "jsonl")] {
        let result = prot_scriber(&[
            OsStr::new("-s"),
            fixture("Twelve_Proteins_vs_Swissprot_blastp.txt").as_os_str(),
            OsStr::new("-s"),
            fixture("Twelve_Proteins_vs_trembl_blastp.txt").as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
            OsStr::new("--format"),
            OsStr::new(format),
        ]);
        assert!(result.status.success(), "{}", stderr(&result));
    }

    let expected: Vec<(String, String)> = read(&plain)
        .lines()
        .skip(1)
        .map(|row| {
            let mut columns = row.split('\t');
            (
                columns.next().unwrap().to_string(),
                columns.next().unwrap().to_string(),
            )
        })
        .collect();

    let written = read(&lines);
    let mut reported: Vec<(String, String)> = written
        .lines()
        .map(|line| {
            let value: serde_json::Value =
                serde_json::from_str(line).unwrap_or_else(|e| panic!("{} in {:?}", e, line));
            (
                value["annotee"].as_str().expect("no annotee").to_string(),
                value["description"]
                    .as_str()
                    .expect("no description")
                    .to_string(),
            )
        })
        .collect();
    // The rows are written as the annotations happen, so they are in no particular order; each
    // row stands on its own, which is what makes sorting them a thing anyone can do.
    reported.sort();

    assert_eq!(expected, reported);

    // And each row carries the account, not only the answer. The field names are a published
    // format -- whatever reads these lines names them -- so they are pinned here, while the
    // values they carry are not, those being the run's own answer and free to change with it:
    let first: serde_json::Value = serde_json::from_str(written.lines().next().unwrap()).unwrap();
    let mut fields: Vec<&String> = first.as_object().expect("a row is not an object").keys().collect();
    fields.sort();
    assert_eq!(
        vec![
            "annotee",
            "candidates",
            "chosen",
            "description",
            "hits",
            "kind",
            "score",
            "verdict",
            "words",
        ],
        fields
    );
    let candidates = first["candidates"].as_array().expect("no candidates");
    assert!(!candidates.is_empty(), "a row states no candidate phrases: {}", first);
    let mut candidate_fields: Vec<&String> = candidates[0]
        .as_object()
        .expect("a candidate is not an object")
        .keys()
        .collect();
    candidate_fields.sort();
    assert_eq!(vec!["phrase", "score", "words"], candidate_fields);
    let hits = first["hits"].as_array().expect("no hits");
    assert!(!hits.is_empty(), "a row states no hit descriptions: {}", first);
    let mut hit_fields: Vec<&String> = hits[0]
        .as_object()
        .expect("a hit is not an object")
        .keys()
        .collect();
    hit_fields.sort();
    assert_eq!(
        vec!["description", "hit", "proposes", "query", "words"],
        hit_fields
    );
    let mut word_fields: Vec<&String> = first["words"].as_array().expect("no words")[0]
        .as_object()
        .expect("a word is not an object")
        .keys()
        .collect();
    word_fields.sort();
    assert_eq!(vec!["frequency", "score", "word"], word_fields);
}

/// `explain --stitle` puts a sequence title through the very stages an annotation run puts it
/// through, and says what each of them did to it. It is how a question about the default
/// expressions -- whether a locus code survives them, say -- becomes a command rather than an
/// argument.
#[test]
fn a_sequence_title_can_be_explained_on_its_own() {
    let result = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new(
            "sp|Q9SX12|ADH1_ARATH At2g26220 Alcohol dehydrogenase 1 OS=Arabidopsis thaliana OX=3702",
        ),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));

    let explanation = stdout(&result);
    assert!(
        explanation.contains("\ndescription  alcohol dehydrogenase\n"),
        "the title was not reduced to the description an annotation run would score:\n{}",
        explanation
    );
    assert!(
        explanation.contains("\nwords        alcohol, dehydrogenase\n"),
        "the words the description would be scored as are not stated:\n{}",
        explanation
    );
    // And each stage says which expression did what, which is the whole point:
    assert!(
        explanation.contains("OS=") && explanation.contains("filter       "),
        "the expressions that changed the title are not named:\n{}",
        explanation
    );
}

/// A title the blacklist discards never becomes a description, and the one thing worth saying
/// about it is which expression discarded it.
#[test]
fn an_explained_title_that_is_blacklisted_says_which_expression_discarded_it() {
    let result = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new("XP_006345678.1 PREDICTED: uncharacterized protein LOC102578 [Solanum tuberosum]"),
    ]);
    assert!(result.status.success(), "{}", stderr(&result));
    let explanation = stdout(&result);
    assert!(
        explanation.contains("blacklist    discarded by "),
        "a blacklisted title was explained as though it were kept:\n{}",
        explanation
    );
    assert!(
        !explanation.contains("description  "),
        "a title that never becomes a description was given one:\n{}",
        explanation
    );
}

/// The rule lists are the annotation options' own, `@NAME` and `none` included, so a candidate
/// list can be tried against the titles a database actually returns before a run is submitted.
#[test]
fn an_explanation_can_be_asked_for_with_other_expressions() {
    let stitle = "sp|Q9SX12|ADH1_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana";

    let untouched = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new(stitle),
        OsStr::new("--filter"),
        OsStr::new("none"),
        OsStr::new("--capture-replace"),
        OsStr::new("none"),
    ]);
    assert!(untouched.status.success(), "{}", stderr(&untouched));
    assert!(
        stdout(&untouched).contains(&format!("\ndescription  {}\n", stitle.to_lowercase())),
        "with no expressions at all the title should reach the scoring as it is:\n{}",
        stdout(&untouched)
    );

    let named = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new(stitle),
        OsStr::new("--filter"),
        OsStr::new("@filter-regexs-ncbi-nr"),
    ]);
    assert!(named.status.success(), "{}", stderr(&named));

    let misspelled = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new(stitle),
        OsStr::new("--filter"),
        OsStr::new("@filter-regexs-ncbi"),
    ]);
    assert_eq!(
        misspelled.status.code(),
        Some(2),
        "a misspelled built-in list was accepted:\n{}",
        stderr(&misspelled)
    );
}

/// A single dash reads the titles from standard input, so a whole search result can be put through
/// the expressions being considered without writing a file.
#[test]
fn sequence_titles_can_be_explained_from_standard_input() {
    let scratch = Scratch::new("explain-from-stdin");
    let titles = scratch.write(
        "titles.txt",
        "sp|Q9SX12|ADH1_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana\n\
         sp|P00000|X_ARATH Cytochrome P450 71A1 OS=Arabidopsis thaliana\n",
    );

    let piped = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{:?} explain --stitle - < {:?}",
            env!("CARGO_BIN_EXE_prot-scriber"),
            titles
        ))
        .current_dir(crate_root())
        .output()
        .expect("failed to run the pipeline");

    assert!(piped.status.success(), "{}", stderr(&piped));
    assert_eq!(
        2,
        stdout(&piped).matches("\nstitle       ").count() + 1,
        "both titles should have been explained:\n{}",
        stdout(&piped)
    );
}

/// What `explain --stitle` says a title contributes must be what an annotation run actually takes
/// from it. The transformations are shared, but the *order* they are applied in was stated twice --
/// once where the input is parsed and once where it is explained -- and two statements of one thing
/// drift.
#[test]
fn explaining_a_title_says_what_a_run_would_make_of_it() {
    let scratch = Scratch::new("explain-agrees-with-parsing");

    // A capture-replace pair whose replacement carries capitals. The parsing path lower-cases
    // once more *after* the pairs have been applied, which is a no-op for prot-scriber's own
    // pairs -- they introduce no capitals -- and not a no-op for this one.
    let pairs = scratch.write("pairs.txt", "(?i)\\bdehydrogenase\\b\nDH\n");

    for (name, stitle, given_pairs) in [
        // An ordinary description:
        (
            "plain",
            "sp|P00002|Y_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana",
            None,
        ),
        // Nothing is left of this one once the locus code is filtered out, so the run does not
        // keep the hit at all:
        ("emptied", "sp|P00001|X_ARATH At2g26220", None),
        // And this one never becomes a description, the blacklist having discarded it:
        (
            "blacklisted",
            "sp|P00003|Z_ARATH Putative uncharacterized protein",
            None,
        ),
        // The same title, rewritten by a pair that produces a capital letter:
        (
            "rewritten",
            "sp|P00002|Y_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana",
            Some(&pairs),
        ),
    ] {
        let mut explain_arguments: Vec<&OsStr> = vec![
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
        ];
        if let Some(pairs) = given_pairs {
            explain_arguments.push(OsStr::new("--capture-replace"));
            explain_arguments.push(pairs.as_os_str());
        }
        let explained = prot_scriber(&explain_arguments);
        assert!(explained.status.success(), "{}", stderr(&explained));
        let explained = stdout(&explained);

        // A second hit, so that the query is annotated whatever becomes of the one under test:
        let table = scratch.write(
            &format!("{}.tsv", name),
            &format!(
                "q1\th_tested\t{}\nq1\th_other\tsp|P00009|W_ARATH Cytochrome P450 71A1\n",
                stitle
            ),
        );
        let out = scratch.path(&format!("{}.tsv.out", name));
        let named_table = format!("hits={}", table.to_string_lossy());
        let named_pairs = given_pairs.map(|p| format!("hits={}", p.to_string_lossy()));
        let mut run_arguments: Vec<&OsStr> = vec![
            OsStr::new("--db"),
            OsStr::new(&named_table),
            OsStr::new("-o"),
            out.as_os_str(),
            OsStr::new("--explain"),
            OsStr::new("q1"),
        ];
        if let Some(pairs) = &named_pairs {
            run_arguments.push(OsStr::new("--db-capture-replace"));
            run_arguments.push(OsStr::new(pairs));
        }
        let run = prot_scriber(&run_arguments);
        assert!(run.status.success(), "{}", stderr(&run));
        let run = stdout(&run);

        // What the run took from the hit under test, if anything:
        let taken: Option<String> = run
            .lines()
            .position(|line| line.split_whitespace().nth(1) == Some("h_tested"))
            .and_then(|at| run.lines().nth(at + 1))
            .and_then(|line| line.trim().strip_prefix("description  ").map(str::to_string));

        // And what the explanation said it would take. Every way of contributing nothing --
        // discarded by the blacklist, or left empty by the expressions -- says so in one sentence:
        let promised: Option<String> = if explained.contains("This hit is not used at all") {
            None
        } else {
            Some(
                explained
                    .lines()
                    .find_map(|line| line.strip_prefix("description  "))
                    .expect("the explanation states no description and no reason for none")
                    .to_string(),
            )
        };

        assert_eq!(
            taken, promised,
            "for the {} title, the run took {:?} from it and the explanation promised {:?}:\n\n{}",
            name, taken, promised, explained
        );
    }
}

/// Every option that takes a list of regular expressions should take it the same way: a file, an
/// '@NAME' built-in, or 'none'. --blacklist-regexs (-b), --filter-regexs (-l) and
/// --capture-replace-pairs (-c) do. --non-informative-words-regexs (-w) and
/// --polish-capture-replace-pairs (-d) did not: '@NAME' was read as a file name of its own, and -w
/// had no way at all to say "no list".
#[test]
fn every_rule_list_option_takes_the_same_kinds_of_value() {
    let scratch = Scratch::new("source-grammar-everywhere");
    // Every word here is non-informative by prot-scriber's default list, so with that list the
    // query cannot be annotated -- and without any list at all it can.
    let table = scratch.write(
        "hits.tsv",
        "q1\th1\tprotein gene 12\nq1\th2\tprotein gene 12\n",
    );

    let run = |extra: &[&str], name: &str| -> String {
        let out = scratch.path(name);
        let mut arguments: Vec<&OsStr> = vec![
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-o"),
            out.as_os_str(),
            OsStr::new("-q"),
            OsStr::new("0.5"),
        ];
        arguments.extend(extra.iter().map(OsStr::new));
        let result = prot_scriber(&arguments);
        assert!(result.status.success(), "{:?}: {}", extra, stderr(&result));
        read(&out)
    };

    // '@NAME' is the built-in list, so it must give what giving nothing gives.
    assert_eq!(
        run(&[], "default.tsv"),
        run(
            &["-w", "@non-informative-words-regexs"],
            "at_name_w.tsv"
        ),
        "-w @non-informative-words-regexs is not the built-in list"
    );
    assert_eq!(
        run(&[], "default_d.tsv"),
        run(
            &["-d", "@polish-capture-replace-pairs"],
            "at_name_d.tsv"
        ),
        "-d @polish-capture-replace-pairs is not the built-in list"
    );

    // 'none' means no list: without it these words are non-informative and nothing can be said.
    assert!(
        run(&[], "unannotated.tsv").contains("unknown protein"),
        "the fixture is not made of non-informative words after all"
    );
    assert!(
        !run(&["-w", "none"], "no_list.tsv").contains("unknown protein"),
        "-w none still treated the words as non-informative"
    );
    // And 'none' for the polishing pairs keeps working, as it always has.
    assert!(run(&["-d", "none"], "no_polish.tsv").contains("unknown protein"));

    // A misspelled built-in is a usage error that says what there is, not a missing file.
    let misspelled = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
        OsStr::new("-w"),
        OsStr::new("@non-informative-words"),
    ]);
    assert_eq!(
        misspelled.status.code(),
        Some(2),
        "a misspelled built-in was not a usage error:\n{}",
        stderr(&misspelled)
    );
    assert!(
        stderr(&misspelled).contains("non-informative-words-regexs"),
        "the error does not say what the built-in lists are:\n{}",
        stderr(&misspelled)
    );
}

/// A tiny reference FASTA, whose headers are sequence titles as a search result carries them.
const REFERENCE_FASTA: &str = "\
>sp|P00001|A_ARATH Receptor like protein kinase 1 OS=Arabidopsis thaliana OX=3702 GN=A PE=1 SV=1
MAAAA
>sp|P00002|B_ARATH Receptor like protein kinase 2 OS=Arabidopsis thaliana OX=3702 GN=B PE=1 SV=1
MBBBB
>sp|P00003|C_ARATH Alcohol dehydrogenase OS=Arabidopsis thaliana OX=3702 GN=C PE=1 SV=1
MCCCC
";

/// A second one, sharing no accession with the first, so that adding the two corpora and counting
/// the two files together must give the same counts.
const MORE_REFERENCE_FASTA: &str = "\
>sp|P00004|D_ARATH Germin like protein 1 OS=Arabidopsis thaliana OX=3702 GN=D PE=1 SV=1
MDDDD
>sp|P00005|E_ARATH Receptor kinase OS=Arabidopsis thaliana OX=3702 GN=E PE=1 SV=1
MEEEE
";

/// The `word<TAB>count` lines of a corpus file, i.e. everything after the `#WORDS` sentinel.
fn corpus_counts(corpus: &str) -> Vec<String> {
    corpus
        .split_once("#WORDS\n")
        .unwrap_or_else(|| panic!("this is not a corpus file:\n{}", corpus))
        .1
        .lines()
        .map(|line| line.to_string())
        .collect()
}

#[test]
fn a_corpus_counts_the_words_of_a_reference_fasta() {
    let scratch = Scratch::new("corpus-build-fasta");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);
    let corpus = scratch.path("reference.corpus");

    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--name"),
        OsStr::new("reference"),
        OsStr::new("--fasta"),
        fasta.as_os_str(),
        OsStr::new("-o"),
        corpus.as_os_str(),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "building a corpus failed:\n{}",
        stderr(&result)
    );

    // The words of the three descriptions, once the default rules have had them: everything from
    // 'OS=' on is filtered away, and 'like', 'protein' and the trailing numbers are non-informative
    // -- a non-informative word is worth a fixed minimum wherever it stands, so it has no
    // frequency to count.
    assert_eq!(
        vec!["kinase\t2", "receptor\t2", "alcohol\t1", "dehydrogenase\t1"],
        corpus_counts(&read(&corpus)),
        "commonest first, and alphabetically between words counted equally often"
    );
    // The corpus says how it was made, so that a run given it can be prepared the same way:
    let text = read(&corpus);
    assert!(text.contains("[preprocessing]"), "{}", text);
    assert!(text.contains("filter_regexs = ["), "{}", text);
    assert!(text.contains("name = \"reference\""), "{}", text);
}

#[test]
fn building_the_same_corpus_twice_gives_the_same_bytes() {
    let scratch = Scratch::new("corpus-deterministic");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);

    let build = |into: &Path| {
        let result = prot_scriber(&[
            OsStr::new("corpus"),
            OsStr::new("build"),
            OsStr::new("--fasta"),
            fasta.as_os_str(),
            OsStr::new("-o"),
            into.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        read(into)
    };

    // Nothing in a corpus is a timestamp, so a corpus is a function of its input and its rules
    // alone -- which is what lets one be committed, diffed, and checked against a re-build:
    assert_eq!(build(&scratch.path("once")), build(&scratch.path("twice")));
}

#[test]
fn adding_two_corpora_gives_the_corpus_of_both_their_inputs() {
    let scratch = Scratch::new("corpus-merge");
    let one = scratch.write("one.fasta", REFERENCE_FASTA);
    let two = scratch.write("two.fasta", MORE_REFERENCE_FASTA);
    let both = scratch.write(
        "both.fasta",
        &format!("{}{}", REFERENCE_FASTA, MORE_REFERENCE_FASTA),
    );

    let build = |from: &Path, into: &Path| {
        let result = prot_scriber(&[
            OsStr::new("corpus"),
            OsStr::new("build"),
            OsStr::new("--fasta"),
            from.as_os_str(),
            OsStr::new("-o"),
            into.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    };
    build(&one, &scratch.path("one.corpus"));
    build(&two, &scratch.path("two.corpus"));
    build(&both, &scratch.path("both.corpus"));

    let merged = scratch.path("merged.corpus");
    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("merge"),
        scratch.path("one.corpus").as_os_str(),
        scratch.path("two.corpus").as_os_str(),
        OsStr::new("-o"),
        merged.as_os_str(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));

    // Counts add. That is the whole reason a corpus holds counts rather than frequencies:
    assert_eq!(
        corpus_counts(&read(&scratch.path("both.corpus"))),
        corpus_counts(&read(&merged))
    );
}

#[test]
fn adding_corpora_prepared_differently_is_a_usage_error() {
    let scratch = Scratch::new("corpus-merge-mismatch");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);

    let build = |filter: &str, into: &Path| {
        let result = prot_scriber(&[
            OsStr::new("corpus"),
            OsStr::new("build"),
            OsStr::new("--fasta"),
            fasta.as_os_str(),
            OsStr::new("--filter"),
            OsStr::new(filter),
            OsStr::new("-o"),
            into.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    };
    build("default", &scratch.path("filtered.corpus"));
    build("none", &scratch.path("unfiltered.corpus"));

    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("merge"),
        scratch.path("filtered.corpus").as_os_str(),
        scratch.path("unfiltered.corpus").as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    // Their counts are counts of different things, and a sum of them is not a frequency of
    // anything. Refused, rather than done quietly:
    assert_eq!(result.status.code(), Some(2), "{}", stdout(&result));
    assert!(
        stderr(&result).contains("not prepared the same way"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn pruning_a_corpus_keeps_its_total_true_to_what_is_left() {
    let scratch = Scratch::new("corpus-prune");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);
    let corpus = scratch.path("pruned.corpus");

    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--fasta"),
        fasta.as_os_str(),
        OsStr::new("--min-count"),
        OsStr::new("2"),
        OsStr::new("-o"),
        corpus.as_os_str(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));

    // The two words seen once are gone, and the total is the total of what is left rather than of
    // what was counted -- otherwise every frequency the corpus yields is quietly too small:
    assert_eq!(
        vec!["kinase\t2", "receptor\t2"],
        corpus_counts(&read(&corpus))
    );
    let text = read(&corpus);
    assert!(text.contains("tokens = 4"), "{}", text);
    assert!(text.contains("pruned_types = 2"), "{}", text);
    assert!(text.contains("pruned_tokens = 2"), "{}", text);

    // ... and it says that it was pruned, because what pruning removes is exactly the rarest, i.e.
    // the most specific, words there were:
    let shown = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("show"),
        corpus.as_os_str(),
    ]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    assert!(
        stdout(&shown).contains("fewer than 2 times: 2 words, 2 occurrences"),
        "{}",
        stdout(&shown)
    );
}

#[test]
fn a_corpus_too_small_to_be_a_background_says_so() {
    let scratch = Scratch::new("corpus-small");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);
    let corpus = scratch.path("small.corpus");
    let built = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--fasta"),
        fasta.as_os_str(),
        OsStr::new("-o"),
        corpus.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{}", stderr(&built));

    let shown = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("show"),
        corpus.as_os_str(),
    ]);
    // A background that is small does not merely help less: it says which words are common in the
    // sample rather than in the database, which can rank the boilerplate above the words that
    // mean something. Failing quietly is the danger, so it is said out loud:
    assert!(
        stdout(&shown).contains("small for a background corpus"),
        "{}",
        stdout(&shown)
    );
}

#[test]
fn a_corpus_build_with_nothing_to_count_is_a_usage_error() {
    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(result.status.code(), Some(2), "{}", stdout(&result));
    assert!(
        stderr(&result).contains("at least one --fasta or one --table"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn a_truncated_corpus_is_refused_rather_than_read_as_a_smaller_one() {
    let scratch = Scratch::new("corpus-truncated");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);
    let corpus = scratch.path("reference.corpus");
    let built = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--fasta"),
        fasta.as_os_str(),
        OsStr::new("-o"),
        corpus.as_os_str(),
    ]);
    assert_eq!(built.status.code(), Some(0), "{}", stderr(&built));

    // A download that stopped early, or an edit: the totals in the header no longer describe the
    // counts, and every frequency taken from it would be wrong without anything saying so.
    let text = read(&corpus);
    let truncated = scratch.write(
        "truncated.corpus",
        text.strip_suffix("dehydrogenase\t1\n").unwrap(),
    );
    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("show"),
        truncated.as_os_str(),
    ]);
    assert_eq!(result.status.code(), Some(3), "{}", stdout(&result));
    assert!(
        stderr(&result).contains("truncated or edited"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn a_subject_sequence_in_two_tables_is_counted_once() {
    let scratch = Scratch::new("corpus-subject-once");
    // The same reference sequence, S1, hit by a query in each table. A corpus counts what the
    // database says, so S1's description is one description however many searches found it --
    // otherwise a corpus built from several runs weighs the popular subjects by how popular they
    // are, which is the very bias a background is meant not to have.
    let one = scratch.write(
        "one.tsv",
        "Q1\tS1\tS1 Receptor kinase\nQ1\tS2\tS2 Alcohol dehydrogenase\n",
    );
    let two = scratch.write("two.tsv", "Q2\tS1\tS1 Receptor kinase\nQ2\tS3\tS3 Germin\n");
    let corpus = scratch.path("two-tables.corpus");

    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--table"),
        one.as_os_str(),
        OsStr::new("--table"),
        two.as_os_str(),
        OsStr::new("-o"),
        corpus.as_os_str(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));

    let mut counts = corpus_counts(&read(&corpus));
    counts.sort();
    // Every word once, S1's included -- and S1's accession is in the count as a word of its own,
    // these titles being too plain for the filter expressions to recognise an accession in:
    assert_eq!(
        vec![
            "alcohol\t1",
            "dehydrogenase\t1",
            "germin\t1",
            "kinase\t1",
            "receptor\t1",
            "s1\t1",
            "s2\t1",
            "s3\t1"
        ],
        counts
    );
}#[test]
fn the_word_list_options_take_default_like_every_other_rule_list() {
    let scratch = Scratch::new("word-list-default");
    let fasta = scratch.write("reference.fasta", REFERENCE_FASTA);

    // `--blacklist default` and `--filter default` are how a per-table option says "leave this one
    // alone", and they have always worked. An option that took a file, an '@NAME' and 'none' but
    // not 'default' is a trap rather than a simplification -- the more so because it is the one
    // spelling a script reaches for when it is filling the value in from a variable.
    let build = |source: Option<&str>, into: &Path| {
        let mut args: Vec<&OsStr> = vec![
            OsStr::new("corpus"),
            OsStr::new("build"),
            OsStr::new("--fasta"),
            fasta.as_os_str(),
            OsStr::new("-o"),
            into.as_os_str(),
        ];
        if let Some(source) = source {
            args.push(OsStr::new("--non-informative-words-regexs"));
            args.push(OsStr::new(source));
        }
        let result = prot_scriber(&args);
        assert_eq!(
            result.status.code(),
            Some(0),
            "--non-informative-words-regexs {:?} failed:\n{}",
            source,
            stderr(&result)
        );
        read(into)
    };
    assert_eq!(
        build(None, &scratch.path("implicit.corpus")),
        build(Some("default"), &scratch.path("explicit.corpus")),
        "saying 'default' is not the same as saying nothing"
    );

    // The same option of `explain`, which resolves its lists by the same code:
    let explained = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new("sp|P00001|A_ARATH Receptor like protein kinase 1"),
        OsStr::new("--non-informative-words-regexs"),
        OsStr::new("default"),
    ]);
    assert_eq!(
        explained.status.code(),
        Some(0),
        "{}",
        stderr(&explained)
    );
    assert!(stdout(&explained).contains("not scored"), "{}", stdout(&explained));
}

#[test]
fn the_number_of_threads_does_not_change_a_single_description() {
    let scratch = Scratch::new("threads");
    let trembl = fixture("Twelve_Proteins_vs_trembl_blastp.txt");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");

    // Annotation runs the annotees over a rayon thread pool. That sharing is safe is settled by
    // the compiler, `Scoring` holding nothing but shared references and `Copy` scalars. That it is
    // DETERMINISTIC is not, and it is the thing that has historically broken here: the same input
    // annotated twice produced different descriptions, because `HashMap` iteration order reached
    // an f64 summation. See the regression that upstream #50 fixes.
    let annotate = |threads: &str, into: &Path| {
        let result = prot_scriber(&[
            OsStr::new("--db"),
            OsStr::new(&format!("trembl={}", trembl.to_string_lossy())),
            OsStr::new("--db"),
            OsStr::new(&format!("sprot={}", swissprot.to_string_lossy())),
            OsStr::new("-n"),
            OsStr::new(threads),
            OsStr::new("-o"),
            into.as_os_str(),
        ]);
        assert_eq!(
            result.status.code(),
            Some(0),
            "-n {} failed:\n{}",
            threads,
            stderr(&result)
        );
        read(into)
    };

    let two = annotate("2", &scratch.path("two.tsv"));
    assert_eq!(two, annotate("8", &scratch.path("eight.tsv")));
    assert_eq!(two, annotate("2", &scratch.path("again.tsv")));
}

#[test]
fn a_sequence_title_that_is_only_an_accession_is_discarded() {
    // A GenPept entry with no description has a `stitle` that is nothing but its accession. In one
    // table of the gene-family benchmark that is 35.9 % of them, every one shaped `AAA9999999.9`
    // or `AAA99999.9`.
    //
    // Nothing removes such a title today, and the result is not that it is ignored -- it is that
    // its letters become a word. The accession-stripping rule of every filter list is `^\s*\S+\s+`,
    // which needs whitespace AFTER the first token and so never fires when there is no second one;
    // the default capture-replace pair `\b([a-z]{2,})[-.,\d]+\b -> "$first "` then rewrites
    // `can6812812.1` to `can`. `can` and `cal` are, in consequence, the two commonest words in a
    // corpus of the GenPept hits of that benchmark -- 6.3 % of it -- and the family "Outer membrane
    // protein 2, Brucella", whose every other hit is a blacklisted `hypothetical protein`, was
    // described as `aaa`.
    let cases = [
        "CAN6812812.1",
        "AAA67785.1",
        "WP_485061005.1",
        "AAD03399.1  ",
    ];
    for stitle in cases {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        assert!(
            stdout(&result).contains("blacklist    discarded"),
            "{:?} was not discarded:\n{}",
            stitle,
            stdout(&result)
        );
    }

    // A title that has an accession AND a description keeps its description: the accession is not
    // what makes a title worthless, having nothing else in it is. Read with the NCBI-NR list,
    // which is what strips a leading accession from a title that has something after it -- the
    // list a search of NR is meant to be read with, and the one the new rule sits beside.
    for (stitle, expected) in [
        ("AAM29559.1 alcohol dehydrogenase", "alcohol dehydrogenase"),
        ("CAN6812812.1 alpha glucanase", "alpha glucanase"),
    ] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
            OsStr::new("--filter"),
            OsStr::new("@filter-regexs-ncbi-nr"),
        ]);
        assert!(
            stdout(&result).contains(&format!("description  {}", expected)),
            "{:?} did not survive as {:?}:\n{}",
            stitle,
            expected,
            stdout(&result)
        );
    }

    // Nor is a bare gene symbol an accession. Two or three digits are a gene name; five or more
    // with a version suffix are an accession, and that is the whole of the distinction.
    for stitle in ["TP53", "IL6", "60S"] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
        ]);
        assert!(
            !stdout(&result).contains("blacklist    discarded"),
            "{:?} was discarded, but it is a name and not an accession:\n{}",
            stitle,
            stdout(&result)
        );
    }
}

#[test]
fn a_two_letter_gene_name_keeps_its_number() {
    // The capture-replace pair `\b([a-z]{2,})[-.,\d]+\b -> "$first "` exists to make `ADH1` and
    // `ADH2` agree on `adh`, which is what lets hits of the same family reinforce each other. With
    // a two-letter prefix it does the opposite: `CD5`, `VP2`, `UL6`, `GP4`, `IF3` and `Ac112` are
    // names in which the number IS the identity, and stripping it leaves `cd`, `vp`, `ul`, `gp`,
    // which name nothing.
    //
    // Measured on the gene-family benchmark's InterPro families: requiring three letters instead
    // of two changes 6.5 % of the descriptions, among them
    //
    //     t cell surface glycoprotein cd  ->  t cell surface glycoprotein cd5
    //     major outer capsid protein vp   ->  major outer capsid protein vp2
    //     capsid portal protein ul        ->  capsid portal protein ul6
    //     112 ac                          ->  ac112 ac113
    for (stitle, expected) in [
        ("T cell surface glycoprotein CD5", "t cell surface glycoprotein cd5"),
        ("Major outer capsid protein VP2", "major outer capsid protein vp2"),
        ("Envelope glycoprotein GP4", "envelope glycoprotein gp4"),
    ] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        assert!(
            stdout(&result).contains(&format!("description  {}", expected)),
            "{:?} did not keep its number:\n{}",
            stitle,
            stdout(&result)
        );
    }

    // What the pair is for still happens: three letters or more, and the trailing number goes, so
    // that two copies of a gene agree on the name they share.
    for (stitle, expected) in [
        ("Alcohol dehydrogenase ADH1", "alcohol dehydrogenase adh"),
        ("Peroxidase PRX-12", "peroxidase prx"),
    ] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
        ]);
        assert!(
            stdout(&result).contains(&format!("description  {}", expected)),
            "{:?} did not lose its copy number:\n{}",
            stitle,
            stdout(&result)
        );
    }
}

#[test]
fn a_locus_tag_is_removed_rather_than_split_into_two_words() {
    // A systematic locus tag is an identifier, not a description, and the splitting expression
    // treats `_` as a separator -- so `KLMA_20055` does not merely survive, it becomes TWO words,
    // `klma` and `20055`. Measured on the gene-family benchmark, family IPR014404, whose hits
    // include `Aga2p KLMA_20055 [Kluyveromyces marxianus DMKU3-1042]`:
    //
    //     0.453011310186  aga2p 20055   <- chosen
    //     0.453010310186  aga2p
    //
    // `20055` is a bare number, so it is non-informative, so it is worth +1e-6 -- and that is
    // exactly the margin by which it beat `aga2p`, which is what the reference calls that family.
    // The same tables carry `J1E43_004521`, `TTV12_gp3`, `DDB_G0273761`, `MTH_1234`, `SPPV_117`.
    for (stitle, expected) in [
        (
            "XP_022674395.1 Aga2p KLMA_20055 [Kluyveromyces marxianus DMKU3-1042]",
            "aga2p",
        ),
        ("ABC12345.1 capsid protein TTV12_gp3 [Torque teno virus]", "capsid protein"),
        ("AAA11111.1 ribosomal protein DDB_G0273761 [Dictyostelium]", "ribosomal protein"),
    ] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
            OsStr::new("--filter"),
            OsStr::new("@filter-regexs-ncbi-nr"),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        assert!(
            stdout(&result).contains(&format!("description  {}\n", expected)),
            "{:?} did not reduce to {:?}:\n{}",
            stitle,
            expected,
            stdout(&result)
        );
    }

    // The boundary is a digit. An underscore alone is not enough to call something an identifier,
    // and `CSB_alpha` -- which does occur -- is left where it is rather than guessed at.
    let result = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new("AAA11111.1 CSB_alpha subunit [Some organism]"),
        OsStr::new("--filter"),
        OsStr::new("@filter-regexs-ncbi-nr"),
    ]);
    assert!(
        stdout(&result).contains("csb"),
        "a tag with no digit in it should be left alone:\n{}",
        stdout(&result)
    );
}

#[test]
fn a_molecular_weight_is_not_part_of_a_name() {
    // `22 pif` came from `QBI90282.1 22.3kDa/pif-6`. The `22` is not meaningless -- it is the front
    // half of a molecular weight, severed from its unit by the split on `.`, and it then beat plain
    // `pif` by the non-informative constant. A mass is a measurement, not a name: it says nothing
    // about what the protein does. Around 14,700 titles across the gene-family benchmark's three
    // tables carry one.
    //
    // Asserted on the WORDS rather than the description text, because that is what reaches the
    // scoring: `22.3kDa/pif-6` leaves `/pif`, whose stray slash never becomes a word.
    for (stitle, expected) in [
        ("QBI90282.1 22.3kDa/pif-6", "pif"),
        ("AAA11111.1 43 kDa postsynaptic protein [Torpedo]", "postsynaptic, protein"),
        ("AAA11111.1 10.5kDa chaperonin [Escherichia coli]", "chaperonin"),
    ] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
            OsStr::new("--filter"),
            OsStr::new("@filter-regexs-ncbi-nr"),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        assert!(
            stdout(&result).contains(&format!("words        {}\n", expected)),
            "{:?} did not reduce to {:?}:\n{}",
            stitle,
            expected,
            stdout(&result)
        );
    }

    // A number bound into a name is untouched -- which is the distinction this rule turns on, and
    // the reason it names `kDa` rather than saying anything about digits:
    let result = prot_scriber(&[
        OsStr::new("explain"),
        OsStr::new("--stitle"),
        OsStr::new("AAA11111.1 22.3kDa T cell surface glycoprotein CD5 [Homo sapiens]"),
        OsStr::new("--filter"),
        OsStr::new("@filter-regexs-ncbi-nr"),
    ]);
    assert!(
        stdout(&result).contains("words        t, cell, surface, glycoprotein, cd5\n"),
        "the mass went but the name's own number should have stayed:\n{}",
        stdout(&result)
    );
}

#[test]
fn a_uniprot_tail_goes_whichever_of_its_tags_comes_first() {
    // Some GenPept and RefSeq entries carry a UniProt-formatted description, tail and all. The
    // generic list removes `\sOS=.*$`, but the NCBI-NR and UniRef lists have no rule for it at
    // all, and none of the three knows `OX=`, `GN=`, `PE=` or `SV=` on their own -- so a title
    // whose `OS=` has already been taken off by the organism-bracket rule keeps the rest:
    //
    //     2og fe dioxygenase family protein ox=1736528 pe=4 sv=1
    //     rna polymerase binding protein rbpa ox=76861 pe=3 sv=1
    //     ribonuclease p protein component 4128 pe=3 sv=1 ribonuclease p
    //
    // 8 of 1215 InterPro family descriptions in the gene-family benchmark.
    for filter in ["@filter-regexs-ncbi-nr", "@filter-regexs-uniref", "default"] {
        for stitle in [
            "AAA11111.1 RNA polymerase-binding protein RbpA OS=Mycobacterium OX=76861 PE=3 SV=1",
            "AAA11111.1 RNA polymerase-binding protein RbpA OX=76861 PE=3 SV=1",
            "AAA11111.1 RNA polymerase-binding protein RbpA PE=3 SV=1",
            "AAA11111.1 RNA polymerase-binding protein RbpA GN=rbpA PE=3 SV=1",
        ] {
            let result = prot_scriber(&[
                OsStr::new("explain"),
                OsStr::new("--stitle"),
                OsStr::new(stitle),
                OsStr::new("--filter"),
                OsStr::new(filter),
            ]);
            assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
            let out = stdout(&result);
            for tag in ["ox=", "pe=", "sv=", "gn=", "os="] {
                assert!(
                    !out.lines().any(|l| l.starts_with("description ") && l.contains(tag)),
                    "{:?} kept {:?} under {:?}:\n{}",
                    stitle,
                    tag,
                    filter,
                    out
                );
            }
        }
    }
}

#[test]
fn refseq_and_pdb_have_built_in_lists_of_their_own() {
    // prot-scriber shipped a list for NCBI's NR and one for UniRef, and none for RefSeq or PDB --
    // though both have a `stitle` shape of their own that the NR list knows nothing about, and both
    // are as ordinary a thing to search. So every pipeline that searched them kept a private copy,
    // and a private copy is a thing that goes stale: the gene-family benchmark's copy of the NR
    // list sat one rule behind the shipped one for months, which is `LOW QUALITY PROTEIN:` opening
    // 8.4 % of its family descriptions.
    //
    // What the two lists know that NR's does not, measured: pooling all three databases under the
    // NR list made `isoform`, `x1` and `x2` 10.6 % of the whole corpus by occurrence -- RefSeq
    // isoform boilerplate -- and `mol` and `length` 257k each, which is PDB's.
    let listed = prot_scriber(&[OsStr::new("defaults")]);
    assert_eq!(listed.status.code(), Some(0), "{}", stderr(&listed));
    for name in ["filter-regexs-refseq", "filter-regexs-pdb"] {
        assert!(
            stdout(&listed).contains(name),
            "{:?} is not among the built-in lists:\n{}",
            name,
            stdout(&listed)
        );
    }

    for (stitle, filter, expected) in [
        (
            "XP_001.1 MULTISPECIES: alcohol dehydrogenase isoform X2 [Bacteria]",
            "@filter-regexs-refseq",
            "alcohol, dehydrogenase",
        ),
        (
            "1abc_A mol:protein length:212 Alcohol dehydrogenase",
            "@filter-regexs-pdb",
            "alcohol, dehydrogenase",
        ),
        (
            "XP_001.1 LOW QUALITY PROTEIN: alcohol dehydrogenase [Homo sapiens]",
            "@filter-regexs-refseq",
            "alcohol, dehydrogenase",
        ),
    ] {
        let result = prot_scriber(&[
            OsStr::new("explain"),
            OsStr::new("--stitle"),
            OsStr::new(stitle),
            OsStr::new("--filter"),
            OsStr::new(filter),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        assert!(
            stdout(&result).contains(&format!("words        {}\n", expected)),
            "{:?} under {:?} did not reduce to {:?}:\n{}",
            stitle,
            filter,
            expected,
            stdout(&result)
        );
    }
}

#[test]
fn corpus_diff_says_what_a_rule_actually_removed() {
    let scratch = Scratch::new("corpus-diff");
    // Two entries with no description at all -- their titles are bare accessions -- and one with a
    // real one. This is the case that produced `can` and `cal` at the head of a GenPept corpus.
    let table = scratch.write(
        "hits.tsv",
        "Q1\tCAN6812812.1\tCAN6812812.1\nQ1\tCAN6812813.1\tCAN6812813.1\nQ1\tS1\tS1 Receptor kinase\n",
    );
    let build = |blacklist: &str, into: &Path| {
        let result = prot_scriber(&[
            OsStr::new("corpus"),
            OsStr::new("build"),
            OsStr::new("--table"),
            table.as_os_str(),
            OsStr::new("--blacklist"),
            OsStr::new(blacklist),
            OsStr::new("-o"),
            into.as_os_str(),
        ]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    };
    // `none` is the list as it stood before the accession rule was added to it; `default` is after.
    build("none", &scratch.path("before.corpus"));
    build("default", &scratch.path("after.corpus"));

    let result = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("diff"),
        scratch.path("before.corpus").as_os_str(),
        scratch.path("after.corpus").as_os_str(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    let report = stdout(&result);

    // The question a diff answers is "I changed a rule; what did it actually take out?" -- so the
    // word the rule removed has to be named, with how much of it went.
    assert!(report.contains("can"), "the removed word is not named:\n{}", report);
    assert!(report.contains("gone"), "a word removed entirely is not marked:\n{}", report);
    // ... and the rule that did it, since the two corpora are allowed to disagree about rules --
    // that disagreement is the whole subject of the report, unlike `merge`, which refuses it.
    assert!(
        report.contains("blacklist") && report.contains("rules"),
        "the rules that differ are not reported:\n{}",
        report
    );
    // The words that survived must not be listed as removed:
    assert!(
        !report.lines().any(|l| l.contains("receptor") && l.contains("gone")),
        "a word that survived was reported as gone:\n{}",
        report
    );
}

#[test]
fn our_own_errors_are_labelled_and_coloured_as_clap_labels_and_colours_its_own() {
    // A usage error `clap` catches and a usage error prot-scriber raises are the same thing to the
    // person reading them, and they looked different: clap writes a bold red `error:` label, ours
    // wrote the bare sentence. Which of the two layers happened to catch a mistake is not something
    // the user knows or should be able to tell.
    let missing = prot_scriber(&[
        OsStr::new("-s"),
        OsStr::new("/there/is/no/such/table.tsv"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert_eq!(missing.status.code(), Some(2), "{}", stdout(&missing));
    assert!(
        stderr(&missing).contains("error: "),
        "our own error carries no label:\n{}",
        stderr(&missing)
    );

    // Piped, both are plain -- an escape code in a log file or a CI transcript is noise:
    assert!(
        !stderr(&missing).contains('\u{1b}'),
        "colour leaked into a piped error:\n{}",
        stderr(&missing).escape_debug()
    );

    // The label is styled exactly as clap styles its own, and under the same conditions. Compared
    // rather than hard-coded, so that a clap upgrade changing the shade cannot leave the two
    // halves of prot-scriber's error output disagreeing.
    let ours = prot_scriber_with_env(
        &[("CLICOLOR_FORCE", "1")],
        &[
            OsStr::new("-s"),
            OsStr::new("/there/is/no/such/table.tsv"),
            OsStr::new("-o"),
            OsStr::new("-"),
        ],
    );
    let claps = prot_scriber_with_env(&[("CLICOLOR_FORCE", "1")], &[OsStr::new("--bogus")]);
    let label = |text: &str| -> String {
        let at = text.find("error:").unwrap_or_else(|| panic!("no label in:\n{}", text));
        let from = text[..at].rfind('\u{1b}').unwrap_or(at);
        text[from..at + "error:".len() + 4].to_string()
    };
    assert_eq!(
        label(&stderr(&claps)),
        label(&stderr(&ours)),
        "prot-scriber's own error label is not styled as clap's"
    );

    // And NO_COLOR turns ours off, as it turns clap's off:
    let no_colour = prot_scriber_with_env(
        &[("CLICOLOR_FORCE", "1"), ("NO_COLOR", "1")],
        &[
            OsStr::new("-s"),
            OsStr::new("/there/is/no/such/table.tsv"),
            OsStr::new("-o"),
            OsStr::new("-"),
        ],
    );
    assert!(
        !stderr(&no_colour).contains('\u{1b}'),
        "NO_COLOR did not silence our own error:\n{}",
        stderr(&no_colour).escape_debug()
    );
}

/// A domain family accession is one word, and reaches the description as one.
///
/// `capture_replace_pairs.txt` joins the prefix to its number -- `DUF 4228` and `DUF4228` both
/// become `duf~4228` -- so that the number stays attached to the family it names and the two
/// spellings reinforce each other. The tilde is a sentinel: it is what keeps the joined accession
/// out of the reach of the rules below it, which would otherwise strip the number as a gene name's
/// copy number or delete the whole token as a locus code.
///
/// It was also in the splitting expression's character class, so the split undid the join
/// immediately and the accession arrived as two words, the second of them a bare number:
///
///     DUF4228 domain protein  ->  duf 4228 domain protein
///
/// which is what the pair exists to prevent, and what its own comment says it does.
#[test]
fn a_domain_family_accession_reaches_the_description_as_one_word() {
    let scratch = Scratch::new("domain-family-accession");
    let table = scratch.write(
        "hits.tsv",
        "q1\ts1\tDUF4228 domain protein alpha\n\
         q1\ts2\tDUF 4228 domain protein beta\n\
         q1\ts3\tDUF4228 domain protein gamma\n",
    );
    let output = prot_scriber(&[
        OsStr::new("-s"),
        table.as_os_str(),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(
        output.status.success(),
        "the run failed:\n{}",
        stderr(&output)
    );
    let hrd = stdout(&output);
    assert!(
        hrd.contains("duf4228"),
        "the accession did not survive as one word:\n{}",
        hrd
    );
    assert!(
        !hrd.contains("duf 4228") && !hrd.contains('~'),
        "the join was undone, or its sentinel reached the description:\n{}",
        hrd
    );
}

/// An annotee none of whose hits survive the blacklist is an `unknown protein`, wherever it sits
/// in the table.
///
/// It was one when it sat anywhere but last. The loop sends a query on at the row where its
/// identifier changes, and that send is unconditional; the send that flushes the final query
/// carried an extra `!curr_query.hits.is_empty()` that the other did not, so the last query of a
/// table vanished from the output entirely if the blacklist had taken all of its hits -- no row at
/// all, rather than the `unknown protein` that `--exclude-not-annotated-queries` exists to remove.
/// A query's own annotation is not supposed to depend on which row of the file it happens to end
/// up on.
#[test]
fn an_annotee_whose_hits_are_all_blacklisted_is_unknown_wherever_it_sits() {
    let scratch = Scratch::new("all-hits-blacklisted");
    let blacklisted = "\tputative\n";
    let real = "\talcohol dehydrogenase\n";
    let first = scratch.write(
        "first.tsv",
        &format!("qa\ts1{}qa\ts2{}qz\ts3{}", blacklisted, blacklisted, real),
    );
    let last = scratch.write(
        "last.tsv",
        &format!("qa\ts1{}qz\ts2{}qz\ts3{}", real, blacklisted, blacklisted),
    );
    for (table, blank) in [(&first, "qa"), (&last, "qz")] {
        let output = prot_scriber(&[
            OsStr::new("-s"),
            table.as_os_str(),
            OsStr::new("-o"),
            OsStr::new("-"),
        ]);
        assert!(
            output.status.success(),
            "the run failed:\n{}",
            stderr(&output)
        );
        assert!(
            stdout(&output).contains(&format!("{}\tunknown protein", blank)),
            "{} lost every hit to the blacklist and then lost its row:\n{}",
            blank,
            stdout(&output)
        );
    }
}

/// The list a table gets when it names none is UniProt's, and it says so.
///
/// Every database whose titles have a shape of its own has a named list -- `@filter-regexs-pdb`,
/// `@filter-regexs-ncbi-nr`, `@filter-regexs-refseq`, `@filter-regexs-uniref`. UniProtKB's had no
/// name: it was the anonymous `filter-regexs`, which is also what every table that names no list
/// is prepared with. So the one list you could not ask for by name was the one you got by default,
/// and nothing said which database it was written for.
///
/// That is not cosmetic. Measured on 1,215 gene families, preparing RefSeq, GenPept and PDB hits
/// with it instead of with their own lists costs 0.156 precision and 0.104 F1 -- recall is
/// untouched, so nothing is lost, junk is added and precision pays for it. A user who does not
/// know that `-l` takes an `@NAME` pays that by default, and the listing gave them no reason to
/// suspect a name was missing.
#[test]
fn the_default_filter_list_is_named_for_the_database_it_is_written_for() {
    let listing = stdout(&prot_scriber(&[OsStr::new("defaults")]));
    assert!(
        listing.contains("filter-regexs-uniprot"),
        "the default filter list has no name of its own:\n{}",
        listing
    );
    assert!(
        listing.contains("filter-regexs-pdb") && listing.contains("filter-regexs-uniref"),
        "the other named lists went missing:\n{}",
        listing
    );

    // It is the same list, and it is the one a table gets when it names none.
    let named = prot_scriber(&[OsStr::new("defaults"), OsStr::new("filter-regexs-uniprot")]);
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(
        stdout(&named).contains("(OS|OX|GN|PE|SV)="),
        "'@filter-regexs-uniprot' is not the UniProt list:\n{}",
        stdout(&named)
    );

    // And the listing says that it is the default, because that is the thing that was invisible.
    let at = listing
        .find("filter-regexs-uniprot")
        .expect("just asserted it is there");
    assert!(
        listing[at..].to_lowercase().contains("default"),
        "the listing does not say which list a table gets when it names none:\n{}",
        listing
    );
}

/// A filter list that does not fit the titles it is applied to is said out loud.
///
/// The list a table gets when it names none is UniProtKB's, and on titles of another shape it is
/// worth 0.156 precision (measured over 1,215 gene families). Nothing said so: the run succeeded,
/// the descriptions came out carrying `mol protein length 187`, and only reading them showed it.
///
/// What is reported is a COMPARISON MADE ON THE USER'S OWN TITLES, never a guess about which
/// database they came from: how many characters the list in use removes per title, against how
/// many the best of prot-scriber's own removes. A guess can be wrong -- older NCBI titles carry
/// `gi|…|ref|…|` and would look like UniProt's shape, a concatenated table has no single right
/// answer -- and a measurement of the user's own data cannot be.
///
/// Measured separation: on PDB titles the UniProt list removes 6.5 characters against the PDB
/// list's 31.4 (4.8x), and on GenPept titles 0.6 against 11.8 (20x), while RefSeq's list and NR's
/// are within 4 % of each other on RefSeq titles -- which is why the threshold is a FACTOR and why
/// it is well clear of the pair that does not need telling apart.
#[test]
fn a_filter_list_that_does_not_fit_the_titles_is_reported() {
    let scratch = Scratch::new("filter-list-fit");
    // PDB's shape: '<id> mol:protein length:NNN <description>'.
    let mut rows = String::new();
    for i in 0..200 {
        rows.push_str(&format!(
            "q{}\t1abc_A\t1ABC_A mol:protein length:{} Alcohol dehydrogenase\n",
            i,
            100 + i
        ));
    }
    let table = scratch.write("pdb_shaped.tsv", &rows);

    // Given no list, the table is prepared with UniProt's, which leaves the PDB prefix standing.
    let unnamed = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("pdb={}", table.display())),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(
        unnamed.status.success(),
        "a badly fitting list is a warning, never a failure:\n{}",
        stderr(&unnamed)
    );
    let said = stderr(&unnamed);
    assert!(
        said.contains("pdb") && said.contains("filter-regexs-pdb"),
        "nothing was said about a list that does not fit, or it did not name the table and the \
         list that fits better:\n{}",
        said
    );

    // Given the list that fits, nothing is said.
    let named = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("pdb={}", table.display())),
        OsStr::new("--db-filter"),
        OsStr::new("pdb=@filter-regexs-pdb"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(
        !stderr(&named).contains("filter"),
        "the list that fits best was still complained about:\n{}",
        stderr(&named)
    );

    // And 'none' is a deliberate choice, so it is left alone.
    let none = prot_scriber(&[
        OsStr::new("--db"),
        OsStr::new(&format!("pdb={}", table.display())),
        OsStr::new("--db-filter"),
        OsStr::new("pdb=none"),
        OsStr::new("-o"),
        OsStr::new("-"),
    ]);
    assert!(none.status.success(), "{}", stderr(&none));
    assert!(
        !stderr(&none).contains("filter"),
        "'none' is a choice, not a mistake, and must not be second-guessed:\n{}",
        stderr(&none)
    );
}

/// `corpus build` says it too, and it is the place that needs saying most.
///
/// A run prepared with the wrong filter list produces visibly odd descriptions, so it can be
/// caught by reading the output. A CORPUS counted with the wrong list is silently wrong and stays
/// wrong: its counts are counts of whatever the list failed to strip, and reading its commonest
/// words -- which is what a corpus is for -- then finds the list's failures rather than the
/// database's vocabulary. Counting Swiss-Prot under the
/// PDB list took its vocabulary from 32,694 words to 89,005 -- all of it organism names and tags
/// that the wrong list left standing -- and nothing said so.
#[test]
fn corpus_build_says_when_the_filter_list_does_not_fit() {
    let scratch = Scratch::new("corpus-fit");
    let mut fasta = String::new();
    for i in 0..200 {
        fasta.push_str(&format!(
            ">1ABC_A mol:protein length:{} Alcohol dehydrogenase\n\
             MKVAAL\n",
            100 + i
        ));
    }
    let reference = scratch.write("pdb_shaped.fasta", &fasta);

    let unnamed = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--name"),
        OsStr::new("mydb"),
        OsStr::new("--fasta"),
        reference.as_os_str(),
        OsStr::new("-o"),
        scratch.path("unnamed.corpus").as_os_str(),
    ]);
    assert!(
        unnamed.status.success(),
        "a badly fitting list is a warning here too, never a failure:\n{}",
        stderr(&unnamed)
    );
    assert!(
        stderr(&unnamed).contains("filter-regexs-pdb"),
        "corpus build counted words under a list that does not fit its titles and said nothing:\n{}",
        stderr(&unnamed)
    );

    let named = prot_scriber(&[
        OsStr::new("corpus"),
        OsStr::new("build"),
        OsStr::new("--name"),
        OsStr::new("mydb"),
        OsStr::new("--fasta"),
        reference.as_os_str(),
        OsStr::new("--filter"),
        OsStr::new("@filter-regexs-pdb"),
        OsStr::new("-o"),
        scratch.path("named.corpus").as_os_str(),
    ]);
    assert!(named.status.success(), "{}", stderr(&named));
    assert!(
        !stderr(&named).contains("filter-regexs"),
        "the list that fits was complained about:\n{}",
        stderr(&named)
    );
}
