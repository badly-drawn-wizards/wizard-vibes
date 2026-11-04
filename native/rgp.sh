#!/usr/bin/env bash
# Capture a Radeon GPU Profiler trace of one bench frame (RADV only).
#   native/rgp.sh NAME [app args...]   e.g. native/rgp.sh wgsl --wgsl
# Writes build/rgp/NAME.rgp; open it with RGP (see README.md, Profiling).
set -euo pipefail
cd "$(dirname "$0")/.."
name=$1; shift
out=build/rgp
mkdir -p "$out"
stamp=$(date +%s)
# One capture per submit: frame 0 warms up, frame 1 is kept. A 640x360 frame
# fits a 256 MiB trace buffer; at 1080p instruction timing overflows it.
MESA_VK_TRACE=rgp MESA_VK_TRACE_PER_SUBMIT=1 \
RADV_THREAD_TRACE_BUFFER_SIZE=$((256 * 1024 * 1024)) \
  nix develop -c ./target/release/wizard --bench 2 --size 640x360 "$@"
# RADV always writes to /tmp, named after the executable and the time.
caps=()
for f in /tmp/wizard_*_submit*.rgp; do
  [ "$(stat -c %Y "$f")" -ge "$stamp" ] && caps+=("$f")
done
[ ${#caps[@]} -ge 2 ] || { echo "no capture found" >&2; exit 1; }
mv "${caps[1]}" "$out/$name.rgp"
rm -f "${caps[@]}"
echo "wrote build/rgp/$name.rgp"
