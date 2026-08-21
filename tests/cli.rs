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
fn a_nonexistent_gene_family_file_does_not_exit_zero() {
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

    assert_ne!(
        result.status.code(),
        Some(0),
        "a missing gene family file was reported as success"
    );
    // TODO(stage-0): today this is a panic on the main thread, hence exit 101 and a
    // `RUST_BACKTRACE` note on stderr. It should become a typed error with a diagnostic that
    // names the file, and exit 74.
    assert_eq!(result.status.code(), Some(101));
    assert!(
        stderr(&result).contains("panicked"),
        "stderr was:\n{}",
        stderr(&result)
    );
}

#[test]
fn a_nonexistent_sequence_similarity_table_wrongly_exits_zero() {
    let scratch = Scratch::new("missing-seq-sim-table");
    let out = scratch.path("hrds.txt");
    let missing = scratch.path("there_is_no_such_table.tsv");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        missing.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    // TODO(stage-0): this is the worst of the exit code defects and the reason this harness runs
    // the binary as a process. The file is opened on a parser thread, so the panic kills that
    // thread only; `main` carries on, annotates nothing, writes nothing and exits 0. A caller
    // checking the exit status is told the run succeeded. It must become a non-zero exit.
    assert_eq!(
        result.status.code(),
        Some(0),
        "the exit code for an unreadable input table has changed -- if it is now non-zero, that \
         is the fix, and this test should be replaced"
    );
    assert!(
        stderr(&result).contains("panicked"),
        "stderr was:\n{}",
        stderr(&result)
    );
    assert!(
        !out.exists(),
        "no output file was expected, but {:?} was written",
        out
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
fn an_unwritable_output_path_wrongly_exits_zero() {
    let scratch = Scratch::new("unwritable-output");
    let swissprot = fixture("Twelve_Proteins_vs_Swissprot_blastp.txt");
    let out = scratch.path("no_such_directory").join("hrds.txt");

    let result = prot_scriber(&[
        OsStr::new("-s"),
        swissprot.as_os_str(),
        OsStr::new("-o"),
        out.as_os_str(),
    ]);

    // TODO(stage-0): the write error is only `eprintln!`ed; a run whose entire product could not
    // be stored still reports success. This must become exit 74.
    assert_eq!(
        result.status.code(),
        Some(0),
        "the exit code for a failed write has changed -- if it is now 74, that is the fix, and \
         this test should be replaced"
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
