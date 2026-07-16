#!/usr/bin/env python3
"""Stage 4: drop fully-identical forward hits and shard the remainder by query for Stage 5.

Reads the raw (headerless) forward_search.tsv produced by Stage 3 (diamond blastp,
FORWARD_COLUMNS column order) and:
  1. Drops hits that are fully identical (100% identity, full-length alignment on both the
     query and the hit) -- these carry no useful reciprocal/description-similarity signal
     (per the issue: "ignore fully identical Blast Hits (100 percent identity and full
     sequence coverage by the pairwise Blast alignment)").
  2. Writes the remaining (filtered) hits once, with a header row, for Stage 6 to reuse.
  3. Shards the filtered hits by a deterministic hash of the query id into self-contained
     per-shard files (a hits TSV plus a matching query-sequence FASTA), so each Stage 5
     SLURM array task only ever needs to read its own shard.
"""
import argparse
import hashlib
import sys
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common.fasta_utils import read_fasta_to_dict, write_fasta  # noqa: E402
from common.schema import FORWARD_COLUMNS  # noqa: E402

# >=99.999 rather than bare ==100 guards against any formatting/rounding artifact in the
# aligner's own percent-identity serialization, while still meaning "fully identical".
FULL_IDENTITY_PIDENT_THRESHOLD = 99.999


def load_forward_hits(forward_tsv):
    return pd.read_csv(forward_tsv, sep="\t", header=None, names=FORWARD_COLUMNS)


def filter_fully_identical(df):
    """Returns `(kept, n_dropped)`; drops hits with >=99.999% identity AND full-length
    alignment coverage of both the query and the hit sequence."""
    full_query_coverage = (df["qend"] - df["qstart"] + 1) == df["qlen"]
    full_hit_coverage = (df["send"] - df["sstart"] + 1) == df["slen"]
    is_fully_identical = (
        (df["pident"] >= FULL_IDENTITY_PIDENT_THRESHOLD) & full_query_coverage & full_hit_coverage
    )
    kept = df[~is_fully_identical].copy()
    return kept, int(is_fully_identical.sum())


def shard_index(key: str, num_shards: int) -> int:
    """A deterministic (unlike Python's hash-randomized built-in `hash()`), stable-across-
    runs shard assignment, so re-running this script always partitions the same way."""
    digest = hashlib.md5(key.encode("utf-8")).hexdigest()
    return int(digest, 16) % num_shards


def write_shards(kept, query_seqs, shard_dir, num_shards):
    shard_dir = Path(shard_dir)
    shard_dir.mkdir(parents=True, exist_ok=True)

    kept = kept.copy()
    kept["_shard"] = kept["qseqid"].apply(lambda q: shard_index(q, num_shards))

    for shard_id, shard_df in kept.groupby("_shard"):
        shard_df = shard_df.drop(columns="_shard")
        shard_df.to_csv(
            shard_dir / f"filtered_forward_shard_{shard_id:03d}.tsv", sep="\t", index=False
        )
        shard_query_ids = set(shard_df["qseqid"])
        write_fasta(
            ((qid, query_seqs[qid]) for qid in shard_query_ids),
            shard_dir / f"query_seqs_shard_{shard_id:03d}.fasta",
        )

    return kept["_shard"].nunique() if len(kept) else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--forward-tsv", required=True, help="Stage 3 diamond blastp output (headerless)"
    )
    parser.add_argument(
        "--sampled-queries-fasta", required=True, help="Stage 2 sampled_queries.fasta"
    )
    parser.add_argument(
        "--out-filtered-tsv",
        required=True,
        help="Where to write the filtered (unsharded) hits, with header",
    )
    parser.add_argument(
        "--shard-dir",
        required=True,
        help="Directory to write per-shard hit TSVs + query FASTAs into",
    )
    parser.add_argument(
        "--num-shards",
        type=int,
        default=200,
        help="Must match the Stage 5 SLURM array size (default: 200)",
    )
    args = parser.parse_args()

    df = load_forward_hits(args.forward_tsv)
    n_total = len(df)
    kept, n_dropped = filter_fully_identical(df)
    Path(args.out_filtered_tsv).parent.mkdir(parents=True, exist_ok=True)
    kept.to_csv(args.out_filtered_tsv, sep="\t", index=False)

    query_seqs = read_fasta_to_dict(args.sampled_queries_fasta)
    n_queries_with_hits = kept["qseqid"].nunique() if len(kept) else 0

    n_shards_used = write_shards(kept, query_seqs, args.shard_dir, args.num_shards)

    print(
        f"Forward hits: {n_total} total, {n_dropped} dropped as fully identical, "
        f"{len(kept)} remaining across {n_shards_used} shards. "
        f"{n_queries_with_hits}/{len(query_seqs)} sampled queries retain at least one hit."
    )


if __name__ == "__main__":
    main()
