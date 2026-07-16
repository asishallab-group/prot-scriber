#!/usr/bin/env python3
"""Stage 6: join forward + backward search results per (query, hit) pair and compute the
final GRASS score + Jaccard description-similarity dataset -- this TSV is the deliverable
the issue asks for.

Reads Stage 4's filtered (unsharded) forward hits and all of Stage 5's per-shard backward
results (concatenated), left-joins them on (query_id, hit_id) -- preserving pairs with no
reciprocal alignment at all, per this pipeline's policy of keeping such pairs visible
(grass_score=0, backward_alignment_found=False) rather than dropping them -- and computes,
per surviving pair:
  - forward/backward "overlap": ((hit_stop-hit_start+1) + (query_stop-query_start+1))
    / (query_length+hit_length), per the issue's definition.
  - GRASS = geometric mean of (forward_overlap, forward_pident/100, backward_overlap,
    backward_pident/100) -- pident is normalized from BLAST's 0-100 scale to 0-1 so all
    four terms combine on a consistent scale (a documented interpretive choice, since the
    issue doesn't specify the scale).
  - Jaccard similarity of the query's UniProtKB/UniRef reference description and the
    RefSeq hit's description, case-insensitive, after stripping RefSeq's conventional
    trailing "[Organism]" suffix (which UniProt/UniRef descriptions don't carry, so leaving
    it in would deflate scores for a formatting reason rather than a real difference).
"""
import argparse
import glob
import re
import sys
from pathlib import Path

import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common.schema import FINAL_DATASET_COLUMNS  # noqa: E402

ORGANISM_SUFFIX_RE = re.compile(r"\s*\[[^\]]+\]\s*$")
TOKEN_RE = re.compile(r"[a-z0-9]+")


def overlap(start_a, end_a, len_a, start_b, end_b, len_b):
    """The issue's overlap formula, generalized to either alignment direction:
    ((b_stop - b_start + 1) + (a_stop - a_start + 1)) / (a_length + b_length).
    Works elementwise on pandas Series as well as on plain numbers."""
    return ((end_a - start_a + 1) + (end_b - start_b + 1)) / (len_a + len_b)


def geometric_mean4(a, b, c, d):
    return (a * b * c * d) ** 0.25


def grass_score(forward_overlap, forward_pident, backward_overlap, backward_pident):
    """`forward_pident`/`backward_pident` must already be normalized to [0, 1]."""
    return geometric_mean4(forward_overlap, forward_pident, backward_overlap, backward_pident)


def strip_organism_suffix(description: str) -> str:
    """Removes a trailing RefSeq-style '... [Organism name]' suffix, if present."""
    return ORGANISM_SUFFIX_RE.sub("", description)


def tokenize(description: str) -> set:
    return set(TOKEN_RE.findall(description.lower()))


def jaccard_similarity(query_description: str, hit_description: str) -> float:
    """Jaccard similarity of the two descriptions' lowercase word sets. The hit description
    (conventionally RefSeq) has its trailing organism-bracket suffix stripped first; the
    query description (UniProt/UniRef) never carries that suffix, so is left as-is."""
    tokens_a = tokenize(query_description)
    tokens_b = tokenize(strip_organism_suffix(hit_description))
    union = tokens_a | tokens_b
    if not union:
        return float("nan")
    return len(tokens_a & tokens_b) / len(union)


def load_backward_results(shard_glob):
    shard_paths = sorted(glob.glob(shard_glob))
    if not shard_paths:
        raise FileNotFoundError(f"No backward-search shard result files matched: {shard_glob}")
    return pd.concat((pd.read_csv(p, sep="\t") for p in shard_paths), ignore_index=True)


def build_dataset(forward, backward, query_metadata):
    merged = forward.merge(
        backward, left_on=["qseqid", "sseqid"], right_on=["query_id", "hit_id"], how="left"
    )
    merged["backward_alignment_found"] = merged["query_id"].notna()
    has_backward = merged["backward_alignment_found"]

    merged["forward_overlap"] = overlap(
        merged["qstart"], merged["qend"], merged["qlen"],
        merged["sstart"], merged["send"], merged["slen"],
    )
    merged["forward_pident_norm"] = merged["pident_x"] / 100.0

    merged["backward_overlap"] = 0.0
    merged.loc[has_backward, "backward_overlap"] = overlap(
        merged.loc[has_backward, "query_start"], merged.loc[has_backward, "query_end"],
        merged.loc[has_backward, "query_len"],
        merged.loc[has_backward, "hit_start"], merged.loc[has_backward, "hit_end"],
        merged.loc[has_backward, "hit_len"],
    )
    merged["backward_pident_norm"] = 0.0
    merged.loc[has_backward, "backward_pident_norm"] = (
        merged.loc[has_backward, "pident_y"] / 100.0
    )

    # Sanity check (log, don't crash): lengths should agree between the two searches for the
    # same pair -- a mismatch would indicate a sequence-extraction bug.
    mismatched = has_backward & (
        (merged["query_len"] != merged["qlen"]) | (merged["hit_len"] != merged["slen"])
    )
    if mismatched.any():
        print(
            f"WARNING: {int(mismatched.sum())} (query,hit) pairs have mismatched "
            "forward/backward sequence lengths -- possible sequence-extraction bug.",
            file=sys.stderr,
        )

    merged["grass_score"] = 0.0
    merged.loc[has_backward, "grass_score"] = geometric_mean4(
        merged.loc[has_backward, "forward_overlap"],
        merged.loc[has_backward, "forward_pident_norm"],
        merged.loc[has_backward, "backward_overlap"],
        merged.loc[has_backward, "backward_pident_norm"],
    )

    merged = merged.merge(query_metadata, left_on="qseqid", right_on="cluster_id", how="left")

    merged["jaccard_similarity"] = merged.apply(
        lambda row: jaccard_similarity(row["description"], row["stitle"]), axis=1
    )

    out = pd.DataFrame(
        {
            "query_id": merged["qseqid"],
            "query_description": merged["description"],
            "query_length": merged["qlen"],
            "query_n_members": merged["n_members"],
            "hit_id": merged["sseqid"],
            "hit_description": merged["stitle"],
            "hit_length": merged["slen"],
            "forward_pident": merged["pident_x"],
            "forward_overlap": merged["forward_overlap"],
            "forward_evalue": merged["evalue_x"],
            "forward_bitscore": merged["bitscore_x"],
            "backward_pident": merged["pident_y"].fillna(0.0),
            "backward_overlap": merged["backward_overlap"],
            "backward_evalue": merged["evalue_y"],
            "backward_bitscore": merged["bitscore_y"],
            "backward_alignment_found": merged["backward_alignment_found"],
            "grass_score": merged["grass_score"],
            "jaccard_similarity": merged["jaccard_similarity"],
        }
    )
    return out[FINAL_DATASET_COLUMNS]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--filtered-forward-tsv", required=True)
    parser.add_argument(
        "--backward-shard-glob",
        required=True,
        help="Glob matching all backward_results_shard_*.tsv files",
    )
    parser.add_argument("--sampled-queries-metadata", required=True)
    parser.add_argument("--out-tsv", required=True)
    args = parser.parse_args()

    forward = pd.read_csv(args.filtered_forward_tsv, sep="\t")
    backward = load_backward_results(args.backward_shard_glob)
    query_metadata = pd.read_csv(args.sampled_queries_metadata, sep="\t")

    dataset = build_dataset(forward, backward, query_metadata)
    Path(args.out_tsv).parent.mkdir(parents=True, exist_ok=True)
    dataset.to_csv(args.out_tsv, sep="\t", index=False)

    n_missing_backward = int((~dataset["backward_alignment_found"]).sum())
    print(
        f"Wrote {len(dataset)} (query, hit) rows to {args.out_tsv} "
        f"({n_missing_backward} with no reciprocal alignment found)."
    )


if __name__ == "__main__":
    main()
