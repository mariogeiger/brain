#!/usr/bin/env bash
# Copy the source MRI scan into the trunk asset dir so the page can fetch it.
# The .nii itself is gitignored — only the source on disk is the source of truth.

set -euo pipefail

# Single-volume EPI extract — no skull/scalp signal, so a windowed transfer
# function actually shows the brain. The full T1 is gorgeous 0.8mm but its
# bright fat/marrow shell dominates MIP/composite no matter how we window.
SRC="${SRC:-/Users/mgeiger/Documents/A003 IRM/_DrReve_SMS4_2ISO_TR1_pat2_PhysioLog_20250621120141_10_1.nii}"
DST_DIR="$(cd "$(dirname "$0")/.." && pwd)/client"
DST="$DST_DIR/brain.nii"

if [ ! -f "$SRC" ]; then
    echo "source not found: $SRC" >&2
    exit 1
fi

cp "$SRC" "$DST"
echo "copied $(stat -f %z "$DST") bytes → $DST"
