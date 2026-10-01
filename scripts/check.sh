#!/bin/bash
# The workshop pass CI runs: formatting, lints with warnings denied, tests,
# and the documentation with warnings denied. Run it before a commit.
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"

echo "check: format"
cargo fmt --all -- --check

echo "check: clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "check: tests"
cargo test --workspace --locked

echo "check: browser module"
# The face: the pipeline for a browser, linted and built for its own target.
cargo clippy -p glass-face --target wasm32-unknown-unknown --locked -- -D warnings
cargo build -p glass-face --profile face --target wasm32-unknown-unknown --locked

echo "check: documentation"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

echo "check: ALSA templates"
scripts/asound_check.sh

echo "check: shell scripts"
for script in get-glass.sh scripts/*.sh remote/linux/*.sh plugin/*.sh; do
  sh -n "$script" || { echo "check: $script does not parse" >&2; exit 1; }
done

echo "check: windows installer"
# The installer is run, with -Check, against both layouts it accepts: the
# release archive (bin/glass.exe) and a checkout after the cross build
# (bin/windows-x64/glass.exe); and it must refuse a layout with neither.
# PowerShell on the machine, else the PowerShell image; skipped without both.
if command -v pwsh >/dev/null 2>&1; then
  run_installer() { pwsh -NoProfile -File "$1/remote/windows/install.ps1" "${@:2}"; }
elif command -v docker >/dev/null 2>&1; then
  run_installer() { docker run --rm --user "$(id -u):$(id -g)" -v "$1:/layout:ro" mcr.microsoft.com/powershell pwsh -NoProfile -File /layout/remote/windows/install.ps1 "${@:2}"; }
else
  run_installer() { echo "check: windows installer skipped (no pwsh, no docker)"; return 0; }
fi
if command -v pwsh >/dev/null 2>&1 || command -v docker >/dev/null 2>&1; then
  layout=$(mktemp -d)
  for bin in bin bin/windows-x64; do
    rm -rf "$layout"/* && mkdir -p "$layout/$bin" "$layout/remote/windows"
    cp remote/windows/*.ps1 "$layout/remote/windows/"
    : > "$layout/$bin/glass.exe"; : > "$layout/$bin/SDL2.dll"
    said=$(run_installer "$layout" -Check) || { echo "check: install.ps1 fails with the display under $bin" >&2; rm -rf "$layout"; exit 1; }
    echo "$said" | grep -q "would install from" || { echo "check: install.ps1 -Check said: $said" >&2; rm -rf "$layout"; exit 1; }
  done
  rm -rf "$layout"/* && mkdir -p "$layout/remote/windows" && cp remote/windows/*.ps1 "$layout/remote/windows/"
  if run_installer "$layout" -Check >/dev/null 2>&1; then
    echo "check: install.ps1 must refuse a layout without the display" >&2; rm -rf "$layout"; exit 1
  fi
  rm -rf "$layout"
else
  run_installer ""
fi

echo "check: page scripts"
# The remote's settings page and the manager's page carry their script
# inline; a browser runs all of it or none, so each must parse. The
# scripts stay beside their pages until the lint has read them too.
PAGES="bins/glass/src/remote/page.html plugin/manager/manage.html plugin/manager/anymote.html"
PAGE_SCRIPTS=""
for page in $PAGES; do PAGE_SCRIPTS="$PAGE_SCRIPTS $page.check.js"; done
# shellcheck disable=SC2086
trap 'rm -f $PAGE_SCRIPTS' EXIT
# shellcheck disable=SC2086
python3 - $PAGES <<'PY'
import re, sys
for path in sys.argv[1:]:
    text = open(path, encoding='utf-8').read()
    blocks = re.findall(r'<script>(.*?)</script>', text, re.S)
    if not blocks:
        print(f"check: {path} has no script", file=sys.stderr); sys.exit(1)
    open(path + '.check.js', 'w', encoding='utf-8').write('\n'.join(blocks))
PY
for page in $PAGES; do
  if command -v node >/dev/null 2>&1; then
    node --check "$page.check.js" || { echo "check: the script of $page does not parse" >&2; exit 1; }
  else
    docker run --rm -v "$ROOT:/glass:ro" node:20-slim node --check "/glass/$page.check.js" || { echo "check: the script of $page does not parse" >&2; exit 1; }
  fi
done

echo "check: plugin files"
# Volumio's core reads every plugin's strings at its start: one bad file
# takes the whole backend down, so every JSON the plugin ships must parse.
python3 - plugin/package.json plugin/config.json plugin/UIConfig.json plugin/i18n/*.json <<'PY'
import json, sys
for path in sys.argv[1:]:
    with open(path, encoding='utf-8') as f:
        text = f.read()
    json.loads(text)
    if not text.endswith('\n') or text.rstrip('\n') != text.rstrip():
        raise SystemExit(f'{path}: must end with one newline and nothing after the JSON')
print(f'{len(sys.argv) - 1} plugin files parse')
PY

echo "check: plugin"
if command -v node >/dev/null 2>&1; then
  node --check plugin/index.js plugin/manager/face-page.js plugin/manager/facefeed.js
  node --test plugin/manager/test/*.test.js
elif command -v docker >/dev/null 2>&1; then
  docker run --rm -v "$ROOT/plugin:/plugin:ro" -w /plugin node:20-slim sh -c 'node --check index.js manager/face-page.js manager/facefeed.js && node --test manager/test/*.test.js'
else
  echo "check: plugin: neither node nor docker found, skipped" >&2
fi

echo "check: undefined names"
# A name nothing defines parses and passes the tests, and throws when its
# line runs: the plugin's scripts, the browser module and the pages' own
# scripts are linted for it, with one pinned ESLint fetched for the run.
ESLINT="eslint@10.11.0"
# shellcheck disable=SC2086
LINT="npx --yes $ESLINT --config scripts/lint/eslint.config.js --no-warn-ignored plugin/index.js plugin/manager/*.js $PAGE_SCRIPTS"
if command -v npx >/dev/null 2>&1; then
  $LINT || { echo "check: a script uses a name nothing defines (or $ESLINT could not be fetched)" >&2; exit 1; }
elif command -v docker >/dev/null 2>&1; then
  docker run --rm -v "$ROOT:/glass:ro" -v glass-check-npm:/root/.npm -w /glass node:20-slim sh -c "$LINT" \
    || { echo "check: a script uses a name nothing defines (or $ESLINT could not be fetched)" >&2; exit 1; }
else
  echo "check: undefined names: neither node nor docker found, skipped" >&2
fi

echo "check: clean"
