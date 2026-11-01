#!/usr/bin/env bash
# usage: dev/bench.sh [-k] id1,id2 [rounds (9: ~0.25 ms resolution)] [frames] [browsers] [t0]
#   -k: list each kernel's GPU time per frame (needs timestamp queries)
# (needs dev/bench-daemon.ts running)
k=""; [ "${1:-}" = "-k" ] && { k="&kernels=1"; shift; }
curl -s "localhost:8091/bench?b=$1&reps=${2:-9}&frames=${3:-12}${4:+&browsers=$4}${5:+&t0=$5}$k"
