#!/bin/bash
#SBATCH --job-name=grass_05_backward
#SBATCH --output=data/logs/05_backward_search_%A_%a.out
#SBATCH --error=data/logs/05_backward_search_%A_%a.err
#SBATCH --time=02:00:00          # PLACEHOLDER -- edit for this cluster
#SBATCH --cpus-per-task=2        # PLACEHOLDER -- edit for this cluster
#SBATCH --mem=4G                 # PLACEHOLDER -- edit for this cluster
#SBATCH --partition=PLACEHOLDER  # PLACEHOLDER -- edit for this cluster
#SBATCH --account=PLACEHOLDER    # PLACEHOLDER -- edit for this cluster
#SBATCH --array=0-199            # PLACEHOLDER -- must match --num-shards used in Stage 4
#
# Stage 5: reciprocal ("backward") realignment, one SLURM array task per shard. Each task
# batches all of its shard's queries into one `blastp -subject` call per query -- see
# python/run_backward_search.py's module docstring for why, and for the column-role-swap
# handling this step needs to get right.
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

mkdir -p data/results data/logs

SHARD_TAG=$(printf '%03d' "$SLURM_ARRAY_TASK_ID")

python3 python/run_backward_search.py \
    --shard-dir data/processed/backward_inputs \
    --shard-index "$SLURM_ARRAY_TASK_ID" \
    --out-tsv "data/results/backward_results_shard_${SHARD_TAG}.tsv" \
    --scratch-dir "${SLURM_TMPDIR:-/tmp}"

echo "Stage 5 shard $SLURM_ARRAY_TASK_ID complete."
