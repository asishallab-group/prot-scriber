#!/usr/bin/env python3
"""Stage 5: reciprocal ("backward") pairwise realignment of each surviving hit against its
original query, batched one `blastp -subject` call per original query.

For a SLURM array task with index `--shard-index`, reads that shard's filtered forward hits
(`filtered_forward_shard_<NNN>.tsv`, which already carries each hit's full sequence via the
`full_sseq` column captured in Stage 3 -- no separate sequence extraction needed) and that
shard's query sequences (`query_seqs_shard_<NNN>.fasta`). For each original query, all of its
surviving hits are written to one small multi-FASTA and aligned in a single
`blastp -query hits.fasta -subject query.fasta` call against the query's own sequence.

IMPORTANT column-role swap: because the hits are `-query` and the original query is
`-subject` in this call, blastp's own "qseqid"/"qstart"/"qend"/"qlen" columns describe the
HIT, and "sseqid"/"sstart"/"send"/"slen" describe the original QUERY. This is renamed
immediately after parsing (see RAW_BACKWARD_COLUMNS -> BACKWARD_COLUMNS in common/schema.py)
so no code downstream of this script ever has to think about the swap again.

If blastp finds no reciprocal alignment for a hit at all (too divergent in that direction),
it simply doesn't appear in this script's output -- Stage 6's left-join is what turns that
absence into an explicit `backward_alignment_found=False` row, per the pipeline's policy of
keeping such pairs visible rather than silently dropping them.
"""
import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common.fasta_utils import read_fasta_to_dict, write_fasta  # noqa: E402
from common.schema import BACKWARD_COLUMNS, RAW_BACKWARD_COLUMNS  # noqa: E402


def run_one_query_backward_search(blastp_bin, hits, query_id, query_seq, scratch_dir):
    """`hits` is an iterable of `(hit_id, hit_sequence)` tuples, all surviving hits of
    `query_id`. Returns a list of dict rows already renamed to BACKWARD_COLUMNS."""
    hits_fasta = scratch_dir / f"{query_id}.hits.fasta"
    query_fasta = scratch_dir / f"{query_id}.query.fasta"
    try:
        write_fasta(hits, hits_fasta)
        write_fasta([(query_id, query_seq)], query_fasta)

        result = subprocess.run(
            [
                blastp_bin,
                "-query",
                str(hits_fasta),
                "-subject",
                str(query_fasta),
                "-outfmt",
                "6 " + " ".join(RAW_BACKWARD_COLUMNS),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
    finally:
        hits_fasta.unlink(missing_ok=True)
        query_fasta.unlink(missing_ok=True)

    rows = []
    for line in result.stdout.splitlines():
        if not line.strip():
            continue
        raw = dict(zip(RAW_BACKWARD_COLUMNS, line.split("\t")))
        rows.append(
            {
                "query_id": raw["sseqid"],
                "hit_id": raw["qseqid"],
                "pident": float(raw["pident"]),
                "length": int(raw["length"]),
                "hit_start": int(raw["qstart"]),
                "hit_end": int(raw["qend"]),
                "hit_len": int(raw["qlen"]),
                "query_start": int(raw["sstart"]),
                "query_end": int(raw["send"]),
                "query_len": int(raw["slen"]),
                "evalue": float(raw["evalue"]),
                "bitscore": float(raw["bitscore"]),
            }
        )
    return rows


def run_shard(shard_dir, shard_index, blastp_bin, scratch_dir):
    shard_dir = Path(shard_dir)
    forward_tsv = shard_dir / f"filtered_forward_shard_{shard_index:03d}.tsv"
    query_fasta = shard_dir / f"query_seqs_shard_{shard_index:03d}.fasta"

    forward = pd.read_csv(forward_tsv, sep="\t")
    query_seqs = read_fasta_to_dict(query_fasta)

    all_rows = []
    for query_id, group in forward.groupby("qseqid"):
        hits = list(zip(group["sseqid"], group["full_sseq"]))
        rows = run_one_query_backward_search(
            blastp_bin, hits, query_id, query_seqs[query_id], scratch_dir
        )
        all_rows.extend(rows)

    return pd.DataFrame(all_rows, columns=BACKWARD_COLUMNS)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--shard-dir", required=True, help="Directory of Stage 4's per-shard outputs"
    )
    parser.add_argument(
        "--shard-index",
        type=int,
        required=True,
        help="Shard to process (e.g. $SLURM_ARRAY_TASK_ID)",
    )
    parser.add_argument(
        "--out-tsv", required=True, help="Where to write this shard's backward-search results"
    )
    parser.add_argument(
        "--blastp-bin",
        default="blastp",
        help="Path to the blastp executable (default: 'blastp' on PATH)",
    )
    parser.add_argument(
        "--scratch-dir",
        default=None,
        help="Node-local scratch directory for per-query temp FASTA files "
        "(default: $SLURM_TMPDIR if set, else the system temp dir)",
    )
    args = parser.parse_args()

    scratch_base = args.scratch_dir or os.environ.get("SLURM_TMPDIR") or tempfile.gettempdir()
    scratch_dir = Path(
        tempfile.mkdtemp(prefix=f"grass_backward_shard{args.shard_index:03d}_", dir=scratch_base)
    )

    try:
        result = run_shard(args.shard_dir, args.shard_index, args.blastp_bin, scratch_dir)
    finally:
        shutil.rmtree(scratch_dir, ignore_errors=True)

    Path(args.out_tsv).parent.mkdir(parents=True, exist_ok=True)
    result.to_csv(args.out_tsv, sep="\t", index=False)
    print(f"Shard {args.shard_index}: {len(result)} backward alignment rows written to {args.out_tsv}")


if __name__ == "__main__":
    main()
