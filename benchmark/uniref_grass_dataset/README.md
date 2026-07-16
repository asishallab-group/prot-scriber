# UniRef50 GRASS Benchmark Dataset

Implements GitHub issue #2 ("Create a test dataset from UniRef references"): sample UniRef50
cluster representatives, BLAST them against NCBI RefSeq, and compute a reciprocal-alignment
similarity score ("GRASS") plus a Jaccard similarity between each hit's description and the
query's known UniProtKB/UniRef description. The resulting TSV is meant to later help evaluate
`prot-scriber`'s HRD-generation quality against a ground-truth reference, and (in a future
step, not this pipeline) to let a meaningful GRASS threshold be introduced to exclude
low-quality Blast hits.

This is a standalone SLURM + Python pipeline, unrelated to the Rust CLI in the rest of this
repo. Nothing here is committed to `data/` (gitignored) -- see "Getting results off the
cluster" below.

## Pipeline stages

Each stage depends on the previous one's output files, so the easiest way to run the whole
pipeline is the master script, which submits all 6 stages via `sbatch` and chains them with
`--dependency=afterok` so each only starts once the previous one (including every task of
the Stage 5 array job) has actually finished successfully:

```sh
cd benchmark/uniref_grass_dataset
bash slurm/submit_all.sh
# or, to override the defaults (N=10000, SEED=42, NUM_SHARDS=200):
bash slurm/submit_all.sh --n 50000 --seed 7 --num-shards 400
```
(Invoked via `bash` rather than `./slurm/submit_all.sh` since this repo has
`core.fileMode=false` -- the executable bit doesn't survive a commit/checkout here, so
`chmod +x` on your own clone would be needed for the `./...` form to work.)
It prints each stage's job id as it submits it; track progress with `squeue -u $USER` /
`sacct`. If a stage fails once running, SLURM leaves the (already-queued) downstream stages
waiting forever rather than starting them -- `scancel` those job ids, fix the problem, and
re-run (either `submit_all.sh` again, or the individual `sbatch` commands below from
wherever the pipeline stopped).

If you'd rather run stages one at a time yourself (e.g. to inspect each stage's output
before continuing) instead of using `submit_all.sh`, submit each manually from inside this
directory, in order:

```sh
sbatch slurm/01_download_data.slurm
sbatch slurm/02_sample_queries.slurm                       # override: --export=ALL,N=50000,SEED=1
sbatch slurm/03_diamond_forward_search.slurm
sbatch slurm/04_filter_and_shard_forward_hits.slurm         # override: --export=ALL,NUM_SHARDS=400
sbatch --array=0-199 slurm/05_backward_search.slurm         # array size MUST match NUM_SHARDS above
sbatch slurm/06_compute_grass_and_jaccard.slurm
```
You can also chain these yourself with `--dependency=afterok:<jobid>` -- see
`slurm/submit_all.sh` for exactly how.

1. **`01_download_data.slurm`** -- downloads `uniref50.fasta` and NCBI's pre-formatted
   `refseq_protein` BLAST database, dumps its full FASTA (`blastdbcmd -entry all`), and
   builds a DIAMOND index from that same dump. Needs outbound internet access from the
   compute node (confirmed available on this cluster). One fetched copy of refseq_protein
   serves the FASTA dump *and* the DIAMOND build, so hit ids/descriptions/sequences are
   guaranteed consistent downstream.
2. **`02_sample_queries.slurm`** -- randomly samples `N` (default 10,000; override via
   `--export=ALL,N=<n>`) UniRef50 cluster representatives, seeded (default 42, override via
   `SEED=`) for reproducibility. Clusters whose representative has no UniProtKB accession
   (UniParc-only, `UniRef50_UPI...`) are excluded, since the baseline here is UniProtKB
   (Swissprot union trEMBL). Outputs: `sampled_queries.fasta`,
   `sampled_queries_metadata.tsv` (cluster id, accession, reference description, member
   count, taxonomy, sequence length), `sampling_provenance.json`.
3. **`03_diamond_forward_search.slurm`** -- the large search: sampled queries vs. all of
   `refseq_protein`, via `diamond blastp` (far faster here than classic `blastp` at this
   scale). Requests `stitle` and `full_sseq` in the output so every hit's description *and*
   full sequence come directly from this one search -- no separate lookup step needed later.
4. **`04_filter_and_shard_forward_hits.slurm`** -- drops hits that are fully identical (>=99.999%
   identity **and** full-length alignment coverage of both query and hit -- the issue's "100
   percent identity and full sequence coverage" definition, with a small epsilon instead of
   bare `==100` to guard against formatting/rounding artifacts). Shards the remainder by a
   deterministic hash of the query id for Stage 5's array job.
5. **`05_backward_search.slurm`** (SLURM array, one task per shard) -- the reciprocal
   ("backward") direction: for each original query, all its surviving hits are aligned back
   to it in **one** `blastp -query hits.fasta -subject query.fasta` call (not one call per
   pair). Column roles are swapped in this call's raw output (the hits are `-query`, the
   original query is `-subject`) and renamed immediately in `python/run_backward_search.py`
   so nothing downstream has to think about the swap again. If blastp finds no reciprocal
   alignment for a hit at all, it's simply absent from this stage's output -- Stage 6 is
   what turns that absence into an explicit flag.
6. **`06_compute_grass_and_jaccard.slurm`** -- joins forward + backward results per
   `(query, hit)` pair and writes the final dataset,
   `data/results/uniref_grass_benchmark_dataset.tsv`.

## Documented assumptions / interpretive decisions

The issue doesn't spell out every detail; these choices were made explicitly and can be
revisited by editing the relevant script:

- **UniParc-only cluster representatives are excluded from sampling** (see stage 2 above) --
  the issue's baseline is UniProtKB, not UniParc.
- **"Fully identical" hit definition**: `pident >= 99.999` (not bare `== 100`) **and** the
  alignment spans the full length of both the query and the hit
  (`filter_and_shard_forward_hits.py::filter_fully_identical`).
- **GRASS scale normalization**: `pident` (0-100 from BLAST/DIAMOND) is divided by 100 before
  combining with overlap (naturally 0-1) in the geometric mean -- the issue doesn't specify a
  scale, so this was chosen for consistency (`compute_grass_and_jaccard.py::grass_score`).
- **Missing reciprocal alignment**: if `blastp -subject` finds no alignment for a hit in the
  backward direction, the pair is **kept** in the final dataset with `grass_score=0` and
  `backward_alignment_found=False`, rather than dropped -- reciprocal-search failures stay
  visible in the dataset instead of silently vanishing.
- **Jaccard tokenization**: RefSeq hit descriptions conventionally end in a bracketed
  organism, e.g. `... [Homo sapiens]`; UniProt/UniRef descriptions don't carry this. It's
  stripped from the hit side only before tokenizing, so Jaccard scores reflect real
  descriptive similarity rather than a formatting difference
  (`compute_grass_and_jaccard.py::strip_organism_suffix`).
- **Database scope**: `refseq_protein` only, not also `nr` -- keeps compute/storage
  manageable; `nr` can be added as a second configured database later if refseq_protein
  coverage turns out insufficient relative to UniProtKB.
- **DIAMOND search parameters** (`--max-target-seqs 50`, `--evalue 1e-5`, `--sensitive`) and
  `blastp -subject`'s default e-value threshold are starting defaults, not tuned against real
  data yet.

## Environment setup

This cluster has no environment-modules system; a conda environment
(`environment.yml`) pins the exact tool versions available here (`blast=2.12.0`,
`diamond=2.1.9`) plus Python and the packages the pipeline scripts need:

```sh
conda env create -f environment.yml
```

Every `.slurm` script activates it the same way (robust in a non-interactive batch shell,
where conda's shell hook usually isn't auto-initialized):
```sh
source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark
```

## Getting code onto the cluster and results back off it

No `git` is used on the cluster -- these are plain files, so no git credentials are needed
there at all. Push the pipeline up (only the small script/config files exist locally; `data/`
doesn't exist yet and is gitignored):
```sh
rsync -av benchmark/uniref_grass_dataset/ <user>@<bioserver>:~/uniref_grass_dataset/
```
After running the stages, pull the results back (resumable -- only transfers deltas, useful
if a large TSV transfer gets interrupted, or if you re-run and just want the new/changed
files):
```sh
rsync -av <user>@<bioserver>:~/uniref_grass_dataset/data/results/ ./results/
```
Re-run the first `rsync` command any time you edit a script locally to re-sync it to the
cluster.

## Resource placeholders

Every `.slurm` file's `--time`/`--cpus-per-task`/`--mem`/`--partition`/`--account` values are
**rough starting guesses only**, marked `# PLACEHOLDER -- edit for this cluster`. Edit them to
match your actual cluster/partition/account before submitting, and revise after a first
real-scale dry run:

| Stage | time | cpus | mem | notes |
|---|---|---|---|---|
| 1 download+build | 12:00:00 | 8 | 32G | network/I/O bound |
| 2 sampling | 04:00:00 | 2 | 16G | streaming pass over full UniRef50 |
| 3 diamond forward | 12:00:00 | 32 | 128G | verify empirically against real refseq_protein size |
| 4 filter+shard | 02:00:00 | 2 | 16G | pandas over up to a few million rows |
| 5 backward (array, ~200 tasks) | 02:00:00/task | 2 | 4G/task | many small blastp -subject calls |
| 6 grass+jaccard | 01:00:00 | 4 | 16G | join + scoring over small tabular data |

Before a full run, dry-run the pipeline against a small sample first, e.g.
`sbatch --export=ALL,N=50 slurm/02_sample_queries.slurm`, to sanity-check the whole chain
cheaply.

## Local smoke tests (no cluster, BLAST, or conda required)

The pure-Python logic (UniRef50 header parsing, sampling, and the GRASS/Jaccard/overlap
math) has smoke tests under `python/tests/` that only need a plain Python 3 interpreter
(stdlib `unittest`, no `pytest`/conda needed):
```sh
cd benchmark/uniref_grass_dataset/python
python3 -m unittest discover -s tests -v
```
These do **not** exercise the actual `diamond`/`blastp`/SLURM invocations -- those are correct
by careful construction and cross-checked against DIAMOND/BLAST+'s documented `-outfmt`
behavior, but should be dry-run on the cluster (small `--n`) before a full-scale run.
