#!/bin/bash
#SBATCH --job-name=grass_06_score
#SBATCH --output=data/logs/06_compute_grass_and_jaccard_%j.out
#SBATCH --error=data/logs/06_compute_grass_and_jaccard_%j.err
#SBATCH --time=01:00:00
#SBATCH --cpus-per-task=4
#SBATCH --mem=16G
#
# Stage 6: join forward+backward results, compute GRASS + Jaccard, write the final
# benchmark dataset TSV -- this is the deliverable the issue asks for.
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

mkdir -p data/results data/logs

python3 python/compute_grass_and_jaccard.py \
    --filtered-forward-tsv data/results/filtered_forward_hits.tsv \
    --backward-shard-glob "data/results/backward_results_shard_*.tsv" \
    --sampled-queries-metadata data/processed/sampled_queries_metadata.tsv \
    --out-tsv data/results/uniref_grass_benchmark_dataset.tsv

echo "Stage 6 complete. Final dataset: data/results/uniref_grass_benchmark_dataset.tsv"
