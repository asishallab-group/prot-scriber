#!/bin/bash
#SBATCH --job-name=grass_02_sample
#SBATCH --output=data/logs/02_sample_queries_%j.out
#SBATCH --error=data/logs/02_sample_queries_%j.err
#SBATCH --time=04:00:00          # PLACEHOLDER -- edit for this cluster
#SBATCH --cpus-per-task=2        # PLACEHOLDER -- edit for this cluster
#SBATCH --mem=16G                # PLACEHOLDER -- edit for this cluster
#SBATCH --partition=PLACEHOLDER  # PLACEHOLDER -- edit for this cluster
#SBATCH --account=PLACEHOLDER    # PLACEHOLDER -- edit for this cluster
#
# Stage 2: sample N UniRef50 cluster representatives. Default is 10000; override at
# submission time, e.g.:
#   sbatch --export=ALL,N=50000 slurm/02_sample_queries.slurm
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

N="${N:-10000}"
SEED="${SEED:-42}"

mkdir -p data/processed data/logs

python3 python/sample_uniref_queries.py \
    --uniref50-fasta data/raw/uniref50.fasta \
    --out-dir data/processed \
    --n "$N" \
    --seed "$SEED"

echo "Stage 2 complete (n=$N, seed=$SEED)."
