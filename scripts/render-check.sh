#!/bin/bash
# The display renders every shipped template before a tag: each meter of
# each theme under plugin/templates drawn headless by the binary just built,
# from a scratch home that names the templates as its base folder and the
# plugin's own fonts and icons. The run must leave cleanly, write one
# picture per meter the display itself lists, and every picture must be a
# PNG of the template's size (its folder name) with something in it.
#   scripts/render-check.sh [BINARY]
# BINARY is target/debug/glass unless given. Nothing of the player's is read.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN=${1:-$ROOT/target/debug/glass}
[ -x "$BIN" ] || { echo "render-check: no display binary at $BIN" >&2; exit 1; }
WORK=$(mktemp -d "${TMPDIR:-/tmp}/glass-render.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
HOME_DIR="$WORK/home"
mkdir -p "$HOME_DIR/config" "$WORK/out"
printf '[current]\nbase.folder = %s\nmeter = random\nrandom.meter.interval = 60\n' "$ROOT/plugin/templates" > "$HOME_DIR/config/meter.txt"
ln -s "$ROOT/plugin/fonts" "$HOME_DIR/fonts"
[ -d "$ROOT/plugin/format-icons" ] && ln -s "$ROOT/plugin/format-icons" "$HOME_DIR/format-icons"
export GLASS_HOME="$HOME_DIR"

# The PNG's size from its header: width and height, big-endian, at bytes 16 to 23.
png_size() {
  od -An -tu1 -j16 -N8 "$1" | awk '{ printf "%dx%d", ($1*16777216 + $2*65536 + $3*256 + $4), ($5*16777216 + $6*65536 + $7*256 + $8) }'
}

listing=$("$BIN" --headless --list 2>/dev/null) || { echo "render-check: --list failed" >&2; exit 1; }
themes=$(printf '%s\n' "$listing" | grep -E '^[^ ]' )
[ -n "$themes" ] || { echo "render-check: the display lists no template under plugin/templates" >&2; exit 1; }
failed=0
for theme in $themes; do
  meters=$(printf '%s\n' "$listing" | awk -v t="$theme" '$0 == t { on = 1; next } /^[^ ]/ { on = 0 } on { sub(/^  /, ""); print }')
  expected=$(printf '%s\n' "$meters" | grep -c .)
  [ "$expected" -gt 0 ] || { echo "render-check: $theme lists no meter" >&2; failed=1; continue; }
  if ! "$BIN" --headless --theme "$theme" --snapshot "$WORK/out" --settle 0.2 > "$WORK/$theme.log" 2>&1; then
    echo "render-check: $theme: the display did not leave cleanly:" >&2; tail -5 "$WORK/$theme.log" >&2; failed=1; continue
  fi
  bad=0
  printf '%s\n' "$meters" > "$WORK/$theme.meters"
  while IFS= read -r meter; do
    png="$WORK/out/$theme/$meter.png"
    if [ ! -s "$png" ]; then echo "render-check: $theme/$meter: no picture" >&2; bad=1; continue; fi
    size=$(png_size "$png")
    if [ "$size" != "$theme" ]; then echo "render-check: $theme/$meter: the picture is $size, the template $theme" >&2; bad=1; fi
    if [ "$(stat -c %s "$png")" -lt 1024 ]; then echo "render-check: $theme/$meter: the picture is empty ($(stat -c %s "$png") bytes)" >&2; bad=1; fi
  done < "$WORK/$theme.meters"
  got=$(find "$WORK/out/$theme" -name '*.png' 2>/dev/null | wc -l)
  if [ "$got" -ne "$expected" ]; then echo "render-check: $theme: $got pictures for $expected meters" >&2; bad=1; fi
  [ "$bad" = 0 ] && echo "render-check: $theme: $expected meters drawn" || failed=1
done
exit $failed
