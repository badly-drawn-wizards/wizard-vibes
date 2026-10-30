#!/usr/bin/env bash
# Builds several variants of fut/ at once (see dev/variant.sh); each build is
# mostly single-threaded, so they run in parallel. Reads one variant per line
# from stdin, in dev/variant.sh's argument syntax (shell-quoted); a line of
# just a name builds fut/ unchanged under that name. Prints "name id" per
# variant as each finishes, then the ids comma-joined in input order, ready
# for dev/bench.sh.
#
# usage: dev/variants.sh <<'EOF'
#          base
#          noao shading.fut 's|let occ = .*|let occ = 1f32|'
#          steps march.fut 's/0.85/0.9/'
#        EOF
set -u
names=(); pids=(); outs=()
dir=$(mktemp -d)
while IFS= read -r line; do
  [ -z "${line// }" ] && continue
  eval "set -- $line"
  out="$dir/$1"
  names+=("$1"); outs+=("$out")
  (dev/variant.sh "$@" > "$out" && cat "$out") &
  pids+=($!)
done
status=0
for p in "${pids[@]}"; do wait "$p" || status=1; done
ids=()
for o in "${outs[@]}"; do [ -s "$o" ] && ids+=("$(cut -d' ' -f2 < "$o")"); done
rm -rf "$dir"
(IFS=,; echo "${ids[*]}")
exit $status
