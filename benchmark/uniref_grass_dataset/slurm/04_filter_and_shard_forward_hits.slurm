#!/bin/bash
#SBATCH --job-name=grass_04_filter_shard
#SBATCH --output=data/logs/04_filter_and_shard_%j.out
#SBATCH --error=data/logs/04_filter_and_shard_%j.err
#SBATCH --time=02:00:00
#SBATCH --cpus-per-task=2
#SBATCH --mem=16G
#
# Stage 4: drop fully-identical hits and shard the remainder by query for Stage 5.
# NUM_SHARDS must match the --array size in slurm/05_backward_search.slurm. Override
# both together at submission time if you change it, e.g.:
#   sbatch --export=ALL,NUM_SHARDS=400 slurm/04_filter_and_shard_forward_hits.slurm
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

NUM_SHARDS="${NUM_SHARDS:-200}"

mkdir -p data/processed/backward_inputs data/logs

python3 python/filter_and_shard_forward_hits.py \
    --forward-tsv data/results/forward_search.tsv \
    --sampled-queries-fasta data/processed/sampled_queries.fasta \
    --out-filtered-tsv data/results/filtered_forward_hits.tsv \
    --shard-dir data/processed/backward_inputs \
    --num-shards "$NUM_SHARDS"

echo "Stage 4 complete (num_shards=$NUM_SHARDS)."
