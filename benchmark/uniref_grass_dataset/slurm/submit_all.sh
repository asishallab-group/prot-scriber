#!/bin/bash
# Submits all 6 pipeline stages in order, chaining each one via --dependency=afterok so a
# stage only starts once the previous one (all of it, including every task of the Stage 5
# array job) has finished successfully. This script itself is NOT a SLURM job -- run it
# directly (not via sbatch) from inside benchmark/uniref_grass_dataset/:
#
#   ./slurm/submit_all.sh
#   ./slurm/submit_all.sh --n-clusters 50000 --seed 7 --num-shards 400
#
# Prints each stage's job id as it's submitted; track progress with `squeue -u $USER` or
# `sacct`. If any `sbatch` call itself fails (e.g. a bad #SBATCH placeholder), this script
# stops immediately rather than chaining further stages onto a dependency that doesn't
# exist. If a later STAGE fails once running, SLURM's `afterok` dependency will simply
# leave the downstream stages queued forever (not started, not failed) -- cancel them with
# `scancel <jobid>` and re-run this script (or the individual sbatch commands) after fixing
# the problem.
set -euo pipefail

N_CLUSTERS=10000
SEED=42
NUM_SHARDS=200

while [[ $# -gt 0 ]]; do
    case "$1" in
        --n-clusters) N_CLUSTERS="$2"; shift 2 ;;
        --seed) SEED="$2"; shift 2 ;;
        --num-shards) NUM_SHARDS="$2"; shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."   # so the relative data/ paths in each .slurm script resolve correctly

mkdir -p data/logs

echo "Submitting pipeline: N_CLUSTERS=$N_CLUSTERS SEED=$SEED NUM_SHARDS=$NUM_SHARDS"
echo

jid1=$(sbatch --parsable slurm/01_download_data.slurm)
echo "  Stage 1 (download data):             job $jid1"

jid2=$(sbatch --parsable --dependency=afterok:"$jid1" \
    --export=ALL,N_CLUSTERS="$N_CLUSTERS",SEED="$SEED" slurm/02_sample_queries.slurm)
echo "  Stage 2 (sample queries):             job $jid2"

jid3=$(sbatch --parsable --dependency=afterok:"$jid2" \
    slurm/03_diamond_forward_search.slurm)
echo "  Stage 3 (diamond forward search):    job $jid3"

jid4=$(sbatch --parsable --dependency=afterok:"$jid3" \
    --export=ALL,NUM_SHARDS="$NUM_SHARDS" slurm/04_filter_and_shard_forward_hits.slurm)
echo "  Stage 4 (filter + shard):             job $jid4"

jid5=$(sbatch --parsable --dependency=afterok:"$jid4" \
    --array=0-$((NUM_SHARDS - 1)) slurm/05_backward_search.slurm)
echo "  Stage 5 (backward search, array):     job $jid5"

jid6=$(sbatch --parsable --dependency=afterok:"$jid5" \
    slurm/06_compute_grass_and_jaccard.slurm)
echo "  Stage 6 (grass + jaccard scoring):    job $jid6"

echo
echo "All 6 stages submitted and chained. Track with: squeue -u \$USER"
echo "Final dataset (once stage 6 finishes): data/results/uniref_grass_benchmark_dataset.tsv"
