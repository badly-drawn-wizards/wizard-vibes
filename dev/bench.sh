#!/usr/bin/env bash
# usage: dev/bench.sh id1,id2 [reps] [frames] [browsers]   (needs dev/bench-daemon.ts running)
curl -s "localhost:8091/bench?b=$1&reps=${2:-3}&frames=${3:-12}${4:+&browsers=$4}"
