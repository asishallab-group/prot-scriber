"""Small helpers for reading/writing simple (non-huge) multi-FASTA files.

For the multi-GB UniRef50/RefSeq source files, see the streaming, offset-based approach in
`sample_uniref_queries.py` instead -- these helpers assume the whole file comfortably fits in
memory, which holds for this pipeline's own intermediate per-shard/per-query FASTA files (at
most tens of thousands of short protein sequences).
"""
from typing import Dict, Iterable, Tuple


def read_fasta_to_dict(path) -> Dict[str, str]:
    """Reads a FASTA file into a dict mapping the first whitespace-delimited token of each
    header (i.e. the sequence id) to its (concatenated, unwrapped) sequence."""
    records: Dict[str, str] = {}
    seq_id = None
    chunks = []
    with open(path) as fh:
        for line in fh:
            line = line.rstrip("\n")
            if line.startswith(">"):
                if seq_id is not None:
                    records[seq_id] = "".join(chunks)
                seq_id = line[1:].split()[0]
                chunks = []
            else:
                chunks.append(line.strip())
        if seq_id is not None:
            records[seq_id] = "".join(chunks)
    return records


def write_fasta(records: Iterable[Tuple[str, str]], path) -> None:
    """Writes `(seq_id, sequence)` pairs as a FASTA file (one line per sequence, unwrapped)."""
    with open(path, "w") as fh:
        for seq_id, sequence in records:
            fh.write(f">{seq_id}\n{sequence}\n")
