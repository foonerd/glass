#!/bin/bash
# Proves the tap PCM passes a stream through byte for byte: raw frames of
# each format play through `type evotap` into a file, and the file must
# begin with exactly the bytes that went in (aplay pads the last period
# with silence). Meanwhile evotap-dump shows the ring the tap wrote. Then the
# same with the audio ring asked for (`audio 1`): evotap-cat captures the ring
# while the stream plays, and what it captured must be one contiguous run
# of the input (found by its first 64 bytes), most of it, with nothing
# lost. Needs aplay and the host's libasound; the binaries default to the
# host build.
#   scripts/tap_exact.sh [libevotap.so] [evotap-dump] [evotap-cat]

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
# ALSA loads the library by the path given, so it must be absolute.
LIB=$(realpath "${1:-$ROOT/target/release/libevotap.so}")
DUMP=$(realpath "${2:-$ROOT/target/release/evotap-dump}")
CAT=$(realpath "${3:-$ROOT/target/release/evotap-cat}")
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# The tap under its name, and under the name from before the evotap
# rename (`type glasstap`), which the library answers to for a release
# window so a chain not yet rewritten still loads it.
cat > "$WORK/asound.conf" <<EOF
</usr/share/alsa/alsa.conf>
pcm_type.evotap { lib "$LIB" }
pcm_type.glasstap { lib "$LIB" }
pcm.tap { type evotap  slave.pcm "sink"  ring "exact"  fft_size 1024 }
pcm.oldtap { type glasstap  slave.pcm "sink"  ring "exactold" }
pcm.tapaudio { type evotap  slave.pcm "sink"  ring "exactaudio"  audio 1  audio_seconds 4 }
pcm.sink { type file  slave.pcm "null"  file "$WORK/out.raw"  format "raw" }
EOF
export ALSA_CONFIG_PATH="$WORK/asound.conf"

# The old type name once, the new one for every format.
for spec in "S16_LE 48000 oldtap" "S16_LE 48000" "S24_3LE 96000" "S32_LE 192000" "DSD_U32_LE 88200"; do
  set -- $spec
  fmt=$1
  rate=$2
  pcm=${3:-tap}
  head -c 3000000 /dev/urandom > "$WORK/in.raw"
  rm -f "$WORK/out.raw"
  aplay -q -t raw -f "$fmt" -r "$rate" -c 2 -D "$pcm" "$WORK/in.raw" &
  play_pid=$!
  sleep 0.5
  "$DUMP" 1 > "$WORK/dump.txt" 2>&1 || true
  wait "$play_pid"
  size=$(stat -c %s "$WORK/in.raw")
  out=$(stat -c %s "$WORK/out.raw" 2>/dev/null || echo 0)
  if [ "$out" -ge "$size" ] && cmp -s -n "$size" "$WORK/in.raw" "$WORK/out.raw"; then
    echo "tap: $fmt at $rate through $pcm passes byte for byte ($size of $out bytes); ring: $(tail -n 1 "$WORK/dump.txt")"
  else
    echo "tap: $fmt at $rate through $pcm DIFFERS ($size in, $out out): $(cmp -n "$size" "$WORK/in.raw" "$WORK/out.raw" 2>&1 | head -n 1)"
    exit 1
  fi
done
echo "tap: exact"

# No audio ring unless asked: the pass above made none.
if ls /dev/shm/evoaudio.exact.* >/dev/null 2>&1; then
  echo "tap: an audio ring was made without being asked" >&2
  exit 1
fi

for spec in "S16_LE 48000" "S24_LE 96000" "S32_LE 192000" "DSD_U32_LE 88200"; do
  set -- $spec
  fmt=$1
  rate=$2
  # Random bytes without NUL or newline, so the capture's first 64 bytes
  # can be looked for in the input as a string.
  head -c 3200000 /dev/urandom | tr -d '\000\012' > "$WORK/plain.raw"
  head -c 3000000 "$WORK/plain.raw" > "$WORK/in.raw"
  rm -f "$WORK/out.raw" "$WORK/cap.raw" "$WORK/cat.txt"
  # evotap-cat waits for the ring the stream makes and takes it from there.
  "$CAT" --tag exactaudio --wait 10 > "$WORK/cap.raw" 2> "$WORK/cat.txt" &
  cat_pid=$!
  # The stream paced as a player's is, in pieces with a pause between: the
  # null device takes bytes as fast as they come, and a ring that lives
  # a few milliseconds is gone before any reader finds it.
  ( for i in $(seq 0 31); do dd if="$WORK/in.raw" bs=96000 skip="$i" count=1 status=none; sleep 0.05; done ) \
    | aplay -q -t raw -f "$fmt" -r "$rate" -c 2 -D tapaudio -
  wait "$cat_pid" || { echo "tap: evotap-cat failed: $(cat "$WORK/cat.txt")"; exit 1; }
  size=$(stat -c %s "$WORK/in.raw")
  cap=$(stat -c %s "$WORK/cap.raw" 2>/dev/null || echo 0)
  if [ "$cap" -lt $((size * 9 / 10)) ]; then
    echo "tap: audio ring: $fmt at $rate captured only $cap of $size bytes: $(cat "$WORK/cat.txt")"
    exit 1
  fi
  # Where the capture begins in the input: its first 64 bytes, found once
  # (after -e: the bytes may begin with a dash, which is not an option).
  start=$(grep -obUaF -e "$(head -c 64 "$WORK/cap.raw")" "$WORK/in.raw" | head -n 1 | cut -d: -f1)
  if [ -z "$start" ]; then
    echo "tap: audio ring: $fmt at $rate: the capture's start is not in the input"
    exit 1
  fi
  # From there the capture must be the input, byte for byte, as far as it goes
  # (aplay's padding of the last period may follow the input's end).
  compare=$(( size - start < cap ? size - start : cap ))
  tail -c +$((start + 1)) "$WORK/in.raw" > "$WORK/part.raw"
  if ! cmp -s -n "$compare" "$WORK/part.raw" "$WORK/cap.raw"; then
    echo "tap: audio ring: $fmt at $rate DIFFERS from byte $start: $(cmp -n "$compare" "$WORK/part.raw" "$WORK/cap.raw" 2>&1 | head -n 1)"
    exit 1
  fi
  if ! grep -q "0 bytes of the stream lost, 0 dropped" "$WORK/cat.txt"; then
    echo "tap: audio ring: $fmt at $rate lost bytes: $(cat "$WORK/cat.txt")"
    exit 1
  fi
  echo "tap: audio ring: $fmt at $rate captured $cap bytes from byte $start, byte for byte, nothing lost; $(grep segment "$WORK/cat.txt" | head -n 1 | sed 's/.*rate=/rate=/')"
done
echo "tap: audio ring exact"
