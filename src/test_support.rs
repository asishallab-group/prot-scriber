//! Helpers shared by the unit tests in `src/`.

use std::path::PathBuf;

/// A path to write a test's output to, in a directory that is certain to exist.
///
/// Not `./target`, and not the source tree either, which is what these tests used to write into.
/// `cargo test` runs with the crate root as its working directory but puts artifacts wherever
/// `CARGO_TARGET_DIR` says, so a developer who has that set -- to share one build cache between
/// checkouts, which is what it is for -- has no `./target` at all, and two tests failed for a
/// reason that had nothing to do with what they were testing.
///
/// # Arguments
///
/// * `name` - Something unique to the calling test, so that tests running on parallel threads do
///   not write over each other.
pub fn scratch_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("prot-scriber-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("could not create the scratch directory");
    dir.join(name)
}
