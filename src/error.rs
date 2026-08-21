//! Everything that can stop a run of prot-scriber, and the exit status each of those things earns.
//!
//! prot-scriber is run by shells, `Makefile` rules, workflow steps and schedulers far more often
//! than by a person watching a terminal, and all of them decide what happens next from the exit
//! status alone. So a failure has to arrive there, classified: a mistake in the command line is
//! something the caller can correct and retry, an input file that says something prot-scriber
//! cannot use is something the caller has to go and repair, and a disk that is full is neither.
//!
//! The messages carried here are the ones prot-scriber has always printed. They are careful and
//! they say the right thing; what was wrong is that they arrived as a Rust panic, telling the user
//! that prot-scriber is broken and inviting them to set `RUST_BACKTRACE` and debug it. A panic now
//! means only what a panic should mean -- a bug in prot-scriber -- and `main` says so in those
//! words and exits `EXIT_INTERNAL_ERROR`.

use std::fmt;
use std::io;

/// The command line cannot be carried out as given, `EX_USAGE` of `sysexits(3)`. The user changes
/// the command and runs it again.
pub const EXIT_USAGE_ERROR: u8 = 2;

/// An input file prot-scriber could read holds something it cannot use. The user repairs the file.
pub const EXIT_MALFORMED_INPUT: u8 = 3;

/// A bug in prot-scriber, `EX_SOFTWARE` of `sysexits(3)`. Nothing the user does to the command
/// line or to the input can help; this is a request to report it.
pub const EXIT_INTERNAL_ERROR: u8 = 70;

/// Something could not be read or written, `EX_IOERR` of `sysexits(3)`.
pub const EXIT_IO_ERROR: u8 = 74;

/// Where to report a bug in prot-scriber.
pub const ISSUES_URL: &str = "https://github.com/usadellab/prot-scriber/issues";

/// A reason a run of prot-scriber could not be completed, in the terms its caller reasons in.
/// Each variant carries the message to show the user and maps to one exit status; `main` prints
/// the one and returns the other.
#[derive(Debug)]
pub enum Error {
    /// The command line is wrong: an argument that is missing, contradictory, given the wrong
    /// number of times, or naming a file that is not there.
    Usage(String),
    /// A file prot-scriber found and read holds something it cannot use: a table that is not
    /// sorted by query identifier, a gene family line in the wrong format, a line that is not a
    /// regular expression.
    MalformedData(String),
    /// Reading or writing failed for a reason that is neither of the above.
    Io(String),
}

impl Error {
    /// The exit status this error is reported with.
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Usage(_) => EXIT_USAGE_ERROR,
            Error::MalformedData(_) => EXIT_MALFORMED_INPUT,
            Error::Io(_) => EXIT_IO_ERROR,
        }
    }

    /// Classifies a failure to open a file the user named on the command line. A path that is not
    /// there is a mistake in the command line, which is where the user corrects it, so it is a
    /// usage error and is reported with `not_found_message`, the message this call site has
    /// always printed. Any other cause -- a permission, a directory where a file was meant, an
    /// unreadable mount -- is an I/O failure, and is reported in the operating system's own
    /// words, which are the only ones that tell those cases apart.
    ///
    /// # Arguments
    ///
    /// * `path` - The path that could not be opened.
    /// * `not_found_message` - What to tell the user if the path simply is not there.
    /// * `cause` - Why the file could not be opened.
    pub fn opening(path: &str, not_found_message: String, cause: &io::Error) -> Error {
        if cause.kind() == io::ErrorKind::NotFound {
            Error::Usage(not_found_message)
        } else {
            Error::Io(format!(
                "\n\nCould not open file {:?}: {}\n\n",
                path, cause
            ))
        }
    }

    /// Reports a file that was opened successfully but could not be read to its end.
    ///
    /// # Arguments
    ///
    /// * `path` - The file being read.
    /// * `cause` - Why reading stopped.
    pub fn reading(path: &str, cause: &io::Error) -> Error {
        Error::Io(format!(
            "\n\nAn error occurred reading file {:?}: {}\n\n",
            path, cause
        ))
    }
}

impl fmt::Display for Error {
    /// Writes the message of this error, and nothing else: no variant name and no exit status,
    /// because what reaches the user must be the diagnostic that was written for them.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Usage(message) | Error::MalformedData(message) | Error::Io(message) => {
                write!(f, "{}", message)
            }
        }
    }
}

impl std::error::Error for Error {}
