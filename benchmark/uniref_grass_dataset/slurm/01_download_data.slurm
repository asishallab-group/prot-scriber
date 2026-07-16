#!/bin/bash
#SBATCH --job-name=grass_01_download
#SBATCH --output=data/logs/01_download_data_%j.out
#SBATCH --error=data/logs/01_download_data_%j.err
#SBATCH --time=12:00:00          # PLACEHOLDER -- edit for this cluster
#SBATCH --cpus-per-task=8        # PLACEHOLDER -- edit for this cluster
#SBATCH --mem=32G                # PLACEHOLDER -- edit for this cluster
#SBATCH --partition=PLACEHOLDER  # PLACEHOLDER -- edit for this cluster
#SBATCH --account=PLACEHOLDER    # PLACEHOLDER -- edit for this cluster
#
# Stage 1: download UniRef50 + a local refseq_protein BLAST database, and build the
# DIAMOND index for it. Needs outbound internet access (confirmed available on this
# cluster's compute nodes). Submit from inside benchmark/uniref_grass_dataset/:
#   cd benchmark/uniref_grass_dataset && sbatch slurm/01_download_data.slurm
set -euo pipefail

source "$(conda info --base)/etc/profile.d/conda.sh"
conda activate uniref-grass-benchmark

RAW_DIR="data/raw"
PROCESSED_DIR="data/processed"
mkdir -p "$RAW_DIR" "$PROCESSED_DIR" data/logs

echo "[1/4] Downloading UniRef50 FASTA..."
if [ ! -f "$RAW_DIR/uniref50.fasta" ]; then
    wget -q "https://ftp.uniprot.org/pub/databases/uniprot/uniref/uniref50/uniref50.fasta.gz" \
        -O "$RAW_DIR/uniref50.fasta.gz"
    gunzip -f "$RAW_DIR/uniref50.fasta.gz"
fi

echo "[2/4] Downloading pre-formatted refseq_protein BLAST database..."
mkdir -p "$RAW_DIR/refseq_protein_blastdb"
(
    cd "$RAW_DIR/refseq_protein_blastdb"
    update_blastdb.pl --decompress refseq_protein
)

echo "[3/4] Dumping full refseq_protein FASTA from the BLAST database..."
# One fetched copy serves both the FASTA dump below and the DIAMOND db build, so
# sseqid/stitle/full_sseq values are guaranteed consistent downstream (no risk of a
# second, separately-sourced refseq_protein copy disagreeing on accessions/deflines).
blastdbcmd -db "$RAW_DIR/refseq_protein_blastdb/refseq_protein" -entry all \
    -out "$RAW_DIR/refseq_protein.fasta"

echo "[4/4] Building the DIAMOND database from that same dump..."
diamond makedb --in "$RAW_DIR/refseq_protein.fasta" -d "$PROCESSED_DIR/refseq_protein" \
    --threads "${SLURM_CPUS_PER_TASK:-4}"

echo "Stage 1 complete."
