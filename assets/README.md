# prot-scriber's built-in regular expression lists

These five files **are** prot-scriber's defaults. They are compiled into the binary and parsed by
the same code that parses a list given on the command line, so there is no second copy anywhere
that could say something else. Editing a file here and rebuilding changes what prot-scriber does;
passing one of them back with `--filter-regexs` (and friends) changes nothing.

Every line is parsed, so **these files cannot carry comments** — that is what this README is for.
Order matters throughout: the lists are applied as a left fold, so two expressions that both match
the same description do not commute.

## One regular expression per line (Rust `regex` syntax)

| file | option | what it does |
|---|---|---|
| `blacklist_stitle_regexs.txt` | `--blacklist-regexs` (`-b`) | A hit whose description matches **any** of these is discarded entirely, before anything else looks at it. |
| `filter_stitle_regexs_UniProt.txt` | `--filter-regexs` (`-l`) | Each match is **deleted** from the description, in order. This is what strips the `sacc` identifier, the `OS=…` taxonomy tail and words that carry no meaning of their own. Written for UniProtKB titles, and **the list a table that names no other is given** — which is why it carries its database in its name like the rest. |
| `non_informative_words_regexs.txt` | `--non-informative-words-regexs` (`-w`) | A word matching any of these is not treated as informative and receives only `NON_INFORMATIVE_WORD_SCORE`. It is not removed — it can still appear in the description that wins. |

`filter_stitle_regexs_UniProt.txt` and `non_informative_words_regexs.txt` overlap by design: a word that
`filter_stitle_regexs_UniProt.txt` deletes never reaches scoring at all, so it does not need an entry in
`non_informative_words_regexs.txt` as well.

## Pairs of lines (fancy-regex syntax)

The first line of each pair is a regular expression, the second is what its match is replaced
with, capture groups included. An odd number of lines is an error.

`capture_replace_pairs.txt` — `--capture-replace-pairs` (`-c`), applied to each description as it
is prepared for scoring:

1. `(?i)\b(?P<first>duf|pf|ipr|pthr|go|kegg|ec)(?P<second>[0-9:]+)\b` → `$first~$second`
   Protects InterPro, PANTHER, Pfam and KEGG identifiers from being mangled by the pairs below.
   It has to come first for that reason.
2. `(?i)\b(?P<first>[a-z]{2,})[-.,\d]+\b` → `$first ` — turns e.g. `eix2` into `eix` and `SBT4.15`
   into `SBT`, so that members of a family are recognised as the same word.
3. `(^|\s+)[-.\d]+(\s+|$)` → ` ` — deletes bare numbers such as `4`, `4.12` or `12-4`.
4. `(?i)\b(?P<first>\b\w+\b)(?P<spacer>.*)\b\k<first>\b` → `$first$spacer` — deletes repeated
   words, keeping only the first mention. The back-reference is why this list needs fancy-regex.
5. `\s{2,}` → ` ` — collapses the runs of whitespace the pairs above leave behind.

`polish_capture_replace_pairs.txt` — `--polish-capture-replace-pairs` (`-d`), applied once to the
finished description:

1. `(?i)\s*\b(and|or|the|from|to)\b\s*$` → *(empty)* — deletes a trailing conjunction or article,
   which a description that was cut short by filtering is often left ending on.

## Lists that are not here

`--description-split-regex` (`-r`) and the default header are single values rather than lists and
live in `src/default.rs`. Example inputs and expected outputs are under `misc/`, not here; nothing
in `misc/` is compiled into the binary.
