#!/bin/sh
# Install Glass on a Volumio player from its latest release, or from a
# release named on the command line:
#
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- 0.7.7
#
# Run as the volumio user on the player. The plugin zip is downloaded from
# GitHub into the place the player's plugin manager takes dropped files,
# handed to that manager over the player's own socket, and enabled; the
# player's backend is then restarted so the audio tap joins the sound path.
# An installed Glass upgrades itself from its manager's System tab instead.
set -eu

REPO=foonerd/glass
WANTED=${1:-}
DROP=/tmp/plugins
NODE=/usr/bin/node
SOCKET_CLIENT=/volumio/node_modules/socket.io-client

say() { printf '%s\n' "$*"; }
fail() { printf 'get-glass: %s\n' "$*" >&2; exit 1; }

[ "$(id -un)" = volumio ] || fail "run this as the volumio user on the player"
[ -d /volumio ] && [ -x "$NODE" ] && [ -d "$SOCKET_CLIENT" ] || fail "this is not a Volumio player"
command -v curl >/dev/null 2>&1 || fail "curl is needed"
if [ -d /data/plugins/user_interface/glass ]; then
  fail "Glass is installed already: upgrade it from its manager's System tab, or uninstall it first"
fi
if [ -f /data/configuration/plugins.json ] && "$NODE" -e '
  const p = JSON.parse(require("fs").readFileSync("/data/configuration/plugins.json", "utf8"));
  const s = (p.user_interface || {}).peppy_screensaver || {};
  process.exit(s.enabled && s.enabled.value === true ? 0 : 1);
'; then
  fail "PeppyMeter Screensaver is enabled; disable it under Plugins, then run this again. Glass takes its themes and settings over"
fi

# The release: the latest, or the one asked for.
if [ -z "$WANTED" ]; then
  say "Looking up the latest release of Glass"
  WANTED=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | "$NODE" -e '
    let s = ""; process.stdin.on("data", d => s += d).on("end", () => {
      const r = JSON.parse(s); process.stdout.write(String(r.tag_name || "").replace(/^v/, ""));
    });')
  [ -n "$WANTED" ] || fail "the latest release could not be read from GitHub"
fi
WANTED=${WANTED#v}
ZIP="glass-$WANTED.zip"
URL="https://github.com/$REPO/releases/download/v$WANTED/$ZIP"

mkdir -p "$DROP"
say "Downloading $ZIP"
curl -fL --progress-bar -o "$DROP/$ZIP" "$URL" || fail "$URL could not be downloaded"

# The player's plugin manager installs it and enables it, as the
# interface's plugin page would. The events it sends tell how it goes.
say "Handing $ZIP to the player's plugin manager"
"$NODE" - "$ZIP" <<'JS'
const io = require('/volumio/node_modules/socket.io-client');
const zip = process.argv[2];
const socket = io.connect('http://localhost:3000', { reconnection: false });
let timer = setTimeout(() => { console.error('get-glass: the player did not answer in time'); process.exit(3); }, 420000);
const done = (code) => { clearTimeout(timer); socket.close(); setTimeout(() => process.exit(code), 200); };
socket.on('connect_error', (e) => { console.error('get-glass: cannot reach the player: ' + e.message); done(4); });
socket.on('connect', () => {
  let last = '';
  socket.on('installPluginStatus', (d) => {
    const line = (d.progress !== undefined ? d.progress + '% ' : '') + (d.message || '');
    if (line !== last) { console.log('  ' + line); last = line; }
  });
  socket.on('pushInstallPlugin', (d) => {
    const message = String((d && d.message) || '');
    if (/fail|error/i.test(message)) { console.error('get-glass: the plugin manager refused: ' + message); return done(1); }
    console.log('Installed. Enabling Glass');
    socket.on('pushInstalledPlugins', (list) => {
      const glass = (list || []).find((p) => p.name === 'glass');
      if (glass && glass.enabled) { console.log('Glass ' + glass.version + ' is enabled'); done(0); }
    });
    socket.emit('pluginManager', { action: 'enable', category: 'user_interface', name: 'glass' });
    setTimeout(() => { console.log('Glass enabled'); done(0); }, 20000);
  });
  socket.emit('installPlugin', { url: 'http://127.0.0.1:3000/plugin-serve/' + zip, confirm: true });
});
JS

# The plugin manager writes its registry a moment after it enables the
# plugin; a restart before that brings the player back with Glass installed
# and off. The restart waits until the registry on disk says enabled.
enabled() {
  "$NODE" -e '
    try {
      const p = JSON.parse(require("fs").readFileSync("/data/configuration/plugins.json", "utf8"));
      const g = (p.user_interface || {}).glass || {};
      process.exit(g.enabled && g.enabled.value === true ? 0 : 1);
    } catch (e) { process.exit(1); }'
}
waited=0
until enabled; do
  waited=$((waited + 1))
  [ "$waited" -le 30 ] || fail "Glass is installed, but the player has not recorded it as enabled: enable it under Settings, Plugins, Installed Plugins"
  sleep 1
done

say "Restarting the player's backend so the audio tap joins the sound path"
volumio vrestart >/dev/null 2>&1 || true
say ""
say "Glass $WANTED is installed. Play something: the meters appear after the screensaver timeout."
say "Its settings are under Settings, Plugins, Installed Plugins, Glass; its manager is at http://$(hostname).local:5582/"
