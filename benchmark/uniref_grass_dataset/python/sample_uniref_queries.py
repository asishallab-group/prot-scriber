#!/usr/bin/env python3
"""Stage 2: sample N UniRef50 cluster representatives for the GRASS benchmark dataset.

Writes, into --out-dir:
  - sampled_queries.fasta            one record per sampled cluster (header = cluster_id)
  - sampled_queries_metadata.tsv     SAMPLED_QUERY_METADATA_COLUMNS, one row per cluster
  - sampling_provenance.json         n, seed, source path, totals, timestamp

Clusters whose representative is UniParc-only (no UniProtKB accession, i.e. cluster id
starts with "UniRef50_UPI") are excluded, since this pipeline's baseline is UniProtKB
(Swissprot union trEMBL). Sampling is a single seeded shuffle of all cluster indices,
walked in order until `n` usable (non-UniParc-only, parsable) clusters are collected --
this naturally "draws more candidates" as needed without a separate retry loop.
"""
import argparse
import json
import random
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common.schema import SAMPLED_QUERY_METADATA_COLUMNS  # noqa: E402
from common.uniref_header import UnparsableHeaderError, parse_header  # noqa: E402


def index_header_offsets(fasta_path):
    """Streams the FASTA file once, returning a list of byte offsets, one per header line."""
    offsets = []
    with open(fasta_path, "rb") as fh:
        while True:
            offset = fh.tell()
            line = fh.readline()
            if not line:
                break
            if line.startswith(b">"):
                offsets.append(offset)
    return offsets


def read_record_at(fh, offset):
    """Reads one FASTA record (header + concatenated sequence lines) starting at byte
    `offset` of the already-open binary file handle `fh`."""
    fh.seek(offset)
    header_line = fh.readline().decode("utf-8", errors="replace")
    seq_chunks = []
    while True:
        line = fh.readline()
        if not line or line.startswith(b">"):
            break
        seq_chunks.append(line.decode("ascii").strip())
    return header_line, "".join(seq_chunks)


def sample_clusters(fasta_path, n, seed):
    """Returns `(selected, total_clusters, n_upi_skipped, n_unparsable_skipped)` where
    `selected` is a list of `(UniRefHeader, sequence)` tuples of length `n`."""
    offsets = index_header_offsets(fasta_path)
    total_clusters = len(offsets)
    if n > total_clusters:
        raise ValueError(
            f"Requested n={n} clusters but UniRef50 fasta only contains {total_clusters}."
        )

    shuffled_idx = list(range(total_clusters))
    random.Random(seed).shuffle(shuffled_idx)

    selected = []
    n_upi_skipped = 0
    n_unparsable_skipped = 0

    with open(fasta_path, "rb") as fh:
        for idx in shuffled_idx:
            if len(selected) == n:
                break
            header_line, sequence = read_record_at(fh, offsets[idx])
            try:
                header = parse_header(header_line)
            except UnparsableHeaderError:
                n_unparsable_skipped += 1
                continue
            if header.is_uniparc_only:
                n_upi_skipped += 1
                continue
            selected.append((header, sequence))

    if len(selected) < n:
        raise RuntimeError(
            f"Only found {len(selected)}/{n} usable (non-UniParc-only) clusters after "
            f"exhausting all {total_clusters} clusters in {fasta_path}."
        )

    return selected, total_clusters, n_upi_skipped, n_unparsable_skipped


def write_outputs(
    selected,
    out_fasta,
    out_metadata,
    out_provenance,
    *,
    n,
    seed,
    fasta_path,
    total_clusters,
    n_upi_skipped,
    n_unparsable_skipped,
):
    with open(out_fasta, "w") as fasta_out:
        for header, sequence in selected:
            fasta_out.write(f">{header.cluster_id}\n{sequence}\n")

    with open(out_metadata, "w") as meta_out:
        meta_out.write("\t".join(SAMPLED_QUERY_METADATA_COLUMNS) + "\n")
        for header, sequence in selected:
            row = [
                header.cluster_id,
                header.representative_accession,
                header.description,
                str(header.n_members),
                header.tax_name or "",
                str(header.taxid) if header.taxid is not None else "",
                str(len(sequence)),
            ]
            meta_out.write("\t".join(row) + "\n")

    provenance = {
        "n_requested": n,
        "n_sampled": len(selected),
        "seed": seed,
        "uniref50_source_path": str(fasta_path),
        "total_clusters_seen": total_clusters,
        "n_upi_skipped": n_upi_skipped,
        "n_unparsable_skipped": n_unparsable_skipped,
        "timestamp": datetime.now(timezone.utc).isoformat(),
    }
    with open(out_provenance, "w") as prov_out:
        json.dump(provenance, prov_out, indent=2)
        prov_out.write("\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--uniref50-fasta", required=True, help="Path to the (uncompressed) uniref50.fasta"
    )
    parser.add_argument("--out-dir", required=True, help="Directory to write outputs into")
    parser.add_argument(
        "--n", type=int, default=10_000, help="Number of clusters to sample (default: 10000)"
    )
    parser.add_argument(
        "--seed", type=int, default=42, help="Random seed for reproducibility (default: 42)"
    )
    args = parser.parse_args()

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    selected, total_clusters, n_upi_skipped, n_unparsable_skipped = sample_clusters(
        args.uniref50_fasta, args.n, args.seed
    )
    write_outputs(
        selected,
        out_dir / "sampled_queries.fasta",
        out_dir / "sampled_queries_metadata.tsv",
        out_dir / "sampling_provenance.json",
        n=args.n,
        seed=args.seed,
        fasta_path=args.uniref50_fasta,
        total_clusters=total_clusters,
        n_upi_skipped=n_upi_skipped,
        n_unparsable_skipped=n_unparsable_skipped,
    )
    print(
        f"Sampled {len(selected)} clusters (requested {args.n}; skipped {n_upi_skipped} "
        f"UniParc-only, {n_unparsable_skipped} unparsable) out of {total_clusters} total "
        f"clusters in {args.uniref50_fasta}."
    )


if __name__ == "__main__":
    main()
