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
(`prot-scriber doc input` has the details.) For a quick test run you can assume to have carried out the searches and use the example output tables below (all files are taken from this repository's [`misc`](https://github.com/asishallab-group/prot-scriber/tree/master/misc) directory):

**To generate HRDs for twelve example biological sequences (proteins) use:**
* [`Twelve_Proteins_vs_Swissprot_blastp.txt`](https://raw.githubusercontent.com/asishallab-group/prot-scriber/master/misc/Twelve_Proteins_vs_Swissprot_blastp.txt)
* [`Twelve_Proteins_vs_trembl_blastp.txt`](https://raw.githubusercontent.com/asishallab-group/prot-scriber/master/misc/Twelve_Proteins_vs_trembl_blastp.txt)

**To generate HRDs for two gene-families, comprising four and three proteins, respectively, use:**
* [`families.txt`](https://raw.githubusercontent.com/asishallab-group/prot-scriber/master/misc/families.txt)
* [`family_prots_vs_Swissprot.txt`](https://raw.githubusercontent.com/asishallab-group/prot-scriber/master/misc/family_prots_vs_Swissprot.txt)
* [`family_prots_vs_trEMBL.txt`](https://raw.githubusercontent.com/asishallab-group/prot-scriber/master/misc/family_prots_vs_trEMBL.txt)

`prot-scriber doc families` has a recipe for clustering biological sequences into gene-families.

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

You can choose to download a pre-built binary, ready to be executed, from the table below, if you want the latest stable version. Other versions can be downloaded from the [Releases page](https://github.com/asishallab-group/prot-scriber/releases). Have a look at the below table to know which is the version you need for your operating system and platform. 

We strongly recommend to _rename_ the downloaded release file to a simple `prot-scriber` (or `prot-scriber.exe` on Windows).

Note that on Mac OS and Unix / Linux operating systems you need to make the downloaded and renamed `prot-scriber` file _executable_. In order to achieve this, open a terminal shell, navigate (`cd`) to the directory where you saved `prot-scriber`, and execute `chmod a+x ./prot-scriber`.


|Operating System|CPU-Architecture|Release-Name (click to download)|Comment|
|---|---|---|---|
|Windows 7 or higher|any|[windows_prot-scriber.exe](https://github.com/asishallab-group/prot-scriber/releases/latest/download/x86_64-pc-windows-gnu_prot-scriber.exe)|to be used in a terminal (`cmd` or Power-Shell)|
|any GNU-Linux|any Intel x86, 64 bits|[x86_64-unknown-linux-gnu_prot-scriber](https://github.com/asishallab-group/prot-scriber/releases/latest/download/x86_64-unknown-linux-gnu_prot-scriber)|requires libm.so.6 (compiled with glibc version 2.27) and libc.so.6 (compiled with glibc 2.18) installed as is the case e.g. in Ubuntu >= 22.04|
|any GNU-Linux|any aarch, 64 bits|[aarch64-unknown-linux-gnu_prot-scriber](https://github.com/asishallab-group/prot-scriber/releases/latest/download/aarch64-unknown-linux-gnu_prot-scriber)|e.g. for Raspberry Pi; requires libm.so.6 (compiled with glibc version 2.27) and libc.so.6 (compiled with glibc 2.18) installed as is the case e.g. in Ubuntu >= 22.04|
|Apple / Mac OS|any Mac Computer with Mac OS 10|[x86_64-apple-darwin_prot-scriber](https://github.com/asishallab-group/prot-scriber/releases/latest/download/x86_64-apple-darwin_prot-scriber)||

### Compilation from source code

#### Prerequisites 

`prot-scriber` is written in Rust. That makes it extremely performant and, once compiled for your operating system (OS), can be used on any machine with that particular OS environment. To compile it for your platform you first need to have Rust and cargo installed. Follow [the official instructions](https://www.rust-lang.org/tools/install).

#### Obtain the code

Download the source code of the [latest release](https://github.com/asishallab-group/prot-scriber/releases/latest) -- "Source code (zip)" at the bottom of the release page -- and unzip it, e.g. by double clicking it or by using the command line:
```sh
unzip prot-scriber-<version>.zip
```

Or clone the repository and check out the tag of the release you want:
```sh
git clone https://github.com/asishallab-group/prot-scriber.git
cd prot-scriber
git checkout v<version>
```

#### Compile `prot-scriber`

Change into the directory of the `prot-scriber` code, e.g. in a Mac OS or Linux terminal `cd prot-scriber-<version>` after having unpacked the release, or stay in `prot-scriber` if you cloned it (see above).

Now, compile `prot-scriber` with
```sh
cargo build --release
```

The above compilation command has generated an executable binary file in the current directory `./target/release/prot-scriber`. You can just go ahead and use `prot-scriber` now that you have compiled it successfully (see Usage below).

#### Global installation

If you are familiar with installing self compiled tools on a system wide level, this section will provide no news to you. It is convenient to make the compiled executable `prot-scriber` program available from anywhere on your system. To achieve this, you need to copy it to any place you typically have your programs installed, or add its directory to your, our all users' `$PATH` environment. In doing so, e.g. in case you are a system administrator, you make `prot-scriber` available for all users of your infrastructure. Make sure you and your group have executable access rights to the file. You can adjust these access right with `chmod ug+x ./target/release/prot-scriber`. You, and possibly other users of your system, are now ready to run `prot-scriber`.

### Install via bioconda
**Bioconda packages the upstream prot-scriber, not this fork.** It installs upstream's 0.1 releases, whose command line and rule lists differ from 0.2's. For 0.2 or later, download an executable or compile the source as described above.

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

### Annotating

Give each table of search results with `-s` (or `--db`), and name the output file with `-o`:

```sh
prot-scriber -s my_prots_vs_sprot.txt -s my_prots_vs_trembl.txt -o my_prots_HRDs.txt
```

Name a table, and it can be given the rule list of the database it was searched in -- here NCBI's non-redundant database, whose titles are shaped differently from UniProt's:

```sh
prot-scriber -s sprot=my_prots_vs_sprot.txt -s nr=my_prots_vs_nr.txt --db-filter nr=@filter-regexs-ncbi-nr -o my_prots_HRDs.txt
```

### Help and documentation

`prot-scriber` documents itself, and what it prints is always what the binary you run does:

* `prot-scriber --help` (or `-h`) lists the commands and options.
* `prot-scriber help <command>`, e.g. `prot-scriber help annotate`, gives every option of that command in full.
* `prot-scriber doc algorithm` is how a description is chosen, step by step: the rule lists, the word scores, the phrases, and gene families.
* `prot-scriber doc` lists the topics that go beyond single options -- how to prepare the input with Blast or Diamond, for instance -- and `prot-scriber doc <topic>` prints one.
* `prot-scriber defaults` lists the built-in rule lists, and `prot-scriber defaults <name>` prints one.

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

You find the script `prot-scriber-word-cloud.R` in the `misc` directory or download it directly from [here](https://raw.githubusercontent.com/asishallab-group/prot-scriber/master/misc/prot-scriber-word-cloud.R).
    
In your Terminal (`cmd` or Power-Shell on Windows) you can invoke the script as follows:
    
```sh
Rscript prot-scriber-word-cloud.R input-prot-scriber-table.txt output-files-name
```

Note that the first argument is the output-table generated by prot-scriber and the second is a file-name, _without_ file extension (e.g. `.pdf`). Several output files will be created, two PDFs and one HTML.
    
_Happy word-clouding!_
    
## Development / Contribute

`prot-scriber` is open source. Please feel free and invited to contribute. 

### Preparation of releases (pre-compiled executables)

**Currently switched off (since 25.09.2026, while the evaluation is still running):** the automatic trigger in `.github/workflows/release-on-version-change.yml` is commented out, so bumping the version and merging it releases nothing. A release is made by hand in the meantime, by starting the "release by hand" workflow (`.github/workflows/release-manual.yml`) on GitHub. What follows describes the release process as it is when the trigger is switched back on.

A release is made by changing the version, and by nothing else. The version lives in exactly one place, `version` in `Cargo.toml`, and the release's tag, `v<version>`, is derived from it, so the two cannot disagree.

1. Bump `version` in `Cargo.toml`, and run `cargo build` so that `Cargo.lock` follows.
2. Get the bump onto `master` on GitHub.

On every push to `master` that touches what a release is built from -- `src/` (which holds the `doc` topics too), `assets/`, `Cargo.toml`, `Cargo.lock` or `.cargo/` -- [GitHub Actions](https://github.com/features/actions) read the version and look for its tag (`.github/workflows/release-on-version-change.yml`). If the tag exists, nothing happens: a merge that did not bump the version releases nothing. If it does not, `.github/workflows/release.yml` runs the test suite and clippy, builds the executables for the platforms in the table above against the committed `Cargo.lock`, and only then creates the tag and publishes the release -- the executables -- on the [releases page](https://github.com/asishallab-group/prot-scriber/releases). A build that fails leaves no tag behind.

**Do not create a version tag by hand.** A tag `v<version>` tells the workflow that this version is released already, so a hand-made one makes it skip that version until the tag is deleted.

To re-run a release that failed part-way, start *release by hand* (`.github/workflows/release-manual.yml`) from the Actions tab. Unlike the automatic trigger it fails, rather than skipping, when the version is released already. On pull requests, `.github/workflows/version-guard.yml` warns when a change to those paths leaves the version where it was, since merging it would then release nothing.

In short, you do not need to compile anything to make a version available for download on the different platforms and operating systems: bump the version, and GitHub Actions take care of the rest. 
