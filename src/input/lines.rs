//! Reading a file, or standard input, one line at a time and hashing it as it goes.
//!
//! Shared rather than written twice: the corpus builder and `explain`'s report read exactly the
//! same inputs -- a reference FASTA that may be `nr.gz` unpacked into a pipe, a search result table
//! measured in hundreds of gigabytes -- and both must hold one line at a time and neither may
//! decide the file is UTF-8. A second copy of this is a second place for those two decisions to
//! come apart.

use crate::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader};

/// Calls `visit` with every line of `path`, the line ending removed, hashing the bytes as read.
///
/// `-` is standard input, which is what makes `zcat nr.gz | ...` work. Bytes that are not valid
/// UTF-8 are replaced rather than refused: BLAST and DIAMOND titles do carry latin-1, and a hit is
/// data a search was run to obtain.
///
/// # Arguments
///
/// * `path` - The file to read, or `-` for standard input.
/// * `digest` - Updated with every byte read, so that what was read can be named afterwards.
/// * `visit` - Called with each line.
pub fn for_each_line(
    path: &str,
    digest: &mut blake3::Hasher,
    mut visit: impl FnMut(&str),
) -> Result<(), Error> {
    let reader: Box<dyn BufRead> = if path == "-" {
        Box::new(BufReader::new(io::stdin()))
    } else {
        Box::new(BufReader::new(File::open(path).map_err(|e| {
            Error::opening(path, format!("No such file {:?}", path), &e)
        })?))
    };
    let mut reader = reader;
    let mut raw: Vec<u8> = Vec::new();
    loop {
        raw.clear();
        match reader.read_until(b'\n', &mut raw) {
            Ok(0) => break,
            Ok(_) => {
                digest.update(&raw);
                let decoded = String::from_utf8_lossy(&raw);
                visit(decoded.trim_end_matches(['\n', '\r']));
            }
            Err(e) => return Err(Error::reading(path, &e)),
        }
    }
    Ok(())
}

/// A number with thousands separators, for a report a person reads.
///
/// # Arguments
///
/// * `n` - The number.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_groups_from_the_right() {
        assert_eq!("0", thousands(0));
        assert_eq!("999", thousands(999));
        assert_eq!("1,000", thousands(1000));
        assert_eq!("81,806", thousands(81_806));
        assert_eq!("1,412,880", thousands(1_412_880));
    }
}
