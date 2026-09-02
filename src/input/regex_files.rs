//! Parsing of the regular expression files prot-scriber accepts on its command line, i.e. the
//! arguments `--blacklist-regexs`, `--filter-regexs` and `--capture-replace-pairs`. The regular
//! expressions parsed here replace prot-scriber's respective compiled in defaults (see
//! `crate::default`) and are applied in `crate::description`.

use crate::error::Error;
use regex::Regex;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::ops::Deref;
use std::sync::Arc;

/// Where one expression came from: the list it stands in, and the line of that list.
///
/// An expression is not identified by its own text. `(?i)\bprobable\b` stands in
/// `blacklist-regexs` line 11 and in `filter-regexs-uniprot` line 72, and the two mean opposite
/// things -- throw the hit away, and delete a word. Printed as its source text alone they are one
/// string, and a reader can neither tell which list is talking nor go and edit it without grepping
/// every list for an expression that can run to a hundred characters.
///
/// The list is named the way the user can reach it: the name `prot-scriber defaults` prints for a
/// built-in one, the path for their own file. The line is the FILE's own, counting comments and
/// blank lines and counting from one, because it exists to send someone to the right line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The list, as the user can ask for it.
    pub list: Arc<str>,
    /// The one-based line of that list.
    pub line: usize,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.list, self.line)
    }
}

/// A parsed rule list that remembers where each of its expressions came from.
///
/// It `Deref`s to `[Regex]`, so everything that only wants to apply the expressions -- which is
/// every hot path -- goes on taking `&[Regex]` and does not know this type exists.
#[derive(Debug, Clone, Default)]
pub struct RuleList {
    regexs: Vec<Regex>,
    origins: Vec<Origin>,
}

// `Regex` is another crate's type and does not implement `PartialEq`, so equality is by the
// expressions as written -- which is the only sense in which two rule lists are the same list.
impl PartialEq for RuleList {
    fn eq(&self, other: &Self) -> bool {
        self.origins == other.origins && self == &other.regexs
    }
}

impl RuleList {
    /// Where the `i`th expression came from, if it is one of these.
    ///
    /// # Arguments
    ///
    /// * `i` - The index of the expression, as `Deref` hands them out.
    pub fn origin(&self, i: usize) -> Option<&Origin> {
        self.origins.get(i)
    }

    /// A list of already-compiled expressions, placed in `source` at the positions they hold in it.
    ///
    /// For expressions that were not read from a list of lines -- a replayed run plan records the
    /// expressions themselves, not the file they came from -- so the line is the position and
    /// `source` says what it is a position in.
    ///
    /// # Arguments
    ///
    /// * `regexs` - The expressions, in order.
    /// * `source` - What names the place they came from.
    pub fn of(regexs: Vec<Regex>, source: impl Into<Arc<str>>) -> RuleList {
        let list: Arc<str> = source.into();
        let origins = (1..=regexs.len())
            .map(|line| Origin {
                list: Arc::clone(&list),
                line,
            })
            .collect();
        RuleList { regexs, origins }
    }
}

impl Deref for RuleList {
    type Target = [Regex];

    fn deref(&self) -> &[Regex] {
        &self.regexs
    }
}

impl PartialEq<Vec<Regex>> for RuleList {
    fn eq(&self, other: &Vec<Regex>) -> bool {
        self.regexs.len() == other.len()
            && self
                .regexs
                .iter()
                .zip(other)
                .all(|(a, b)| a.as_str() == b.as_str())
    }
}

/// Reads the whole of the file at `path` into memory and parses it with `parse_rules`, keeping
/// where each expression came from.
///
/// # Arguments
///
/// * `path` - The path to the file containing one regular expression per line.
pub fn parse_rule_file(path: &str) -> Result<RuleList, Error> {
    parse_rules(&slurp(path)?, path)
}

/// The same as `parse_regexs`, keeping the list and line each expression came from.
///
/// # Arguments
///
/// * `content` - The text to parse, one regular expression per line.
/// * `source` - What names this list: a path, or a built-in list's name. A leading `@` is dropped,
///   so that a list reached as `@filter-regexs-pdb` and the same list reached as the default print
///   the same way.
pub fn parse_rules(content: &str, source: &str) -> Result<RuleList, Error> {
    let list: Arc<str> = Arc::from(source.strip_prefix('@').unwrap_or(source));
    let mut regexs = vec![];
    let mut origins = vec![];
    for (line, regex_line) in rules(content) {
        match Regex::new(regex_line) {
            Ok(regex) => {
                regexs.push(regex);
                origins.push(Origin {
                    list: Arc::clone(&list),
                    line,
                });
            }
            Err(e) => return Err(Error::MalformedData(format!("\n\n{:?} in file {:?} line {}. Could not parse the line into a Rust regular expression\n\n", e, source, line))),
        }
    }
    Ok(RuleList { regexs, origins })
}

/// A parsed list of capture-replace pairs that remembers where each pair came from.
///
/// `Deref`s to the pairs, for the same reason `RuleList` does: applying them is the hot path and
/// has no use for the origins.
#[derive(Debug, Clone, Default)]
pub struct PairList {
    pairs: Vec<(fancy_regex::Regex, String)>,
    origins: Vec<Origin>,
}

impl PairList {
    /// Where the `i`th pair came from, if it is one of these.
    ///
    /// # Arguments
    ///
    /// * `i` - The index of the pair, as `Deref` hands them out.
    pub fn origin(&self, i: usize) -> Option<&Origin> {
        self.origins.get(i)
    }

    /// A list of already-compiled pairs, placed in `source` at the positions they hold in it. See
    /// `RuleList::of`, which this mirrors.
    ///
    /// # Arguments
    ///
    /// * `pairs` - The pairs, in order.
    /// * `source` - What names the place they came from.
    pub fn of(pairs: Vec<(fancy_regex::Regex, String)>, source: impl Into<Arc<str>>) -> PairList {
        let list: Arc<str> = source.into();
        let origins = (1..=pairs.len())
            .map(|line| Origin {
                list: Arc::clone(&list),
                line,
            })
            .collect();
        PairList { pairs, origins }
    }
}

impl Deref for PairList {
    type Target = [(fancy_regex::Regex, String)];

    fn deref(&self) -> &[(fancy_regex::Regex, String)] {
        &self.pairs
    }
}

/// Reads the file at `path` and parses it with `parse_pairs`, keeping where each pair came from.
///
/// # Arguments
///
/// * `path` - The path to the file holding the pairs.
pub fn parse_pair_file(path: &str) -> Result<PairList, Error> {
    parse_pairs(&slurp(path)?, path)
}

/// Reads the whole of the file at `path` into memory and parses it with `parse_regexs`.
///
/// # Arguments
///
/// * `path` - A `&str` representing the path to the file containing one regular expression per
///   line.
pub fn parse_regex_file(path: &str) -> Result<Vec<Regex>, Error> {
    parse_regexs(&slurp(path)?, path)
}

/// Converts each line of `content` into an instance of `Regex` and returns a vector of the so
/// instantiated regular expressions.
///
/// This is where prot-scriber's own default lists come from, too: they are the files under
/// `assets/`, compiled into the binary with `include_str!` and parsed right here. There is no
/// second copy of them written out as Rust literals, so a built-in list and the file that
/// documents it cannot say different things.
///
/// # Arguments
///
/// * `content` - The text to parse, one regular expression per line.
/// * `source` - What to name in an error message; a file path, or the built-in list's name.
pub fn parse_regexs(content: &str, source: &str) -> Result<Vec<Regex>, Error> {
    Ok(parse_rules(content, source)?.regexs)
}

/// The lines of a rule list that are rules, each with the number of the line it came from.
///
/// A blank line is not a rule, and this is not merely tidiness: every line used to become an
/// expression, so an empty line became `Regex::new("")`, which matches at every position. In a
/// filter list that is a wasted pass over every description; in a blacklist it discards every
/// description there is, and a run ends with nothing annotated and nothing saying why.
///
/// A line whose first non-blank character is `#` is a comment. These lists are documentation as
/// much as configuration -- `prot-scriber defaults` prints them, and the manual says to write one
/// out and edit it -- and several of the expressions in them cannot be read without a sentence
/// saying what they are for. An expression may still match a literal `#`, just not open with a
/// bare one: `[#]` is the form the lists recommend, being the one no regex dialect can read as
/// anything else, and `\#`, `(#)` and `(?:#)` work too. See
/// `an_expression_can_match_a_literal_hash`.
///
/// The line numbers are the file's own, counting the comments and the blanks and counting from
/// one, because they exist to send someone to the right line of the right file.
///
/// # Arguments
///
/// * `content` - The text of a rule list.
fn rules(content: &str) -> impl Iterator<Item = (usize, &str)> {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .filter(|(_, line)| !is_skipped(line))
}

/// Reads the file at `path` in full, distinguishing a file that is not there from one that cannot
/// be read.
///
/// The whole file is taken at once on purpose. These files hold a few dozen short lines, and
/// reading them line by line is how `-b <directory>` used to spin for ever: on Linux opening a
/// directory succeeds and only the read fails, and a line iterator hands back the same error
/// without ever advancing.
fn slurp(path: &str) -> Result<String, Error> {
    let mut file = File::open(path)
        .map_err(|e| Error::opening(path, format!("No such file {:?}", path), &e))?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|e| Error::reading(path, &e))?;
    Ok(content)
}

/// The same, keeping the list and the line each pair's EXPRESSION stands on.
///
/// A pair is two lines and only one of them can be the line, so it is the expression's: that is the
/// half a reader recognises and the half they came to find.
///
/// # Arguments
///
/// * `content` - The text to parse, an expression and its replacement on alternating lines.
/// * `source` - What names this list: a path, or a built-in list's name, `@` dropped as in
///   `parse_rules`.
pub fn parse_pairs(content: &str, source: &str) -> Result<PairList, Error> {
    let list: Arc<str> = Arc::from(source.strip_prefix('@').unwrap_or(source));
    let mut regex_replace_tuples: Vec<(fancy_regex::Regex, String)> = vec![];
    let mut origins: Vec<Origin> = vec![];
    let mut lines = content.lines().enumerate().map(|(i, line)| (i + 1, line));
    // A comment or a blank line where an EXPRESSION is expected is skipped, so that a list can be
    // commented and its pairs separated for reading.
    while let Some((line_number, regex_line)) = lines.by_ref().find(|(_, line)| !is_skipped(line)) {
        let regex = fancy_regex::Regex::new(regex_line).map_err(|_| {
            Error::MalformedData(format!(
                "\n\nCannot read {:?} line {}: {:?} is not a regular expression (Rust syntax).\n\n",
                source, line_number, regex_line
            ))
        })?;
        // ... but the very next line is the REPLACEMENT, taken exactly as it stands. A blank
        // replacement means "delete what matched" and a single space means "replace it with one",
        // both of which the shipped pairs use, so neither may be skipped as empty. A comment is
        // still a comment: a replacement is literal text rather than an expression, so one that
        // has to begin with `#` is the one thing this format cannot say. No shipped pair needs it.
        let replacement = lines.by_ref().find(|(_, line)| !is_comment(line));
        match replacement {
            Some((_, replacement)) => {
                regex_replace_tuples.push((regex, replacement.to_string()));
                origins.push(Origin {
                    list: Arc::clone(&list),
                    line: line_number,
                });
            }
            None => {
                return Err(Error::MalformedData(format!(
                    "\n\nThe --capture-replace-pairs (-c) argument file {:?} ends with the expression on line {}, which has no replacement after it. Every expression needs the line below it to say what to replace what it matched with; that line may be empty, meaning delete it. See --help (-h) for more details.\n\n",
                    source, line_number
                )))
            }
        }
    }
    Ok(PairList {
        pairs: regex_replace_tuples,
        origins,
    })
}

/// Whether a line of a rule list carries no rule: blank, or a comment.
fn is_skipped(line: &str) -> bool {
    line.trim_start().is_empty() || is_comment(line)
}

/// Whether a line of a rule list is a comment, i.e. its first non-blank character is `#`.
fn is_comment(line: &str) -> bool {
    line.trim_start().starts_with('#')
}

#[cfg(test)]
mod tests {
    use super::{
        parse_regex_file, parse_pair_file, parse_pairs, parse_regexs,
    };
    use crate::default::{
        BLACKLIST_STITLE_REGEXS, CAPTURE_REPLACE_DESCRIPTION_PAIRS, FILTER_REGEXS,
        NON_INFORMATIVE_WORDS_REGEXS, POLISH_CAPTURE_REPLACE_PAIRS,
    };
    use pretty_assertions::assert_eq;

    /// Forces every built-in list, which is what parses them, and pins how many expressions each
    /// one holds. The counts are here so that an `assets/` file losing lines -- to a bad merge, or
    /// to an editor that stops at the first blank line -- fails a test instead of quietly
    /// annotating with less filtering than it should.
    #[test]
    fn the_built_in_lists_parse() {
        assert_eq!(NON_INFORMATIVE_WORDS_REGEXS.len(), 10);
        assert_eq!(BLACKLIST_STITLE_REGEXS.len(), 12);
        // 27 until 02.09.2026, when the six expressions that also stood in the blacklist were
        // removed from every filter list: the blacklist decides whether a hit is worth
        // anything, a filter decides which of a kept hit's words survive, and the same word in
        // both means the filter copy can never fire.
        assert_eq!(FILTER_REGEXS.len(), 21);
        // These two have no `default` of their own -- they are reached only through `@NAME`, which
        // is why nothing forced them here until 25.08.2026, and why an edit to either could have
        // lost a line without a test noticing:
        let named = |content, name| parse_regexs(content, name).unwrap().len();
        assert_eq!(named(crate::assets::FILTER_STITLE_REGEXS_NCBI_NR, "ncbi-nr"), 18);
        assert_eq!(named(crate::assets::FILTER_STITLE_REGEXS_UNIREF, "uniref"), 17);
        assert_eq!(named(crate::assets::FILTER_STITLE_REGEXS_REFSEQ, "refseq"), 21);
        assert_eq!(named(crate::assets::FILTER_STITLE_REGEXS_PDB, "pdb"), 17);
        assert_eq!(CAPTURE_REPLACE_DESCRIPTION_PAIRS.len(), 6);
        assert_eq!(POLISH_CAPTURE_REPLACE_PAIRS.len(), 2);
    }

    /// A list may carry comments and blank lines, because a list is documentation as much as it is
    /// configuration -- `prot-scriber defaults` prints it and the manual tells you to edit it --
    /// and several of the expressions in it are not readable without a sentence saying what they
    /// are for.
    #[test]
    fn a_list_may_be_commented() {
        let list = "\
# What this list is for.
(?i)\\bputative\\b

   # indented, and after a blank line
(?i)\\bprobable\\b
";
        let parsed = parse_regexs(list, "commented").unwrap();
        assert_eq!(
            vec!["(?i)\\bputative\\b", "(?i)\\bprobable\\b"],
            parsed.iter().map(|r| r.as_str()).collect::<Vec<&str>>()
        );
    }

    /// The same for the paired lists, where a comment must not be taken for one half of a pair --
    /// which would silently pair every following regular expression with the wrong replacement.
    #[test]
    fn a_pair_list_may_be_commented() {
        let list = "\
# Strip a trailing copy number, so that ADH1 and ADH2 agree on `adh`.
(?i)\\b(?P<first>[a-z]{3,})[-.,\\d]+\\b
$first

# And collapse the runs of spaces that leaves behind.
\\s{2,}
 
";
        let parsed = parse_pairs(list, "commented").unwrap();
        assert_eq!(2, parsed.len());
        assert_eq!("$first", parsed[0].1);
        assert_eq!(" ", parsed[1].1);
    }

    /// Where a blank line is a rule and where it is nothing, which is the one thing about these
    /// files that is not obvious and the one thing a reader asks about first.
    ///
    /// An expression is always exactly one line -- there is no way to continue one onto the next,
    /// and no list needs it. What varies is only whether a line is *taken* or *skipped*, and the
    /// whole rule is: a comment is always ignored, and a blank line is ignored too, EXCEPT
    /// directly after an expression in a paired list, where it is that expression's replacement
    /// and means "delete what matched". It has to be, because a replacement is allowed to be
    /// empty, and the shipped polishing pair is exactly that.
    #[test]
    fn a_blank_line_is_a_replacement_only_where_a_replacement_is_due() {
        // Directly after an expression: it IS the replacement, and deletes.
        let deleting = parse_pairs("\\s+\n\n", "deleting").unwrap();
        assert_eq!(1, deleting.len());
        assert_eq!("", deleting[0].1);

        // Anywhere else: nothing at all, so pairs can be spaced apart for reading.
        let spaced = parse_pairs("\\s+\nX\n\n\n\\d+\nY\n", "spaced").unwrap();
        assert_eq!(2, spaced.len());
        assert_eq!(("\\s+", "X"), (spaced[0].0.as_str(), spaced[0].1.as_str()));
        assert_eq!(("\\d+", "Y"), (spaced[1].0.as_str(), spaced[1].1.as_str()));

        // A comment is ignored in both places, including between an expression and its
        // replacement, so annotating a pair cannot accidentally become the replacement:
        let commented =
            parse_pairs("# before\n\\s+\n# between\nX\n", "commented").unwrap();
        assert_eq!(1, commented.len());
        assert_eq!("X", commented[0].1);
    }

    /// Seven of the nine shipped lists are plain: one expression per line, nothing else, exactly as
    /// they were before comments existed. Only the two capture-replace lists are paired. This is
    /// worth a test because the paired form is the one with a rule to remember, and it is easy to
    /// come away thinking it applies everywhere.
    #[test]
    fn only_the_capture_replace_lists_are_paired() {
        // A plain list of two expressions is two expressions -- the second is not a replacement:
        let plain = parse_regexs("(?i)\\bputative\\b\n(?i)\\bprobable\\b\n", "plain").unwrap();
        assert_eq!(2, plain.len());

        // Every plain shipped list parses as one expression per line, an odd count included, which
        // a paired parser could not accept:
        for (name, content) in [
            ("blacklist", crate::assets::BLACKLIST_STITLE_REGEXS),
            ("filter", crate::assets::FILTER_STITLE_REGEXS_UNIPROT),
            ("ncbi-nr", crate::assets::FILTER_STITLE_REGEXS_NCBI_NR),
            ("refseq", crate::assets::FILTER_STITLE_REGEXS_REFSEQ),
            ("pdb", crate::assets::FILTER_STITLE_REGEXS_PDB),
            ("uniref", crate::assets::FILTER_STITLE_REGEXS_UNIREF),
            ("non-informative", crate::assets::NON_INFORMATIVE_WORDS_REGEXS),
        ] {
            let parsed = parse_regexs(content, name).unwrap();
            let lines = content
                .lines()
                .filter(|l| !l.trim_start().is_empty() && !l.trim_start().starts_with('#'))
                .count();
            assert_eq!(lines, parsed.len(), "{} lost or gained a line", name);
        }
    }

    /// An expression CAN match a literal `#`; it just may not open with a bare one, since that is
    /// how a comment is spelled. All of these work, in both engines -- the plain lists use `regex`
    /// and the paired ones `fancy_regex`, and an escape rule the two disagreed on would be a trap:
    ///
    ///     [#]     a character class, unambiguous in every regex dialect
    ///     \#      an escape, which both crates accept
    ///     (#)     or (?:#), a group
    ///
    /// `[#]` is what the lists recommend, because it is the one that cannot depend on how a
    /// particular engine treats an escaped punctuation character.
    #[test]
    fn an_expression_can_match_a_literal_hash() {
        for form in ["[#]", "\\#", "(#)", "(?:#)"] {
            let parsed = parse_regexs(form, "hash").unwrap();
            assert_eq!(1, parsed.len(), "{} did not parse as one expression", form);
            assert!(parsed[0].is_match("#tagged"), "{} does not match a hash", form);

            let paired = parse_pairs(&format!("{}\nX\n", form), "hash").unwrap();
            assert_eq!(1, paired.len(), "{} did not parse as one pair", form);
            assert!(
                paired[0].0.is_match("#tagged").unwrap(),
                "{} does not match a hash in a pair list",
                form
            );
        }

        // ... and a bare `#` at the front is a comment, which is the whole reason the above matters:
        assert!(parse_regexs("#tagged", "hash").unwrap().is_empty());
    }

    /// A blank line used to become `Regex::new("")`, which matches at every position. In a filter
    /// list that is merely a wasted pass; in a BLACKLIST it discards every description there is,
    /// and the run ends with nothing annotated and nothing saying why. An editor that leaves a
    /// trailing newline in the middle of a file was enough to do it.
    #[test]
    fn a_blank_line_does_not_become_an_expression_that_matches_everything() {
        let parsed = parse_regexs("(?i)\\bputative\\b\n\n(?i)\\bprobable\\b\n", "blank").unwrap();
        assert_eq!(2, parsed.len());
        assert!(!parsed.iter().any(|r| r.is_match("alcohol dehydrogenase")));
    }

    /// An error still names the line the user has to go and look at, counting comments and blanks,
    /// and counting from one as an editor does.
    #[test]
    fn a_bad_expression_is_reported_by_its_line_in_the_file() {
        let error = parse_regexs("# a comment\n\n(?i)\\bok\\b\n(\n", "bad-list").unwrap_err();
        let message = format!("{}", error);
        assert!(message.contains("line 4"), "{}", message);
        assert!(message.contains("bad-list"), "{}", message);
    }

    /// No built-in list repeats an expression. A repeat cannot change what prot-scriber produces
    /// -- the lists are folded with `replace_all`, so the second application of an expression
    /// finds nothing the first one left -- but it is a defect in a list that is published as
    /// documentation, and it costs a pass over every description of every hit.
    #[test]
    fn no_built_in_list_repeats_an_expression() {
        for (name, list) in [
            (
                "non_informative_words_regexs",
                &NON_INFORMATIVE_WORDS_REGEXS[..],
            ),
            ("blacklist_stitle_regexs", &BLACKLIST_STITLE_REGEXS[..]),
            ("filter_stitle_regexs", &FILTER_REGEXS[..]),
        ] {
            let mut seen = std::collections::HashSet::new();
            for regex in list.iter() {
                assert!(
                    seen.insert(regex.as_str()),
                    "{} holds {:?} more than once",
                    name,
                    regex.as_str()
                );
            }
        }
    }

    /// A user who passes one of the shipped files back on the command line must get exactly the
    /// default. The two reach the parser by different routes -- one through `include_str!`, the
    /// other read off disk -- and this is what says the routes agree.
    #[test]
    fn passing_a_shipped_file_back_reproduces_the_default() {
        for (path, compiled) in [
            (
                "assets/non_informative_words_regexs.txt",
                &NON_INFORMATIVE_WORDS_REGEXS[..],
            ),
            (
                "assets/blacklist_stitle_regexs.txt",
                &BLACKLIST_STITLE_REGEXS[..],
            ),
            (
                "assets/filter_stitle_regexs_UniProt.txt",
                &FILTER_REGEXS[..],
            ),
        ] {
            assert_eq!(
                parse_regex_file(path)
                    .unwrap()
                    .iter()
                    .map(|r| r.as_str())
                    .collect::<Vec<_>>(),
                compiled.iter().map(|r| r.as_str()).collect::<Vec<_>>(),
                "{} does not parse to the default it is compiled in as",
                path
            );
        }
    }

    /// The same for the two files holding pairs of lines, where the replacement string is part of
    /// the payload -- including a replacement that is the empty line.
    #[test]
    fn passing_a_shipped_pair_file_back_reproduces_the_default() {
        for (path, compiled) in [
            (
                "assets/capture_replace_pairs.txt",
                &*CAPTURE_REPLACE_DESCRIPTION_PAIRS,
            ),
            (
                "assets/polish_capture_replace_pairs.txt",
                &*POLISH_CAPTURE_REPLACE_PAIRS,
            ),
        ] {
            assert_eq!(
                parse_pair_file(path)
                    .unwrap()
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
                compiled
                    .iter()
                    .map(|(r, s)| (r.as_str(), s.as_str()))
                    .collect::<Vec<_>>(),
                "{} does not parse to the default it is compiled in as",
                path
            );
        }
    }

    #[test]
    fn a_list_that_does_not_parse_is_malformed_data_and_names_its_source() {
        let e = parse_regexs("\\b\\w+\\b\n(unclosed\n", "my_regexs.txt").unwrap_err();
        assert_eq!(e.exit_code(), crate::error::EXIT_MALFORMED_INPUT);
        let message = format!("{}", e);
        assert!(message.contains("my_regexs.txt"), "{}", message);
        // The offending line is the second one, and is now reported as line 2. It used to be
        // reported as `line <1>`, counting from zero, which is not how the editor the user is
        // about to open the file in counts. Comments and blank lines are counted too, for the
        // same reason: the number exists to send someone to the right line of the right file.
        assert!(message.contains("line 2"), "{}", message);
    }

    #[test]
    fn an_odd_number_of_lines_cannot_make_pairs() {
        let e = parse_pairs("\\s+\n \n\\d+\n", "my_pairs.txt").unwrap_err();
        assert_eq!(e.exit_code(), crate::error::EXIT_MALFORMED_INPUT);
        assert!(format!("{}", e).contains("my_pairs.txt"));
    }

    /// An empty list is a legitimate way of saying "do not filter at all", so it is not an error.
    #[test]
    fn an_empty_list_is_empty_rather_than_an_error() {
        assert!(parse_regexs("", "empty.txt").unwrap().is_empty());
        assert!(parse_pairs("", "empty.txt")
            .unwrap()
            .is_empty());
    }

    /// A file that is not there and a file that cannot be read are told apart, and neither is
    /// reported as a bug in prot-scriber. A path that simply does not exist is a mistake in the
    /// command line and earns the usage status; a path that exists but will not yield its contents
    /// is not something the user can correct by retyping.
    #[test]
    fn a_missing_file_and_an_unreadable_one_are_told_apart() {
        let missing = parse_regex_file("no/such/list.txt").unwrap_err();
        assert_eq!(missing.exit_code(), crate::error::EXIT_USAGE_ERROR);
        assert!(format!("{}", missing).contains("No such file"));

        // A directory opens successfully on Linux and only fails on the read. Reading it line by
        // line used to spin for ever; the whole file is taken at once now.
        let directory = parse_regex_file("assets").unwrap_err();
        assert_eq!(directory.exit_code(), crate::error::EXIT_IO_ERROR);
    }
}
