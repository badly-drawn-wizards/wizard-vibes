#!/usr/bin/env bash
# Builds a variant of fut/ for benchmarking: copies fut/ to
# build/variants/<name>, applies sed expressions to its files, and builds it
# (content-addressed, so an unchanged variant is reused). Prints
# "<name> <build id>"; load it with ?build=<id> or time it with dev/bench.sh.
#
# usage: dev/variant.sh name [file sed-expr]...
#   e.g. dev/variant.sh noao shading.fut 's|let occ = .*|let occ = 1f32|'
set -u
name=$1; shift
d=build/variants/$name; rm -rf "$d"; mkdir -p "$d"; cp fut/*.fut "$d"/
while [ $# -gt 0 ]; do
  before=$(md5sum < "$d/$1")
  sed -i "$2" "$d/$1"
  [ "$before" = "$(md5sum < "$d/$1")" ] && { echo "$name: sed changed nothing in $1: $2" >&2; exit 1; }
  shift 2
done
id=$(node dev/build-fut.ts "$d" 2> >(sed "s/^/$name: /" >&2)) || exit 1
echo "$name $id"
