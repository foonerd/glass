#!/bin/bash
# The plugin's ALSA templates go to the player as they are, so they must
# hold no variable left to fill and must parse as ALSA configuration. The
# parse needs aplay; without it only the variables are checked.

set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
status=0
for tmpl in "$ROOT"/plugin/Glass.postGlass.5*.conf.tmpl; do
  name=$(basename "$tmpl")
  if grep -n '\${' "$tmpl"; then
    echo "asound: $name holds a variable nothing fills"
    status=1
    continue
  fi
  if command -v aplay >/dev/null 2>&1; then
    work=$(mktemp -d)
    {
      echo '</usr/share/alsa/alsa.conf>'
      cat "$tmpl"
    } > "$work/asound.conf"
    if ALSA_CONFIG_PATH="$work/asound.conf" aplay -L >/dev/null 2> "$work/err"; then
      echo "asound: $name parses"
    else
      echo "asound: $name does not parse:"
      cat "$work/err"
      status=1
    fi
    rm -rf "$work"
  else
    echo "asound: $name holds no variable (no aplay here to parse it)"
  fi
done
exit $status
