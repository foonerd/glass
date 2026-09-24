#!/bin/bash
# Render one frame from a theme already on disk, with sample meter and spectrum
# bytes in the installed FIFOs. On a Volumio player this uses the installed
# config.txt. Next to a PeppyMeter checkout it uses that tree instead.

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
OUT=${1:-/tmp/glass-bar.ppm}
INSTALLED=/data/plugins/user_interface/peppy_screensaver/screensaver/peppymeter/config.txt
CFG=/tmp/glass-theme-config.txt

if [ -f "$ROOT/../PeppyMeter/480x320/meters.txt" ]; then
  THEME_ROOT=$(CDPATH= cd -- "$ROOT/../PeppyMeter" && pwd)
  cat > "$CFG" <<EOF
[current]
meter = bar
meter.folder = 480x320
base.folder = $THEME_ROOT
frame.rate = 30
screen.width =
screen.height =
EOF
  export GLASS_CONFIG=$CFG
elif [ -f "$INSTALLED" ]; then
  export GLASS_CONFIG=$INSTALLED
  echo "show-theme: using installed theme config $INSTALLED"
else
  echo "show-theme: no theme tree." >&2
  echo "show-theme: looked for $ROOT/../PeppyMeter and $INSTALLED" >&2
  exit 1
fi

make_fifo() {
  local path=$1
  if [ -p "$path" ]; then
    return 0
  fi
  if [ -e "$path" ]; then
    echo "show-theme: $path exists and is not a fifo" >&2
    exit 1
  fi
  mkfifo "$path"
}

make_fifo /tmp/myfifo
make_fifo /tmp/myfifosa

python3 - <<'PY' &
import os, struct, time
meter = os.open("/tmp/myfifo", os.O_RDWR)
spec = os.open("/tmp/myfifosa", os.O_RDWR)
meter_bytes = bytes([80, 0, 40, 0])
spec_bytes = b"".join(struct.pack("<i", i * 5) for i in range(20))
while True:
    os.write(meter, meter_bytes)
    os.write(spec, spec_bytes)
    time.sleep(0.2)
PY
HOLDER=$!
trap 'kill "$HOLDER" 2>/dev/null || true; wait "$HOLDER" 2>/dev/null || true' EXIT

echo "show-theme: frame is on the display. Ctrl-C to stop. Also writing $OUT"
"$ROOT/plugin/run_glass.sh" --output "$OUT"
