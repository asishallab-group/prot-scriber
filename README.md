# prot-scriber

Assigns short human readable descriptions (HRD) to query biological sequences using reference candidate descriptions. In this, `prot-scriber` consumes sequence similarity search (Blast or Diamond or similar) results in tabular format. A customized lexical analysis is carried out on the descriptions of these Blast Hits and a resulting HRD is assigned to the query sequences. 

`prot-scriber` can also apply the same methodology to produce HRDs for sets of biological sequences, i.e. gene families. 

## Quick Start

(This section is for the lazy :wink: [TL;DR](https://en.wikipedia.org/wiki/Wikipedia:Too_long;_didn%27t_read))

`prot-scriber` can be used to generate human readable descriptions (HRDs) for _either_ query biological _sequences_ (proteins) or for _gene-families_. We will walk you through both use cases below with ready to use example input files.

### Step 1 - Get `prot-scriber`

Depending on your operating system, download the ready to use executable from the table in section [\"Installation\"](#download-ready-to-use-executables).

### Step 2 - Sequence similarity searches with Blast or Diamond

Independent of your use-case, sequence or gene-family annotation, you need to run a sequence similarity search of your query biological sequences against reference databases. We recommend searching [UniProt](https://www.uniprot.org/downloads) Swissprot ([uniprot_sprot.fasta.gz](https://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/complete/uniprot_sprot.fasta.gz)) and trEMBL ([uniprot_trembl.fasta.gz](https://ftp.uniprot.org/pub/databases/uniprot/current_release/knowledgebase/complete/uniprot_trembl.fasta.gz)). You will need to format (Blast `makeblastdb`, Diamond `diamond makedb`) these UniProt reference databases and search them with either Blast 
```sh
blastp -db uniprot_sprot.fasta -query my_prots.fasta -num_threads 10 -out my_prots_vs_sprot.txt -outfmt "6 delim=<TAB> qacc sacc stitle"
```
(Note that the above `<TAB>` actually needs to be a tab character. Typically you type that in with \"Ctrl+v\" followed by \"Tab\".)

or Diamond 
```sh
diamond blastp -p 10 --quiet -d uniprot_sprot.fasta.dmnd -q my_prots.fasta -o my_prots_vs_sprot.txt -f 6 qseqid sseqid stitle
```
(See [the manual](#manual) section \"2.3 Example Blast or Diamond commands\" for details). For a quick test run you can assume to have carried out the searches and use the example output tables below (all files are taken from this repository's [`misc`](https://github.com/usadellab/prot-scriber/tree/master/misc) directory):

**To generate HRDs for twelve example biological sequences (proteins) use:**
* [`Twelve_Proteins_vs_Swissprot_blastp.txt`](https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/Twelve_Proteins_vs_Swissprot_blastp.txt)
* [`Twelve_Proteins_vs_trembl_blastp.txt`](https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/Twelve_Proteins_vs_trembl_blastp.txt)

**To generate HRDs for two gene-families, comprising four and three proteins, respectively, use:**
* [`families.txt`](https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/families.txt)
* [`family_prots_vs_Swissprot.txt`](https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/family_prots_vs_Swissprot.txt)
* [`family_prots_vs_trEMBL.txt`](https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/family_prots_vs_trEMBL.txt)

Please read section \"2.4 Gene Family preparation and analysis\" of [the manual](#manual) for a recipy on how to cluster biological sequences into gene-families.

### Step 3 - Assign human readable descriptions (HRDs)

Note that on Windows the below would be \"`prot-scriber.exe`\" instead of just \"`prot-scriber`\".

**To annotate biological sequences, e.g. proteins, with HRDs, use:**

```sh
prot-scriber -s Twelve_Proteins_vs_Swissprot_blastp.txt -s Twelve_Proteins_vs_trembl_blastp.txt -o Twelve_Proteins_HRDs.txt
```
Find `prot-scriber`'s output in file `Twelve_Proteins_HRDs.txt`.

**To annotate gene-families with HRDs, use:**
```sh
prot-scriber -f families.txt -s family_prots_vs_Swissprot.txt -s family_prots_vs_trEMBL.txt -o families_HRDs.txt
```
Find `prot-scriber`'s output in file `families_HRDs.txt`.

## Installation

### Download ready to use executables

You can choose to download a pre-built binary, ready to be executed, from the table below, if you want the latest stable version. Other versions can be downloaded from the [Releases page](https://github.com/usadellab/prot-scriber/releases). Have a look at the below table to know which is the version you need for your operating system and platform. 

We strongly recommend to _rename_ the downloaded release file to a simple `prot-scriber` (or `prot-scriber.exe` on Windows).

Note that on Mac OS and Unix / Linux operating systems you need to make the downloaded and renamed `prot-scriber` file _executable_. In order to achieve this, open a terminal shell, navigate (`cd`) to the directory where you saved `prot-scriber`, and execute `chmod a+x ./prot-scriber`.


|Operating System|CPU-Architecture|Release-Name (click to download)|Comment|
|---|---|---|---|
|Windows 7 or higher|any|[windows_prot-scriber.exe](https://github.com/usadellab/prot-scriber/releases/download/latest-stable/x86_64-pc-windows-gnu_prot-scriber.exe)|to be used in a terminal (`cmd` or Power-Shell)|
|any GNU-Linux|any Intel x86, 64 bits|[x86_64-unknown-linux-gnu_prot-scriber](https://github.com/usadellab/prot-scriber/releases/download/latest-stable/x86_64-unknown-linux-gnu_prot-scriber)|requires libm.so.6 (compiled with glibc version 2.27) and libc.so.6 (compiled with glibc 2.18) installed as is the case e.g. in Ubuntu >= 22.04|
|any GNU-Linux|any aarch, 64 bits|[aarch64-unknown-linux-gnu_prot-scriber](https://github.com/usadellab/prot-scriber/releases/download/latest-stable/aarch64-unknown-linux-gnu_prot-scriber)|e.g. for Raspberry Pi; requires libm.so.6 (compiled with glibc version 2.27) and libc.so.6 (compiled with glibc 2.18) installed as is the case e.g. in Ubuntu >= 22.04|
|Apple / Mac OS|any Mac Computer with Mac OS 10|[x86_64-apple-darwin_prot-scriber](https://github.com/usadellab/prot-scriber/releases/download/latest-stable/x86_64-apple-darwin_prot-scriber)||

### Compilation from source code

#### Prerequisites 

`prot-scriber` is written in Rust. That makes it extremely performant and, once compiled for your operating system (OS), can be used on any machine with that particular OS environment. To compile it for your platform you first need to have Rust and cargo installed. Follow [the official instructions](https://www.rust-lang.org/tools/install).

#### Obtain the code

Download the [latest stable release of `prot-scriber` here](https://github.com/usadellab/prot-scriber/archive/refs/tags/latest-stable.zip).

Unzip it, e.g. by double clicking it or by using the command line:
```sh
unzip latest-stable.zip
```

#### Compile `prot-scriber`

Change into the directory of the downloaded `prot-scriber` code, e.g. in a Mac OS or Linux terminal `cd prot-scriber-latest-stable` after having unpacked the latest stable release (see above).

Now, compile `prot-scriber` with
```sh
cargo build --release
```

The above compilation command has generated an executable binary file in the current directory `./target/release/prot-scriber`. You can just go ahead and use `prot-scriber` now that you have compiled it successfully (see Usage below).

#### Global installation

If you are familiar with installing self compiled tools on a system wide level, this section will provide no news to you. It is convenient to make the compiled executable `prot-scriber` program available from anywhere on your system. To achieve this, you need to copy it to any place you typically have your programs installed, or add its directory to your, our all users' `$PATH` environment. In doing so, e.g. in case you are a system administrator, you make `prot-scriber` available for all users of your infrastructure. Make sure you and your group have executable access rights to the file. You can adjust these access right with `chmod ug+x ./target/release/prot-scriber`. You, and possibly other users of your system, are now ready to run `prot-scriber`.

### Install via bioconda
In case you are using [conda](https://docs.conda.io/en/latest/) to manage your pacakges, `prot-scriber` is available on [bioconda](https://anaconda.org/bioconda/prot-scriber). Download via

```
conda install -c bioconda prot-scriber
```
or create and activate a new conda environment via
```
conda create -n prot-scriber -c bioconda -c conda-forge prot-scriber
conda activate prot-scriber
```

## Usage

`prot-scriber` is a command line tool and _must_ be used in a terminal application. On Windows that will be `cmd` or PowerShell, on Mac OS X or any Linux / Unix system that will be a standard terminal shell.

### Manual

<details>
    <summary><b>Please read the manual and command line options of the latest stable version (<i>click to expand</i>).</b></summary>

```
PLEASE USE '--help' FOR MORE DETAILS!

prot-scriber assigns human readable descriptions (HRD) to query biological sequences or sets of them
(a.k.a gene-families).

Usage: prot-scriber [OPTIONS]
       prot-scriber <COMMAND>

Commands:
  annotate
          Assign human readable descriptions to queries or families of them. The default
  defaults
          Print one of prot-scriber's built-in regular expression lists
  explain
          Show what prot-scriber makes of a sequence title, step by step
  help
          Print this message or the help of the given subcommand(s)

Options:
  -o, --output <PATH>
          Filename in which the tabular output will be stored. Give a single dash ('-') to write the
          table to standard output instead of to a file. Progress messages, warnings and errors
          always go to standard error, so the standard output carries the table and nothing else and
          'prot-scriber ... -o - | head' shows you its first rows.

  -s, --db <[NAME=]PATH>
          File in which to find sequence similarity search results in tabular format (SSST). Use
          e.g. Blast or Diamond to produce them. Required columns are: 'qacc sacc stitle' (Blast) or
          'qseqid sseqid stitle' (Diamond). (See section '2. prot-scriber input preparation' for
          more details.) If the required columns, or more, appear in different order than shown here
          you must use the --db-header argument. If any of the input SSSTs uses a different
          field-separator than the '<TAB>' character, you must provide the --db-sep argument. You
          can provide multiple SSSTs, simply by repeating the -s argument, e.g. '-s
          queries_vs_swissprot_diamond_out.txt -s queries_vs_trembl_diamond_out.txt'. Providing
          multiple --seq-sim-table (-s) arguments might imply the order in which you give other
          arguments like --db-header and --db-sep. See there for more details. All rows belonging to
          one query must stand together in the table, which is what Blast and Diamond produce on
          their own; concatenating tables or shuffling one does not preserve it, and prot-scriber
          stops with an error rather than annotate a query twice. 'sort -s -t"<TAB>" -k1,1 <table>'
          restores it, and being a stable sort on the query column alone it leaves the order of each
          query's hits alone; --unsorted-input reads such a table as it is instead, at the cost of
          memory.
          
          Give a table a name with NAME=PATH, e.g. '--db nr=at_vs_nr.tsv', and the --db-header,
          --db-sep, --db-blacklist, --db-filter and --db-capture-replace options can then say which
          table they are for by that name instead of by the order they are written in. Without a
          name a table is called after its file, so '--db at_vs_nr.tsv' is the table 'at_vs_nr'.
          
          [alias: --seq-sim-table]

      --db-header <NAME=SPEC>
          The header of one --db table, as NAME=SPEC, e.g. '--db-header nr="qacc sacc evalue
          stitle"'. Separated by spaces, the names of the columns in the order they appear in that
          table. The required columns are 'qacc sacc stitle'; Blast and Diamond terminology are both
          understood, so write 'qacc' and 'sacc' or Diamond's 'qseqid' and 'sseqid', whichever your
          search produced. Additional columns are ignored and the required ones may appear in any
          order -- what this argument does is say which column is which.

      --db-sep <NAME=CHAR>
          The field separator of one --db table, as NAME=CHAR, e.g. '--db-sep nr=\t'. The default is
          the TAB character. A separator is a single character; write '\t' or 'tab' for TAB, '\s'
          for a space and '\0' for the null byte, since a shell makes those awkward to type
          literally.

      --db-blacklist <NAME=SOURCE>
          The blacklist regular expressions for one --db table, as NAME=SOURCE. A hit description
          matching any of them is discarded whole. The value is a file, or '@NAME' for one of
          prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME'
          is the same list -- or 'none' to apply no list at all.

      --db-filter <NAME=SOURCE>
          The filter regular expressions for one --db table, as NAME=SOURCE, e.g. '--db-filter
          nr=@filter-regexs-ncbi-nr'. Substrings matching any of them are deleted from a hit
          description before it is scored. It names the table it belongs to, so it can only ever
          mean the table declared '--db nr=...', whatever order the arguments are written in. The
          value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber
          defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list
          at all.

      --db-capture-replace <NAME=SOURCE>
          The capture-replace pairs for one --db table, as NAME=SOURCE: pairs of lines, an
          expression and the replacement below it, rewriting a hit description before it is scored.
          The value is a file, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber
          defaults' prints what there is, and '@NAME' is the same list -- or 'none' to apply no list
          at all.

  -r, --description-split-regex <REGEX>
          A regular expression in Rust syntax to be used to split descriptions (`stitle` in Blast
          terminology) into words. Default is '([()\[\]{}<>+*^_\-/|\\;,':.\s]+)'. Note that this is
          an expert option.

  -q, --center-inverse-word-information-content-at-quantile <QUANTILE>
          The quantile (percentile) to be subtracted from calculated inverse word information
          content to center these values. Consequently, this must be a value between zero and one or
          literal 50, which is interpreted as mean instead of a quantile. Default is 50, implying
          centering at the mean. Note that this is an expert option.

  -v, --verbose
          Print informative messages about the annotation process.

  -w, --non-informative-words-regexs <SOURCE>
          Regular expressions (regexs) used to recognize non-informative words, which will only
          receive a minimun score in the prot-scriber process that generates human readable
          description. The value is a file with one expression per line, or '@NAME' for one of
          prot-scriber's built-in lists -- 'prot-scriber defaults' prints what there is, and '@NAME'
          is the same list -- or 'none' to hold no word non-informative at all. There is a default
          list hard-coded into prot-scriber. Write it out to start from it, with 'prot-scriber
          defaults non-informative-words-regexs > my_non_informative_words_regexs.txt'; nothing
          needs downloading, and what you get is the list this binary applies. - Note that this is
          an expert option.

  -d, --polish-capture-replace-pairs <SOURCE>
          The last step of the process generating human readable descriptions (HRDs) for the queries
          (proteins or sequence families) is to 'polish' the selected HRDs. Polishing is done by
          iterative application of regular expressions (fancy-regex) and replace instructions
          (capture-replace-pairs). If you do not want to use the default polishing capture replace
          pairs specify a file in which pairs of lines are given. Of each pair the first line hold a
          regular expression (fancy-regex syntax) and the second the replacement instructions
          providing access to capture groups. Set to 'none' or provide an empty file, if you want to
          suppress polishing, or '@NAME' for one of prot-scriber's built-in lists -- 'prot-scriber
          defaults' prints what there is. If you want a template for your custom polishing
          capture-replace-pairs, write the default out with 'prot-scriber defaults
          polish-capture-replace-pairs > my_polish_pairs.txt'. - Note that this an expert option.

  -n, --n-threads <N>
          The maximum number of parallel threads to use. Default is the number of logical cores.
          Required minimum is two (2). Note that at most one thread is used per input sequence
          similarity search result (Blast table) file. After parsing these annotation may use up to
          this number of threads to generate human readable descriptions.

      --plan <PATH>
          Run again exactly what the given run plan records: the same input tables, the same regular
          expressions written out in it, the same everything. It cannot be combined with any option
          that would configure the run, because then there would be two answers to one question and
          a rule about which of them wins -- and a precedence rule is a thing you have to know to
          read a command line. Edit the plan if you want something else; that is what it is for.

      --var <NAME=VALUE>
          Fill in a ${NAME} placeholder in a run plan's paths, e.g. '--var sample=at' for a plan
          whose table is '${sample}/hits.tsv'. One plan then serves a whole set of datasets that are
          annotated the same way, which is the usual reason to have written a plan down at all.
          
          Only paths are filled in: the input tables, the gene families file and the output. Not the
          regular expressions, where '${name}' is the ordinary way fancy-regex names a capture group
          and a blind substitution would quietly rewrite them.
          
          A placeholder left with nothing to fill it, and a --var that filled nothing in, are both
          errors: the first would read a file called '${sample}' and the second is a misspelling
          that would otherwise do nothing at all.

      --plan-out <PATH>
          Where to write the record of this run: every setting it resolved to, the regular
          expressions written out rather than named, and a BLAKE3 hash of every byte it read. Replay
          it with --plan. By default it is the --output (-o) file with '.plan.toml' after it; give
          'none' to write no plan at all, and note that no plan is written by default when the table
          goes to standard output, there being no file name to derive one from.
          
          A command line is not a record of a run: it names files whose contents change, it says
          '@filter-regexs-ncbi-nr' where what matters is the expressions that name stood for on the
          day, and it leaves out everything that was defaulted. A plan is what you commit beside a
          result.

      --dry-run
          Resolve and check the command line, report what would be done, and stop without annotating
          anything. Everything that can be found out before reading the input tables is found out:
          that every argument can be paired with the table it is for, that every file of regular
          expressions exists and parses, that every input table exists and how large it is. The
          report says which settings each table would be parsed with, and whether each of them is
          prot-scriber's default or came from the command line. Meant to be the step before
          submitting a long run, so that an hour is not spent discovering a mistake that was visible
          at the start.

      --unsorted-input
          Read input tables whose rows are not grouped by query. prot-scriber normally annotates
          each query as soon as its rows are behind it, which is what keeps the memory a run needs
          independent of how large the input is: a query's hits are dropped the moment it is
          annotated. That requires a query's rows to stand together, which is what Blast and Diamond
          produce and what concatenating tables destroys. Given this flag, prot-scriber holds every
          query until all input has been read instead, and so needs memory in proportion to the
          whole input rather than to one query. Prefer grouping the table -- 'sort -s -t"<TAB>"
          -k1,1 table' does it, and preserves the order of each query's hits -- and keep this for
          when that is not possible.

      --format <FORMAT>
          The shape of the output table.
          
          'tsv' is the identifier and the description, which is what prot-scriber has always
          written.
          
          'tsv-scored' adds what the description scored, how many hit descriptions it was chosen
          from and how many distinct phrases were proposed, so that a result can be sorted or
          thresholded by how well founded it is.
          
          'jsonl' writes one JSON object per annotee holding the whole account of how its
          description was chosen -- the same thing --explain writes for a few annotees, for all of
          them, and machine readable. It is written as the run produces it, which is what keeps the
          memory a run needs independent of the size of its input; its rows are therefore in the
          order the annotations happened rather than sorted by identifier, and 'sort' after the fact
          gives a byte-stable file, each row standing on its own.

          Possible values:
          - tsv:        Two columns, the identifier and the description
          - tsv-scored: The same two, and the score, the number of hit descriptions and the number
            of phrases
          - jsonl:      One JSON object per annotee, holding the whole account of how its
            description was chosen
          
          [default: tsv]

      --explain <ID>
          Say why these queries or families got the description they got, and not another one. Give
          the identifiers that appear in the output table: query identifiers ('qacc' in the input
          tables), or -- with --seq-families (-f) -- the names of families. Repeat the option or
          separate them with commas.
          
          What is written is the whole of the choice: every hit description that was scored and the
          words it was split into, what each informative word was worth and how often it appeared,
          every phrase that was proposed and its score, and which of them won. It is written as each
          annotee is finished and goes to standard output; --explain-out sends it to a file instead.
          
          An identifier that was never annotated is an error rather than a silence, a misspelled one
          being otherwise indistinguishable from a query prot-scriber could say nothing about.

      --explain-out <PATH>
          Write the --explain output to this file instead of to standard output. The file is created
          when the run starts rather than when it ends, so a path that cannot be written is reported
          before the annotation rather than after it.

  -x, --exclude-not-annotated-queries
          Exclude results from the output table that could not be annotated, i.e. 'unknown protein'
          or 'unknown sequence family', respectively.

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version

Gene families:
  -f, --seq-families <PATH>
          A file in which families of biological sequences are stored, one family per line. Each
          line must have format 'fam-name TAB gene1,gene2,gene3'. Make sure no gene appears in more
          than one family.

  -i, --seq-family-id-genes-separator <STRING>
          A string used as separator in the argument --seq-families (-f) gene families file. This
          string separates the gene-family-identifier (name) from the gene-identifier list that
          family comprises. Default is '<TAB>' ("\t").

  -g, --seq-family-gene-ids-separator <REGEX>
          A regular expression (Rust syntax) used to split the list of gene-identifiers in the
          argument --seq-families (-f) gene families file. Default is '(\s*,\s*|\s+)'.

  -a, --annotate-non-family-queries
          Use this option only in combination with --seq-families (-f), i.e. when prot-scriber is
          used to generate human readable descriptions for gene families. If in that context this
          flag is given, queries for which there are sequence similarity search (Blast) results but
          that are NOT member of a sequence family will receive an annotation (human readable
          description) in the output file, too. Default value of this setting is 'OFF' (false).



MANUAL 
====== 

1. Summary 
---------- 
'prot-scriber' uses reference descriptions ('stitle' in Blast terminology) from sequence similarity
search results (Blast Hits) to assign short human readable descriptions (HRD) to query biological
sequences or sets of them (a.k.a gene, or sequence, families). In this, prot-scriber consumes
sequence similarity search (Blast, Diamond, or similar) results in tabular format. A customized
lexical analysis is carried out on the descriptions ('stitle' in Blast terminology) of these Blast
Hits and a resulting HRD is assigned to the query sequences or query families, respectively. 
 
2. prot-scriber input preparation 
--------------------------------- 
This sections explains how to run your favorite sequence similarity search tool, so that it produces
tabular results in the format prot-scriber needs them. You can run sequence similarity searches with
Blast [McGinnis, S. & Madden, T. L. BLAST: at the core of a powerful and diverse set of sequence
analysis tools. Nucleic Acids Res 32, W20–W25 (2004).] or Diamond [Buchfink, B., Xie, C. & Huson, D.
H. Fast and sensitive protein alignment using DIAMOND. Nat Meth 12, 59–60 (2015).]. Note that there
are other tools to carry out sequence similarity searches which can be used to generate the input
for prot-scriber. As long as you have a tabular text file with the three required columns holding
the query identifier, the subject ('Hit') identifier, and the subject ('Hit') description ('stitle'
in Blast terminology) prot-scriber will accept this as input. 
Depending on the type of your query sequences the search method and searched reference databases
vary. For amino acid queries search protein reference databases, for nucleotide query sequences
search nucleotide reference databases. If you have protein coding nucleotide query sequences you can
choose to either search protein reference databases using translated nucleotide queries with
'blastx' or 'diamond blastx' or search reference nucleotide databases with 'blastn' or 'diamond
blastn'. Note, that before carrying out any sequence similarity searches you need to format your
reference databases. This is achieved by either the 'makeblastdb' (Blast) or 'makedb' (Diamond)
commands, respectively. Please see the respective tool's (Blast or Diamond) manual for details on
how to format your reference sequence database. 
 
2.1 A note on TAB characters 
---------------------------- 
TAB is often used as a field separator, e.g. by default in Diamond sequence similarity search result
tables, or to separate gene-family identifiers from their respective gene-lists. Consequently,
prot-scriber has several arguments that could be a TAB, e.g. the --db-sep or the
--seq-family-id-genes-separator (-i) (please see below for more details on these arguments).
Unfortunately providing the TAB character as a command line argument can be tricky. It is even more
tricky to write it into a manual like this, because it appears as a blank whitespace and cannot
easily be distiunguished from other whitespace characters. We thus write '<TAB>' whenever we mean
the TAB character. To type it in the command line and provide it as an argument to prot-scriber you
can (i) either use $'\t' (e.g. --db-sep "nr=$'\t'") or (ii) hit Ctrl+v and subsequently hit the TAB
key on your keyboard. 
 
2.2 Which reference databases to search 
--------------------------------------- 
For amino acid (protein) or protein coding nucleotide query sequences we recommend searching
UniProt's Swissprot and trEMBL. For nucleotide sequences UniRef100 and, or UniParc might be good
choices. Note that you can search _any_ database you deem to hold valuable reference sequences.
However, you might have to provide custom blacklist, filter, and capture-replace arguments for Blast
or Diamond output tables stemming from searches in these non UniProt databases (run 'prot-scriber
--help' and see the arguments --db-blacklist, --db-filter and --db-capture-replace there for further
details). If you want to search any NCBI reference database, please see section 2.2.2 for more
details. 
 
2.2.1 UniProtKB, and the list a table gets when it names none 
------------------------------ 
UniProtKB titles are 'sp|P12345|ADH1_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana OX=3702
GN=ADH1 PE=1 SV=2': an accession between pipes at the front, and a tail of 'OS=' taxonomy and 'GN='
gene tags at the back. The list that strips those is 'prot-scriber defaults filter-regexs-uniprot',
or '--db-filter <table>=@filter-regexs-uniprot'. 

IT IS ALSO THE LIST A TABLE IS PREPARED WITH WHEN NO OTHER IS NAMED, and that is the thing to know
about it. It was written for the shape above and no other, so on results from a database whose
titles are shaped differently it deletes almost nothing, and whatever it fails to delete is scored
as words. Measured over 1215 gene families, preparing RefSeq, GenPept and PDB hits with it rather
than with their own lists costs 0.156 precision and 0.104 F1. Recall does not move: nothing is lost,
junk is added, and precision pays for it. So name a list for every table that is not UniProtKB's --
the sections below say which. 

prot-scriber warns when the list a table was given deletes far less from its titles than one of the
shipped lists would. That warning is a comparison of lists on your own titles and not a claim about
which database they came from, and it is silent when you gave a list of your own or 'none'. It
cannot catch everything: a database whose titles carry no structure beyond the leading accession
looks the same to every list. 

2.2.2 NCBI reference databases 
------------------------------ 
The National Center for Biotechnology Information (NCBI) has excellent reference databases to be
searched by Blast or Diamond, too. Note that NCBI and UniProt update each other's databases very
frequently. So, by searching UniProt only you should not loose information. Anyway, NCBI has e.g.
the popular non redundant ('NR') database. However, NCBI has a different description ('stitle' in
Blast terminology) format. To make sure prot-scriber parses sequence similarity search result (Blast
or Diamond) tables (SSSTs) correctly, you should use a tailored --db-filter argument. Such a list of
regular expressions, specifically tailored for parsing SSSTs produced by searching NCBI reference
databases, e.g. NR, ships inside prot-scriber. Write it out, and edit it if neccessary, with
'prot-scriber defaults filter-regexs-ncbi-nr > my_filters.txt'. 
NCBI's RefSeq has a format of its own again, different from NR's: its titles carry a 'MULTISPECIES:'
prefix, an 'isoform X1' suffix, 'LOC' gene identifiers and a 'LOW QUALITY PROTEIN:' marker, none of
which the NR list knows about. Use 'prot-scriber defaults filter-regexs-refseq' for it, or give it
to the table that needs it as '--db-filter refseq=@filter-regexs-refseq'. 
 
2.2.3 UniRef reference databases 
------------------------------ 
The UniRef databases (UniProt Reference Clusters) provide clustered sets of sequences from the
UniProt Knowledgebase and selected UniParc records to obtain complete coverage of sequence space at
several resolutions (100%, 90% and 50% identity) while hiding redundant sequences. The UniRef100
database combines identical sequences and subfragments from any source organism into a single UniRef
entry (i.e. cluster). UniRef90 and UniRef50 are built by clustering UniRef100 sequences at the 90%
or 50% sequence identity levels. To make sure prot-scriber parses sequence similarity search result
(Blast or Diamond) tables (SSSTs) correctly, you should use a tailored --db-filter argument. Such a
list of regular expressions, specifically tailored for parsing SSSTs produced by searching the
UniRef databases ships inside prot-scriber. Write it out, and edit it if neccessary, with
'prot-scriber defaults filter-regexs-uniref > my_filters.txt'. 
 
2.2.4 The Protein Data Bank (PDB) 
------------------------------ 
PDB titles are '<entry-id> mol:protein length:NNN <description>', so both the molecule type and the
sequence length sit in front of the description and would otherwise be scored as words. A tailored
list ships for it too: 'prot-scriber defaults filter-regexs-pdb', or '--db-filter
pdb=@filter-regexs-pdb'. 
 
2.2.5 A note on giving these lists by name 
------------------------------ 
Every list carries comments explaining what its expressions are for, and several of them are not
readable without one -- '\w{2,}\d{1,2}[gGmMcC]\d+(\.\d+)*' is a locus code of the Arabidopsis kind,
At2g26220 -- so read a list before editing it. 
Seven of the nine lists are plain: one expression per line and nothing else -- no replacements. Only
the two capture-replace lists hold PAIRS of lines, an expression and the replacement below it. An
expression never spans lines in either kind. 
A line whose first non-blank character is '#' is a comment, and a blank line is nothing -- with one
exception, in the paired lists: the line directly after an expression is its replacement whatever it
contains, including a blank one, which means 'delete what matched'. So leave no blank line between
an expression and its replacement; between pairs they are free. 
An expression can still match a literal '#'; it just may not open with a bare one. Write it '[#]',
which no regex dialect can read as anything else ('\#' and '(#)' also work). 
Every one of these lists can be given to the table it belongs to as '@NAME' -- '--db-filter
nr=@filter-regexs-ncbi-nr' -- and that is worth preferring to a copy on disk. A copy is a thing that
goes stale: when prot-scriber improves a list, a pipeline holding its own copy keeps whatever it
copied, and nothing says so. Write a list out only when you mean to edit it. Run 'prot-scriber
defaults' with no name to see what there is. 
 
2.2.6 Checking a filter list against a database, before you trust it
------------------------------
If you are searching a database prot-scriber ships no list for, or you are unsure of the list you
have, put the database through the rules and read what they made of it:

prot-scriber explain --fasta <reference_database.fasta> --filter <the list you mean to use> -o
mydb.txt

One pass, nothing written but the report. It answers six questions at once, and none of them needs
you to know in advance what to look for. WORDS IN NEARLY EVERY DESCRIPTION are a property of the
database's title FORMAT rather than of the database -- 'mol' and 'length' are in every PDB title
because the PDB writes '<id> mol:protein length:NNN <desc>' -- while 'protein', 'domain' and
'family' are common because proteins are, and sit far below that line. WHAT THE SPLIT TOOK APART
names the compound tokens prot-scriber cuts and the bare numbers it makes of them, which is the one
class of artefact that is in no title at all: 'KLMA_20055' becomes 'klma' and '20055', and a bare
number is worth a fixed tiny score that joins it to whatever phrase stands beside it. CHARACTERS THE
SPLIT DOES NOT SEPARATE ON are the other half of that: 'ox=1736528' is one word because '=' is
neither part of a word nor a separator. WORDS SHAPED LIKE AN IDENTIFIER separates a code that IS the
description, which a blacklist rule can reach, from one that is only part of it, which only a
capture-replace pair can. RULES THAT NEVER FIRED names every expression of every list that matched
nothing, with the line it stands on -- and says whether it was even checked, since the blacklist
stops at its first match. CONSISTENCY reads no data at all and reports what the lists say about each
other.
Give '--table <search_result.tsv>' instead if you do not have the FASTA. It works, and the report
says what it costs: a search result holds only the sequences something matched, so the format words
are unaffected and the 'seen once' counts -- the evidence the identifier section rests on -- are
inflated.
Then try a rule without writing it anywhere:

prot-scriber explain --fasta <reference_database.fasta> --try 'filter:(?i)\bmol:\S+\s*'

That says what the rule removed, what it removed that you did not intend, and what it CREATED -- a
rule makes words as readily as it removes them, and the two-letter form of prot-scriber's own
gene-name pair was found to be turning 'CD5' into 'cd' and 'SH3' into 'sh' that way. It also puts
the candidate through the titles prot-scriber must not damage -- enzyme cofactors in brackets, gene
names whose number is their identity, the function words that carry the readability -- and says
which of them it touches, before the rule reaches a list.
To read a whole edit rather than one rule, give the list as it was:

prot-scriber explain --fasta <reference_database.fasta> --baseline 'filter=<the old list>'

Both run over the same titles in the same pass, so the difference between them is the edit.

2.3 Example Blast or Diamond commands 
------------------------------------- 
Note that the following instructions on how to execute your sequence similarity searches with Blast
or Diamond only include the information - in terms of selected output table columns - absolutely
required by 'prot-scriber'. You are welcome, of course, to have more columns in your tabular output,
e.g. 'bitscore' or 'evalue' etc., but then you must name them: prot-scriber reads the columns by
position, so give the whole header, in order, with --db-header -- e.g. --db-header "nr=qacc sacc
evalue stitle" for a table written with '-f 6 qseqid sseqid evalue stitle'. A column prot-scriber
does not itself read still has to be named, because a name is what puts the description in the right
place; an unnamed column in front of it shifts everything after it. A table whose column count
disagrees with its header is refused rather than read as something it is not. Note that you need to
search each of your reference databases with a separate Blast or Diamond command, respectively. 
Note also that prot-scriber requires all rows belonging to one query to stand together in the table.
Blast and Diamond write their output that way, so the commands below need nothing added; but if you
concatenate tables, or sort one by anything other than the query column, you have to restore it with
e.g. 'sort -s -t"<TAB>" -k1,1 <your-table>' -- a stable sort on the query column alone, which leaves
the order of each query's hits as it was. Alternatively give --unsorted-input, which reads such a
table as it is by holding every query until all input has been read; that needs memory in proportion
to the whole input rather than to a single query. prot-scriber stops with an error if a query it has
finished reappears, rather than annotate it twice from half its hits. 

2.3.1 Blast 
----------- 
Generate prot-scriber input with Blast as follows. The following example uses 'blastp', replace it,
if your query sequence type makes that necessary with 'blastn' or 'blastx'. 
 
blastp -db <reference_database.fasta> -query <your_query_sequences.fasta> -num_threads
<how-many-do-you-want-to-use> -out <queries_vs_reference_db_name_blastout.txt> -outfmt "6
delim=<TAB> qacc sacc stitle" 
 
It is important to note, that in the above 'outfmt' argument the 'delim' set to '<TAB>' means you
need to actually type in a TAB character. (We write '<TAB>' here, so you see something, not only
whitespace.) Typically you can type it by hitting Ctrl+Tab in the terminal. 
 
2.3.2 Diamond 
------------- 
Generate prot-scriber input with Diamond as follows. The following example uses 'blastp', replace
it, if your query sequence type makes that necessary with 'blastn' or 'blastx'. 
 
diamond blastp -p <how-many-threads-do-you-want-to-use> --quiet -d <reference-database.dmnd> -q
<your_query_sequences.fasta> -o <queries_vs_reference_db_name_diamondout.txt> -f 6 qseqid sseqid
stitle 
 
Note that diamond by default uses the '<TAB>' character as a field-separator for its output tables. 
 
2.4 Gene Family preparation and analysis 
---------------------------------------- 
Assume you have the proteomes of eight crucifer plant species and want to cluster the respective
amino acid sequences into gene families. Note that the following example provides code to be
executed in a BASH Shell (also available on Windows). We provide a very basic procedure to perform
the clustering: 
 
(i) "All versus all" Blast or Diamond 
 
Assume all amino acid sequences of the eight example proteomes stored in a single file
'all_proteins.fasta' 
Run: 
 
diamond makedb --in all_proteins.fasta -d all_proteins.fasta 
 
diamond blastp --quiet -p <how-many-threads-do-you-want-to-use?> -d all_proteins.fasta.dmnd -q
all_proteins.fasta -o all_proteins_vs_all.txt -f 6 qseqid sseqid pident 
 
(ii) Run markov clustering 
 
Note that 'mcl' is a command line tool implementing the original Markov Clustering algorithm [Stijn
van Dongen, A cluster algorithm for graphs. Technical Report INS-R0010, National Research Institute
for Mathematics and Computer Science in the Netherlands, Amsterdam, May 2000]. On most systems you
can install the 'mcl' binary using the respective package manager, e.g. 'sudo apt-get update && sudo
apt-get install -y mcl' (Debian / Ubuntu). 
 
mcl all_proteins_vs_all.txt -o all_proteins_gene_clusters.txt --abc -I 2.0 
 
(iii) Add gene family names to mcl output and filter out singleton clusters 
 
Note that we use the GNU tools 'sed' and 'awk' to do some basic post-processing of the 'mcl' output.

 
sed -e 's/\t/,/g' all_proteins_gene_clusters.txt | awk -F "," 'BEGIN{i=1}{if (NF > 1){print
"Seq-Fam_" i "\t" $0; i=i+1}}' > all_proteins_gene_families.txt 
 
Congratulations! You now have clustered your eight plant crucifer proteomes into gene families (file
'all_proteins_gene_families.txt'). 
 
(iv) Run prot-scriber 
 
We assume that you ran either 'blastp' or 'diamond blastp' (see section 2.3 for details) to search
your selected reference databases with the 'all_proteins.fasta' queries. Here, we assume you have
searched UniProt's Swissprot and trEMBL databases. 
 
prot-scriber -f all_proteins_gene_families.txt -s all_proteins_vs_Swissprot_blastout.txt -s
all_proteins_vs_trEMBL_blastout.txt -o all_proteins_gene_families_HRDs.txt
 
3. Finding out why prot-scriber said what it said 
================================================= 
Every description prot-scriber assigns is chosen from the descriptions of the hits a sequence
similarity search found, and everything that choice was made of can be shown. Nothing has to be
re-derived from the input, and no second implementation of the procedure is needed to inspect its
results. 
 
3.1 Why did this query get this description? 
------------------------------------------- 
Name the queries or families to account for with --explain. What is written is the whole of the
choice: every hit description that was scored and the words it was split into, what each informative
word was worth and how often it appeared, every phrase that was proposed and its score, and which of
them won. 
 
prot-scriber -s at_vs_sprot.tsv -o at_hrds.tsv --explain AT1G01010.1 
 
It goes to standard output; --explain-out writes it to a file instead. An identifier that was never
annotated is an error, not an empty answer. 
 
To have the same account of every annotee, ask for it as the output format: 
 
prot-scriber -s at_vs_sprot.tsv -o at_hrds.jsonl --format jsonl 
 
That writes one JSON object per annotee, as the run produces them, so the memory prot-scriber needs
stays independent of how large the input is. Its rows are therefore in the order the annotations
happened rather than sorted by identifier; each row stands on its own, so 'sort at_hrds.jsonl' gives
a byte-stable file. 
 
3.2 What does prot-scriber make of a sequence title? 
--------------------------------------------------- 
Before anything is scored, each hit's title ('stitle' in Blast terminology) is put through a
blacklist, a list of filter expressions, and a list of capture-replace pairs. To see what those do
to a particular title, and which expression did it: 
 
prot-scriber explain --stitle 'sp|P12345|ADH1_ARATH Alcohol dehydrogenase 1 OS=Arabidopsis thaliana
OX=3702 GN=ADH1 PE=1 SV=2' 
 
The rule lists are the annotation options' own, so a list you are considering can be tried out
before a run is submitted, on the titles the database actually returns: 
 
cut -f 3 at_vs_nr.tsv | prot-scriber explain --stitle - --filter @filter-regexs-ncbi-nr 
 
3.3 How well founded is a description? 
-------------------------------------- 
'--format tsv-scored' writes the ordinary output table with three columns added to it: what the
chosen phrase scored, how many hit descriptions it was chosen from, and how many distinct phrases
were proposed. It is the ordinary table otherwise -- the same rows, the same descriptions, sorted
the same way -- so a result can be sorted or thresholded by how well founded it is without anything
having to look at the input again.
```

</details>

Note, that you can get the manual directly from `prot-scriber`. In the command prompt (`cmd` or PowerShell on Windows) or the Terminal (Mac OS, Linux, or Unix) use  
```sh
prot-scriber --help
```
to get it printed.

If you are not interested in the command line options at this point, just read [MANUAL.txt](./MANUAL.txt) directly.

_Happy `prot-scribing`!_
    
## Speed and memory requirements
    
`prot-scriber` is **blazingly fast** and has **low memory requirements**. Consider the following two standard use cases, in which `prot-scriber` generated Human readable descriptions (HRDs) for (i) a single species and (ii) gene families.

### single species 

On a standard Laptop with 4 cores `prot-scriber` took approx. **7 seconds** and used a little under **50 MB RAM** to generate human readable descriptions for a complete plant proteome with Blast search Hits for 32,567 distinct query proteins (input: Blast result table from searches in UniProt Swissprot 66 MB, Blast result table from searches in UniProt trEMBL 144 MB)

### gene families 

On a standard Laptop with 4 cores `prot-scriber` took approx. **15 seconds** and used a little under **180 MB RAM** to generate human readable descriptions for 24,072 gene families with Blast search Hits for 71,610 distinct query proteins (input: Blast result table from searches in UniProt Swissprot 126 MB, Blast result table from searches in UniProt trEMBL 273 MB)
    
## Word-Cloud visualization of prot-scriber results
    
prot-scriber comes with a simple and small _R_ script to generate a word-cloud plot from any prot-scriber results. To use it, you must have R and the following packages installed (click [here](https://cran.r-project.org/doc/manuals/r-patched/R-admin.html#Installing-packages) to learn how to install R packages):
* `RColorBrewer`                                                          
* `wordcloud`                                                             
* `wordcloud2`                                                            
* `htmlwidgets`                                                           
* `webshot`

You find the script `prot-scriber-word-cloud.R` in the `misc` directory or download it directly from [here](https://raw.githubusercontent.com/usadellab/prot-scriber/master/misc/prot-scriber-word-cloud.R).
    
In your Terminal (`cmd` or Power-Shell on Windows) you can invoke the script as follows:
    
```sh
Rscript prot-scriber-word-cloud.R input-prot-scriber-table.txt output-files-name
```

Note that the first argument is the output-table generated by prot-scriber and the second is a file-name, _without_ file extension (e.g. `.pdf`). Several output files will be created, two PDFs and one HTML.
    
_Happy word-clouding!_
    
## Development / Contribute

`prot-scriber` is open source. Please feel free and invited to contribute. 

### Preparation of releases (pre-compiled executables)

This repository is set up to use [GitHub Actions](https://github.com/features/actions) (see `.github/workflows/push.yml` for details). We use GitHub actions to trigger compilation of `prot-scriber` every time a Git Tag is pushed to this repository. So, if you, after writing new code and committing it, do the following in your local repo:

```sh
git tag -a 'version-Foo-Bar-Baz' -m "My fancy new version called Foo Bar Baz"
git push origin master --tags
```

GitHub will automatically compile `prot-scriber` and provide executable binary versions of prot-scriber for the platforms and operating systems mentioned above. The resulting binaries are then made available for download on the [releases page](https://github.com/usadellab/prot-scriber/releases).

In short, you do not need to worry about compiling your latest version to make it available for download and the different platforms and operating systems, GitHub Actions take care of this for you. 
