"""Shared column-name constants for every TSV produced/consumed by this pipeline.

`diamond`/`blastp` tabular output (`-outfmt 6` / `--outfmt 6`) has no header row, so the column
order requested on each tool invocation must exactly match the constants here, and every script
that reads such a file must use these same names.
"""

# Stage 3: `diamond blastp --outfmt 6 <these columns, in this order>` against refseq_protein.
# `stitle` and `full_sseq` let later stages get each hit's description and full sequence
# directly from this one search, with no separate lookup step.
FORWARD_COLUMNS = [
    "qseqid",
    "sseqid",
    "pident",
    "length",
    "qstart",
    "qend",
    "qlen",
    "sstart",
    "send",
    "slen",
    "evalue",
    "bitscore",
    "stitle",
    "full_sseq",
]

# Stage 5: raw `blastp -outfmt 6 <these columns>` output from a single reciprocal call
# (`-query hits.fasta -subject query.fasta`). NOTE the roles are swapped relative to the
# forward search: here "qseqid"/"qstart"/"qend"/"qlen" refer to the HIT (it's the -query),
# and "sseqid"/"sstart"/"send"/"slen" refer to the original QUERY (it's the -subject).
RAW_BACKWARD_COLUMNS = [
    "qseqid",
    "sseqid",
    "pident",
    "length",
    "qstart",
    "qend",
    "qlen",
    "sstart",
    "send",
    "slen",
    "evalue",
    "bitscore",
]

# After renaming raw backward columns to their real-world roles (done once, right after
# parsing each `blastp -subject` call's output — see run_backward_search.py):
#   qseqid -> hit_id,   sseqid -> query_id,
#   qstart/qend/qlen -> hit_start/hit_end/hit_len     (coords & length ON THE HIT)
#   sstart/send/slen -> query_start/query_end/query_len (coords & length ON THE QUERY)
BACKWARD_COLUMNS = [
    "query_id",
    "hit_id",
    "pident",
    "length",
    "hit_start",
    "hit_end",
    "hit_len",
    "query_start",
    "query_end",
    "query_len",
    "evalue",
    "bitscore",
]

# Stage 2 sampling metadata (one row per sampled UniRef50 cluster representative):
SAMPLED_QUERY_METADATA_COLUMNS = [
    "cluster_id",
    "representative_accession",
    "description",
    "n_members",
    "tax_name",
    "taxid",
    "seq_length",
]

# Stage 6: the final deliverable dataset, one row per surviving (query, hit) pair.
FINAL_DATASET_COLUMNS = [
    "query_id",
    "query_description",
    "query_length",
    "query_n_members",
    "hit_id",
    "hit_description",
    "hit_length",
    "forward_pident",
    "forward_overlap",
    "forward_evalue",
    "forward_bitscore",
    "backward_pident",
    "backward_overlap",
    "backward_evalue",
    "backward_bitscore",
    "backward_alignment_found",
    "grass_score",
    "jaccard_similarity",
]
