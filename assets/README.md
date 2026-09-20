# prot-scriber's built-in regular expression lists

This directory holds 10 files besides this README, and they **are** prot-scriber's defaults. They
are compiled into the binary and parsed by the same code that parses a list given on the command
line, so there is no second copy anywhere that could say something else. Editing a file here and
rebuilding changes what prot-scriber does; passing one of them back with `--db-filter` (and
friends) changes nothing.

Not every line is an expression: a line whose first non-blank character is `#` is a comment, and a
blank line is nothing. A list is documentation as much as it is configuration — `prot-scriber
defaults` prints it and the manual tells you to edit it — and all 10 of them carry comments,
because several of the expressions cannot be read without one: the rule in
`filter_stitle_regexs_UniProt.txt` that deletes an Arabidopsis-style locus code, `At2g26220`, is a
line of backslashes that says nothing about what it is for, and its comment is the whole
explanation. An expression may still match a literal `#`; write it `[#]`, which no regex dialect
can read as anything else.

Order matters throughout: the lists are applied as a left fold, so two expressions that both match
the same description do not commute.

**The expressions themselves are not repeated here**, only what each list and each pair is for. A
rule written down twice stays right until nobody looks: the entry for the copy-number pair below
went on saying `[a-z]{2,}` for the month after the rule was widened to `[a-z]{3,}` on 25.08.2026,
so that `CD5` and `VP2` keep their number. Every expression carries its own comment in its own
file, which is the copy that cannot fall behind, and `prot-scriber defaults <name>` prints it.

## One regular expression per line (Rust `regex` syntax)

| file | option | what it does |
|---|---|---|
| `blacklist_stitle_regexs.txt` | `--db-blacklist NAME=` | A hit whose description matches **any** of these is discarded entirely, before anything else looks at it. |
| `filter_stitle_regexs_UniProt.txt` | `--db-filter NAME=` | Each match is **deleted** from the description, in order. This is what strips the `sacc` identifier, the `OS=…` taxonomy tail and words that carry no meaning of their own. Written for UniProtKB titles, and **the list a table that names no other is given** — which is why it carries its database in its name like the rest. |
| `filter_stitle_regexs_NCBI_NR.txt` | `--db-filter NAME=` | The same, for titles from NCBI's non-redundant database. |
| `filter_stitle_regexs_RefSeq.txt` | `--db-filter NAME=` | The same, for titles from NCBI's RefSeq, which carry a `MULTISPECIES:` prefix and an `isoform X1` suffix. |
| `filter_stitle_regexs_PDB.txt` | `--db-filter NAME=` | The same, for the PDB's `seqres` titles, which read `<id> mol:protein length:NNN <description>`. |
| `filter_stitle_regexs_UniRef.txt` | `--db-filter NAME=` | The same, for titles from the UniRef databases. |
| `non_informative_words_regexs.txt` | `--non-informative-words-regexs` (`-w`) | A word matching any of these is not treated as informative and receives only `NON_INFORMATIVE_WORD_SCORE`. It is not removed — it can still appear in the description that wins. |

Applying one database's filter list to another's titles is worth 0.156 precision and 0.104 F1,
measured over 1,215 gene families: what it fails to strip becomes words. The run succeeds either
way, so prot-scriber measures the fit itself and says so — see `src/input/list_fit.rs`.

A word must not stand in a filter list and in `non_informative_words_regexs.txt` both, because the
two do opposite things. A filter **deletes** the word, so it is gone from the description a reader
gets; the non-informative list **keeps** it in the text and takes away its vote. Which of the two
a word wants is a question about the word, so it has one answer — and the same answer for every
database, since a filter list is per-database and the non-informative list is not: a word standing
in one filter list and in no other is deleted or scored depending on where the hit came from, which
is a property of the search database and of nothing else.

`tests/cli.rs` guards the first half of that. It probes every plain word the non-informative list
names against every shipped filter list and fails if one of them deletes the word or leaves it
scored, and it reads its two sides from the binary's two listings rather than from a copy written
out there. The second half is not guarded, and one word stands on the wrong side of it today:
`homolog` is deleted by UniProtKB's filter list and by no other database's (GitHub issue 7, open).

## Pairs of lines (fancy-regex syntax)

The first line of each pair is a regular expression, the second is what its match is replaced
with, capture groups included. An odd number of lines is an error, and the line directly after an
expression is its replacement whatever it contains — a blank one means "delete what matched", so
leave no blank line between an expression and its replacement.

### `capture_replace_pairs.txt` — `--db-capture-replace NAME=`

Applied to each description as it is prepared for scoring, in this order:

1. Joins a domain accession to its number with a `~` sentinel, so that `DUF4228`, `PF01234` and
   their kind survive the pairs below intact. It has to come first for that reason.
2. Strips a trailing copy number, so that `eix2` and `SBT4.15` are recognised as the same word as
   the rest of their family. Three letters at least, so that `CD5`, `VP2` and `SH3` keep theirs.
3. Deletes a bare number standing on its own, such as `4`, `4.12` or `12-4`.
4. Deletes a locus code sitting inside a real description — `ZYRO0A01628g`, `C1952.04c` — and
   keeps the description around it, which a blacklist rule cannot do. A run of four or more digits
   is what marks one, so `SLC25A24` and `C18orf32` are left alone.
5. Deletes a word repeated later in the same description, keeping the first mention and what lies
   between. The back-reference is why this list needs fancy-regex.
6. Collapses the runs of whitespace the pairs above leave behind.

### `polish_capture_replace_pairs.txt` — `--polish-capture-replace-pairs` (`-d`)

Applied once to the finished description:

1. Deletes a dangling conjunction or article left at the end, which a description cut short by
   filtering is often left ending on.
2. Deletes the `~` sentinel pair 1 above put in, now that it has done its work: a reader wants to
   see `duf4228`.

## Not a list at all

`titles_that_must_not_be_damaged.txt` is a fixture rather than configuration: sequence titles that
prot-scriber's rules must leave alone, which `prot-scriber explain --try` measures a candidate rule
against. It is compiled in like the rest and reachable through no option — every rule-list option
would accept it happily, `acyl [carrier protein] desaturase` being a legal character class, and
apply it as expressions, which would wreck a run quietly. Add to it whenever a class of correct
output is mistaken for a defect.

## Lists that are not here

`--description-split-regex` (`-r`) and the default header are single values rather than lists and
live in `src/default.rs`. Example inputs and expected outputs are under `misc/`, not here; nothing
in `misc/` is compiled into the binary.
