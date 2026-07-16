#!/bin/bash
#SBATCH --job-name=grass_03_forward
#SBATCH --output=data/logs/03_diamond_forward_search_%j.out
#SBATCH --error=data/logs/03_diamond_forward_search_%j.err
#SBATCH --time=12:00:00          # PLACEHOLDER -- edit for this cluster
#SBATCH --cpus-per-task=32       # PLACEHOLDER -- edit for this cluster
#SBATCH --mem=128G               # PLACEHOLDER -- edit for this cluster
#SBATCH --partition=PLACEHOLDER  # PLACEHOLDER -- edit for this cluster
#SBATCH --account=PLACEHOLDER    # PLACEHOLDER -- edit for this cluster
#
# Stage 3: the large forward search -- sampled UniRef50 representatives vs. all of
# refseq_protein, via DIAMOND (far faster here than classic blastp at this scale).
# `stitle`/`full_sseq` give each hit's description and full sequence directly from this
# one search, so no separate lookup step is needed in later stages.
#
# --max-target-seqs/--evalue/--sensitive are reasonable starting defaults, not tuned --
# revisit after seeing real result volumes.
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

mkdir -p data/results data/logs

diamond blastp \
    --db data/processed/refseq_protein.dmnd \
    --query data/processed/sampled_queries.fasta \
    --out data/results/forward_search.tsv \
    --outfmt 6 qseqid sseqid pident length qstart qend qlen sstart send slen evalue bitscore stitle full_sseq \
    --max-target-seqs 50 \
    --evalue 1e-5 \
    --sensitive \
    --threads "${SLURM_CPUS_PER_TASK:-8}" \
    --quiet

echo "Stage 3 complete."
