#!/bin/bash
# Proves the tap PCM passes a stream through byte for byte: raw frames of
# each format play through `type glasstap` into a file, and the file must
# begin with exactly the bytes that went in (aplay pads the last period
# with silence). Meanwhile tapdump shows the ring the tap wrote. Needs
# aplay and the host's libasound; the library defaults to the host build.
#   scripts/tap_exact.sh [libglasstap.so] [tapdump]

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
# ALSA loads the library by the path given, so it must be absolute.
LIB=$(realpath "${1:-$ROOT/target/release/libglasstap.so}")
DUMP=$(realpath "${2:-$ROOT/target/release/tapdump}")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

cat > "$WORK/asound.conf" <<EOF
</usr/share/alsa/alsa.conf>
pcm_type.glasstap { lib "$LIB" }
pcm.tap { type glasstap  slave.pcm "sink"  ring "exact"  fft_size 1024 }
pcm.sink { type file  slave.pcm "null"  file "$WORK/out.raw"  format "raw" }
EOF
export ALSA_CONFIG_PATH="$WORK/asound.conf"

for spec in "S16_LE 48000" "S24_3LE 96000" "S32_LE 192000" "DSD_U32_LE 88200"; do
  set -- $spec
  fmt=$1
  rate=$2
  head -c 3000000 /dev/urandom > "$WORK/in.raw"
  rm -f "$WORK/out.raw"
  aplay -q -t raw -f "$fmt" -r "$rate" -c 2 -D tap "$WORK/in.raw" &
  play_pid=$!
  sleep 0.5
  "$DUMP" 1 > "$WORK/dump.txt" 2>&1 || true
  wait "$play_pid"
  size=$(stat -c %s "$WORK/in.raw")
  out=$(stat -c %s "$WORK/out.raw" 2>/dev/null || echo 0)
  if [ "$out" -ge "$size" ] && cmp -s -n "$size" "$WORK/in.raw" "$WORK/out.raw"; then
    echo "tap: $fmt at $rate passes byte for byte ($size of $out bytes); ring: $(tail -n 1 "$WORK/dump.txt")"
  else
    echo "tap: $fmt at $rate DIFFERS ($size in, $out out): $(cmp -n "$size" "$WORK/in.raw" "$WORK/out.raw" 2>&1 | head -n 1)"
    exit 1
  fi
done
echo "tap: exact"
