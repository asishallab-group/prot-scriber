#!/bin/bash
#SBATCH --job-name=grass_02_sample
#SBATCH --output=data/logs/02_sample_queries_%j.out
#SBATCH --error=data/logs/02_sample_queries_%j.err
#SBATCH --time=04:00:00
#SBATCH --cpus-per-task=2
#SBATCH --mem=16G
#
# Stage 2: sample N_CLUSTERS UniRef50 cluster representatives. Default is 10000; override
# at submission time, e.g.:
#   sbatch --export=ALL,N_CLUSTERS=50000 slurm/02_sample_queries.slurm
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

N_CLUSTERS="${N_CLUSTERS:-10000}"
SEED="${SEED:-42}"

mkdir -p data/processed data/logs

python3 python/sample_uniref_queries.py \
    --uniref50-fasta data/raw/uniref50.fasta \
    --out-dir data/processed \
    --n-clusters "$N_CLUSTERS" \
    --seed "$SEED"

echo "Stage 2 complete (n_clusters=$N_CLUSTERS, seed=$SEED)."
