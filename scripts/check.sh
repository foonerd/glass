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

echo "check: glibc"
# The display must load on Volumio's glibc 2.36: a symbol versioned above
# it, even a weak one, makes the loader refuse the whole binary (the
# standard library's process spawn brings two such). The release build
# refuses such a binary (scripts/ship.sh); here the same is asked of this
# machine's build, which shows it wherever this machine's glibc is newer
# than the player's.
cargo build --locked -p glass
newest=$(readelf -W --dyn-syms target/debug/glass | grep -o 'GLIBC_2\.[0-9]*' | sort -t. -k2,2n -u | tail -n1)
case "$newest" in
  GLIBC_2.3[7-9]|GLIBC_2.[4-9]*|GLIBC_2.[1-9][0-9][0-9])
    echo "check: the display refers to $newest, newer than Volumio's glibc 2.36:" >&2
    readelf -W --dyn-syms target/debug/glass | grep "$newest" >&2
    exit 1
    ;;
esac

echo "check: browser module"
# The face: the pipeline for a browser, linted and built for its own target.
cargo clippy -p page -p glass-face --target wasm32-unknown-unknown --locked -- -D warnings
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
  # The bundle's layout: glass-evo's display in the archive is installed as the bundle.
  rm -rf "$layout"/* && mkdir -p "$layout/bin" "$layout/remote/windows"
  cp remote/windows/*.ps1 "$layout/remote/windows/"
  : > "$layout/bin/glass-evo.exe"; : > "$layout/bin/SDL2.dll"
  said=$(run_installer "$layout" -Check) || { echo "check: install.ps1 fails with glass-evo's display under bin" >&2; rm -rf "$layout"; exit 1; }
  echo "$said" | grep -q "would install from .*(the bundle)" || { echo "check: install.ps1 -Check said of the bundle: $said" >&2; rm -rf "$layout"; exit 1; }
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
# Node runs the checks of the pages and the plugin's files: here where it
# is installed, else in the node image.
if command -v node >/dev/null 2>&1; then
  with_node() { node "$@"; }
else
  # -i: a script given on standard input must reach the container.
  with_node() { docker run -i --rm -v "$ROOT:/glass" -w /glass node:20-slim node "$@"; }
fi
# shellcheck disable=SC2086
with_node - $PAGES <<'JS'
const fs = require('fs');
for (const page of process.argv.slice(2)) {
  const text = fs.readFileSync(page, 'utf8');
  const blocks = [...text.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
  if (!blocks.length) { console.error('check: ' + page + ' has no script'); process.exit(1); }
  fs.writeFileSync(page + '.check.js', blocks.join('\n'));
}
JS
for page in $PAGES; do
  with_node --check "$page.check.js" || { echo "check: the script of $page does not parse" >&2; exit 1; }
done

echo "check: plugin files"
# Volumio's core reads every plugin's strings at its start: one bad file
# takes the whole backend down, so every JSON the plugin ships must parse.
# A key written twice in one object parses, and the later one silently
# wins: that is refused too.
with_node - plugin/package.json plugin/config.json plugin/UIConfig.json plugin/i18n/*.json <<'JS'
const fs = require('fs');
// Every key of every object as written, in order: a JSON parser keeps the
// last of two, so the text is walked for the keys instead.
function keysWrittenTwice(text) {
  const twice = new Set();
  const stack = [];
  let i = 0;
  const skipString = () => { i++; while (i < text.length && text[i] !== '"') { if (text[i] === '\\') i++; i++; } return text.slice(start + 1, i++); };
  let start = 0;
  while (i < text.length) {
    const c = text[i];
    if (c === '{') { stack.push({ keys: new Set(), expectKey: true }); i++; }
    else if (c === '}') { stack.pop(); i++; }
    else if (c === '[') { stack.push(null); i++; }
    else if (c === ']') { stack.pop(); i++; }
    else if (c === '"') {
      start = i;
      const s = skipString();
      const top = stack[stack.length - 1];
      if (top && top.expectKey) { if (top.keys.has(s)) twice.add(s); top.keys.add(s); top.expectKey = false; }
    }
    else if (c === ':') { i++; }
    else if (c === ',') { const top = stack[stack.length - 1]; if (top) top.expectKey = true; i++; }
    else i++;
  }
  return [...twice].sort();
}
const files = process.argv.slice(2);
for (const file of files) {
  const text = fs.readFileSync(file, 'utf8');
  try { JSON.parse(text); } catch (e) { console.error(file + ': ' + e.message); process.exit(1); }
  const twice = keysWrittenTwice(text);
  if (twice.length) { console.error(file + ': written twice in one object: ' + twice.join(', ')); process.exit(1); }
  if (!text.endsWith('\n') || text.replace(/\n+$/, '') !== text.trimEnd()) { console.error(file + ': must end with one newline and nothing after the JSON'); process.exit(1); }
}
console.log(files.length + ' plugin files parse');
JS

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

# The install script carries the release signing key's public half: the
# one in the repository, or the first install verifies against nothing.
if ! grep -qF "$(sed -n '2p' keys/release-signing.pub)" get-glass.sh; then
  echo "check: get-glass.sh does not carry keys/release-signing.pub" >&2
  exit 1
fi

# No trace print left in the source: a line printed to find a fault once
# ships to every player's journal at every event it marks (0.9.27 left two
# at the persist event, found in 0.9.39). A print that names itself a trace
# is refused, outside comments.
echo "check: traces"
trace_prints() {
  grep -rnE "(eprintln!|println!|console\.(log|error|warn))\(\s*['\"]TRACE" --include=*.rs --include=*.js . 2>/dev/null \
    | grep -v "target/\|node_modules\|scripts/check.sh" \
    | grep -vE ":[0-9]+:[[:space:]]*(#|//)"
}
if trace_prints >/dev/null; then
  echo "check: a trace print is left in the source:" >&2
  trace_prints >&2
  exit 1
fi

# No python anywhere in Glass: the product is Rust and Node, and a player
# without python3 must install and run it. Looked for as a command run,
# outside comments; this script's own words and the plugin's prose aside.
python_runs() {
  grep -rnE "(^|[^A-Za-z_\"'/.-])python3?( |$|-)" --include=*.sh --include=*.js --include=*.rs --include=*.yml . 2>/dev/null \
    | grep -v "target/\|/\.sdl/\|/build/\|node_modules\|scripts/check.sh" \
    | grep -vE ":[0-9]+:[[:space:]]*(#|//)"
}
if python_runs >/dev/null; then
  echo "check: python is run in the source:" >&2
  python_runs >&2
  exit 1
fi

# No API key in the source: a 32-hex literal in the plugin is one.
if grep -nE "['\"][0-9a-f]{32}['\"]" plugin/index.js plugin/manager/*.js | grep -v "test/" >/dev/null; then
  echo "check: a key-like literal is in the plugin's source:" >&2
  grep -nE "['\"][0-9a-f]{32}['\"]" plugin/index.js plugin/manager/*.js | grep -v "test/" >&2
  exit 1
fi

echo "check: clean"
