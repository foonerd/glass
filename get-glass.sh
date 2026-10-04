#!/bin/sh
# Install Glass on a Volumio player from its latest release, or from a
# release named on the command line:
#
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- 0.7.7
#
# And on a player that has Glass, the way back to the stable release when
# its manager does not come up to do it (the System tab's "Back to the
# stable release" is the same act, with its questions):
#
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- --stable
#
# It puts the latest release that is no test release in place and the
# settings back to what that release comes with, keeping the screen's and
# the network's set-up, after writing a settings backup named before-stable
# that the manager's Backups tab restores. glass-evo and whose the screen is
# are left as they are: the manager, once it is back, has both.
#
# Run as the volumio user on the player. The plugin zip is downloaded from
# GitHub into the place the player's plugin manager takes dropped files,
# handed to that manager over the player's own socket, and enabled; the
# player's backend is then restarted so the audio tap joins the sound path.
# An installed Glass upgrades itself from its manager's System tab instead.
set -eu

REPO=foonerd/glass
STABLE=no
WANTED=
for arg in "$@"; do
  case "$arg" in
    --stable) STABLE=yes ;;
    -*) printf 'get-glass: unknown argument %s\n' "$arg" >&2; exit 2 ;;
    *) WANTED=$arg ;;
  esac
done
PLUGIN=/data/plugins/user_interface/glass
SETTINGS=/data/configuration/user_interface/glass/config.json
BACKUPS=/data/INTERNAL/glass/backups
DROP=/tmp/plugins
NODE=/usr/bin/node
SOCKET_CLIENT=/volumio/node_modules/socket.io-client

say() { printf '%s\n' "$*"; }
fail() { printf 'get-glass: %s\n' "$*" >&2; exit 1; }

[ "$(id -un)" = volumio ] || fail "run this as the volumio user on the player"
[ -d /volumio ] && [ -x "$NODE" ] && [ -d "$SOCKET_CLIENT" ] || fail "this is not a Volumio player"
command -v curl >/dev/null 2>&1 || fail "curl is needed"
HERE=
if [ -f "$PLUGIN/package.json" ]; then
  HERE=$("$NODE" -e 'try { process.stdout.write(String(require(process.argv[1]).version || "")); } catch (e) {}' "$PLUGIN/package.json")
fi
if [ "$STABLE" = yes ]; then
  [ -z "$WANTED" ] || fail "--stable goes to the latest stable release; it takes no version"
  command -v python3 >/dev/null 2>&1 || fail "python3 is needed for --stable"
elif [ -d "$PLUGIN" ]; then
  fail "Glass is installed already: upgrade it from its manager's System tab, run this with --stable to go back to the stable release, or uninstall it first"
fi
if [ -z "$HERE" ] && [ -f /data/configuration/plugins.json ] && "$NODE" -e '
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

# The way back to the stable release, on a player that has Glass: the
# settings as they stand are backed up, and what they become is made by the
# stable release's own plan (its manager/stable.js, out of its zip): its
# defaults, with the screen's and the network's set-up kept.
PLANNED=
if [ "$STABLE" = yes ] && [ -n "$HERE" ]; then
  WORK=$(mktemp -d)
  python3 - "$DROP/$ZIP" "$WORK" <<'PY' || fail "the release's zip could not be read"
import sys, zipfile
zip_path, out = sys.argv[1:3]
with zipfile.ZipFile(zip_path) as z:
    for name in z.namelist():
        top = name.split('/')
        wanted = (name in ('package.json', 'config.json', 'config/meter.txt.tmpl', 'config/spectrum.txt.tmpl')
                  or (len(top) == 2 and top[0] == 'manager' and name.endswith('.js'))
                  or name.startswith('node_modules/ini/'))
        if wanted and not name.endswith('/') and '..' not in top:
            z.extract(name, out)
PY
  [ -f "$WORK/manager/stable.js" ] || fail "Glass $WANTED has no way back of its own (it is older than 0.8.52); install it from its manager instead"
  "$NODE" - "$WORK" "$PLUGIN" "$SETTINGS" "$BACKUPS" "$HERE" <<'JS' || fail "the settings could not be planned; nothing was changed"
const fs = require('fs');
const path = require('path');
const [work, plugin, settings, backups, here] = process.argv.slice(2);
const ini = require(path.join(work, 'node_modules/ini'));
const stable = require(path.join(work, 'manager/stable.js'));
const text = (file) => { try { return fs.readFileSync(file, 'utf8'); } catch (e) { return null; } };
const parsed = (value, parse) => { try { return value === null ? {} : parse(value); } catch (e) { return {}; } };
const now = { config: text(settings), meter: text(path.join(plugin, 'config/meter.txt')), spectrum: text(path.join(plugin, 'config/spectrum.txt')) };
// The backup, as the manager's own are: three files and a manifest.
let name = 'before-stable';
if (fs.existsSync(path.join(backups, name))) name += '-' + new Date().toISOString().replace(/[-:]/g, '').slice(0, 15).replace('T', '-');
const dir = path.join(backups, name);
fs.mkdirSync(dir, { recursive: true });
const kept = [['config.json', now.config], ['peppymeter_config.txt', now.meter], ['spectrum_config.txt', now.spectrum]];
kept.forEach(([file, value]) => fs.writeFileSync(path.join(dir, file), value === null ? '' : value));
fs.writeFileSync(path.join(dir, 'manifest.json'), JSON.stringify({ schema_version: 1, plugin_version: here, created: new Date().toISOString(), name: name, files: kept.map((k) => k[0]), automatic: true }, null, 2));
const current = { config: parsed(now.config, JSON.parse), meter: parsed(now.meter, ini.parse), spectrum: parsed(now.spectrum, ini.parse) };
const defaults = { config: JSON.parse(text(path.join(work, 'config.json'))), meter: ini.parse(text(path.join(work, 'config/meter.txt.tmpl'))), spectrum: ini.parse(text(path.join(work, 'config/spectrum.txt.tmpl'))) };
const keep = stable.answers(null, {});
const plan = stable.plan(current, defaults, keep);
const out = path.join(work, 'planned');
fs.mkdirSync(out, { recursive: true });
fs.writeFileSync(path.join(out, 'config.json'), JSON.stringify(plan.config, null, 4));
fs.writeFileSync(path.join(out, 'meter.txt'), ini.stringify(plan.meter, { whitespace: true }));
fs.writeFileSync(path.join(out, 'spectrum.txt'), ini.stringify(plan.spectrum, { whitespace: true }));
console.log('Settings backed up as ' + name + '; ' + stable.changed(current, plan) + ' settings go back to the defaults, kept: ' + Object.keys(keep).filter((k) => keep[k]).join(', '));
JS
  PLANNED=$WORK/planned
fi

# The player's plugin manager installs it and enables it, as the
# interface's plugin page would; a Glass that is here is replaced by its
# update. The events it sends tell how it goes.
HOW=installPlugin
[ -z "$HERE" ] || HOW=updatePlugin
if [ "$STABLE" = yes ] && [ "$HERE" = "$WANTED" ]; then
  say "Glass $HERE is the stable release: only the settings go back"
  HOW=
fi
[ -z "$HOW" ] || say "Handing $ZIP to the player's plugin manager"
[ -z "$HOW" ] || "$NODE" - "$ZIP" "$HOW" <<'JS'
const io = require('/volumio/node_modules/socket.io-client');
const zip = process.argv[2];
const how = process.argv[3];
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
  const url = 'http://127.0.0.1:3000/plugin-serve/' + zip;
  if (how === 'updatePlugin') socket.emit('updatePlugin', { url: url, category: 'user_interface', name: 'glass' });
  else socket.emit('installPlugin', { url: url, confirm: true });
});
JS

# The planned settings go in once the plugin is in place: its update lays
# fresh files of its own, and the player's own go over them.
if [ -n "$PLANNED" ]; then
  mkdir -p "$PLUGIN/config" "$(dirname "$SETTINGS")"
  cp "$PLANNED/meter.txt" "$PLUGIN/config/meter.txt"
  cp "$PLANNED/spectrum.txt" "$PLUGIN/config/spectrum.txt"
  cp "$PLANNED/config.json" "$SETTINGS"
  rm -rf "$WORK"
fi

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
if [ "$STABLE" = yes ] && [ -n "$HERE" ]; then
  say "Glass is at its stable release, $WANTED, with the settings of that release; the screen's and the network's set-up were kept."
  say "The settings as they were are a backup on the manager's Backups tab. glass-evo and whose the screen is were not touched:"
  say "the manager's System tab, Back to the stable release, has both."
else
  say "Glass $WANTED is installed. Play something: the meters appear after the screensaver timeout."
fi
say "Its settings are under Settings, Plugins, Installed Plugins, Glass; its manager is at http://$(hostname).local:5582/"
