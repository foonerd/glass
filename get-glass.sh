#!/bin/sh
# Install Glass on a Volumio player from its latest release, from a
# release named on the command line, or from a release zip brought by
# hand (downloaded on another machine, for a player that cannot reach
# GitHub):
#
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- 0.7.7
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- --zip glass-0.9.38.zip
#
# Every zip, downloaded or given, is held to the signature it carries
# inside before anything is installed: MANIFEST, each file's digest, and
# MANIFEST.sig, the manifest's Ed25519 signature made in the release
# workflow with the project's key. The check uses the player's own tools
# and nothing else: miniunzip unpacks the zip, openssl checks the signature
# with the public key below, sha256sum holds every file to the manifest,
# and a file the manifest does not name is refused. A zip that is not the
# release as published is refused. A release before 0.9.23 carries no
# signature: asked for by its version, it goes in with a warning.
#
#   ... | sh -s -- --check --zip glass-0.9.38.zip
#
# With --check the zip is verified and nothing is installed.
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
# Run as the volumio user on the player. The zip goes into the place the
# player's plugin manager takes dropped files, is handed to that manager
# over the player's own socket, and enabled; the player's backend is then
# restarted so the audio tap joins the sound path. An installed Glass
# upgrades itself from its manager's System tab instead.
#
# The player's Node is used in two places only, each one short-lived
# process with one job and a time limit: the settings plan of the way back,
# made by the stable release's own code, and the hand-over to the plugin
# manager, which is reachable over its socket alone. Everything before the
# zip is verified is plain sh over the player's tools.
set -eu

REPO=foonerd/glass
STABLE=no
CHECK=no
WANTED=
GIVEN=
TAKE_ZIP=no
for arg in "$@"; do
  if [ "$TAKE_ZIP" = yes ]; then
    GIVEN=$arg; TAKE_ZIP=no; continue
  fi
  case "$arg" in
    --stable) STABLE=yes ;;
    --check) CHECK=yes ;;
    --zip) TAKE_ZIP=yes ;;
    --zip=*) GIVEN=${arg#--zip=} ;;
    -*) printf 'get-glass: unknown argument %s\n' "$arg" >&2; exit 2 ;;
    *) WANTED=$arg ;;
  esac
done
[ "$TAKE_ZIP" = no ] || { printf 'get-glass: --zip takes the zip file\n' >&2; exit 2; }
# The project's release signing key, public half: keys/release-signing.pub
# in the repository, the same the plugin and the remotes carry.
PUBKEY='-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAoUKUKzZ9j8S3Kx64S7oeSfKLfb+VAWGaxA5b5ccFy58=
-----END PUBLIC KEY-----'
PLUGIN=/data/plugins/user_interface/glass
SETTINGS=/data/configuration/user_interface/glass/config.json
BACKUPS=/data/INTERNAL/glass/backups
DROP=/tmp/plugins
NODE=/usr/bin/node
SOCKET_CLIENT=/volumio/node_modules/socket.io-client
PLUGINS_JSON=/data/configuration/plugins.json

say() { printf '%s\n' "$*"; }
warn() { printf 'get-glass: %s\n' "$*" >&2; }
fail() { warn "$*"; exit 1; }

# The "version" a package.json names, by its text alone.
version_in() {
  sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$1" 2>/dev/null | head -n 1
}

# Whether the player's plugin registry says a plugin is enabled: the
# plugin's own object, its whitespace squeezed, read for enabled.value.
plugin_enabled() {
  [ -f "$PLUGINS_JSON" ] || return 1
  tr -d ' \n\t\r' < "$PLUGINS_JSON" | grep -o "\"$1\":{[^}]*\"enabled\":{[^}]*}" | grep -q '"value":true'
}

[ "$(id -un)" = volumio ] || fail "run this as the volumio user on the player"
[ -d /volumio ] && [ -x "$NODE" ] && [ -d "$SOCKET_CLIENT" ] || fail "this is not a Volumio player"
for tool in miniunzip openssl sha256sum find sort; do
  command -v "$tool" >/dev/null 2>&1 || fail "$tool is needed and the player does not have it"
done
[ -n "$GIVEN" ] || command -v curl >/dev/null 2>&1 || fail "curl is needed"
[ -z "$GIVEN" ] || [ -f "$GIVEN" ] || fail "$GIVEN is not a file"
[ -z "$GIVEN" ] || [ -z "$WANTED" ] || fail "--zip installs the version the zip holds; it takes no version"
HERE=
[ ! -f "$PLUGIN/package.json" ] || HERE=$(version_in "$PLUGIN/package.json")
if [ "$STABLE" = yes ]; then
  [ -z "$WANTED" ] || fail "--stable goes to the latest stable release; it takes no version"
  [ "$CHECK" = no ] || fail "--check verifies a zip and installs nothing; it does not go with --stable"
elif [ -d "$PLUGIN" ] && [ "$CHECK" = no ]; then
  fail "Glass is installed already: upgrade it from its manager's System tab, run this with --stable to go back to the stable release, or uninstall it first"
fi
if [ -z "$HERE" ] && [ "$CHECK" = no ] && plugin_enabled peppy_screensaver; then
  fail "PeppyMeter Screensaver is enabled; disable it under Plugins, then run this again. Glass takes its themes and settings over"
fi

# The zip unpacked into a folder of its own, by the player's miniunzip;
# the folder is removed when the script ends.
UNPACKED=
cleanup() { [ -z "$UNPACKED" ] || rm -rf "$UNPACKED"; }
trap cleanup EXIT
unpack_zip() {
  UNPACKED=$(mktemp -d) || fail "no temporary folder could be made"
  miniunzip -o "$1" -d "$UNPACKED" >/dev/null 2>&1 || return 1
}

# The unpacked zip held to the signature it carries inside. Prints the
# version and the file count; exit 2 for a zip with no signature at all (a
# release before 0.9.23), 1 for one that is not the release as published.
verify_unpacked() {
  dir=$1
  if [ ! -f "$dir/MANIFEST" ] && [ ! -f "$dir/MANIFEST.sig" ]; then
    return 2
  fi
  if [ ! -f "$dir/MANIFEST" ] || [ ! -f "$dir/MANIFEST.sig" ]; then
    warn "the zip carries half a signature; it is not the release as published"; return 1
  fi
  keyfile=$(mktemp) || return 1
  printf '%s\n' "$PUBKEY" > "$keyfile"
  if ! openssl pkeyutl -verify -pubin -inkey "$keyfile" -rawin -in "$dir/MANIFEST" -sigfile "$dir/MANIFEST.sig" >/dev/null 2>&1; then
    rm -f "$keyfile"
    warn "the zip's signature does not verify; it is not the release as published"; return 1
  fi
  rm -f "$keyfile"
  # Every file the manifest names is there and matches: the manifest is in
  # sha256sum's own format.
  if ! (cd "$dir" && sha256sum -c --quiet --strict MANIFEST >/dev/null 2>&1); then
    bad=$(cd "$dir" && sha256sum -c --quiet --strict MANIFEST 2>/dev/null | head -n 1 | sed 's/: FAILED.*$//')
    warn "${bad:-a file} does not match the zip's signed manifest"; return 1
  fi
  # And nothing the manifest does not name.
  have=$(cd "$dir" && find . -type f ! -name MANIFEST ! -name MANIFEST.sig | sed 's|^\./||' | LC_ALL=C sort)
  listed=$(sed 's/^[0-9a-fA-F]\{64\} *\**//' "$dir/MANIFEST" | sed 's/^ *//' | LC_ALL=C sort)
  if [ "$have" != "$listed" ]; then
    stray=$(printf '%s\n' "$have" | grep -vxF -e "$listed" | head -n 1)
    warn "the zip holds ${stray:-a file}, which its signed manifest does not name"; return 1
  fi
  printf '%s %s\n' "$(version_in "$dir/package.json")" "$(printf '%s\n' "$listed" | wc -l | tr -d ' ')"
}

# The release: the zip given, the latest, or the one asked for.
ASKED=$WANTED
mkdir -p "$DROP"
if [ -n "$GIVEN" ]; then
  unpack_zip "$GIVEN" || fail "$GIVEN is not a zip that miniunzip can open"
  WANTED=$(version_in "$UNPACKED/package.json")
  [ -n "$WANTED" ] || fail "$GIVEN is not a Glass release zip (it holds no package.json with a version)"
  ZIP="glass-$WANTED.zip"
  say "Taking $GIVEN, Glass $WANTED"
  cp "$GIVEN" "$DROP/$ZIP" || fail "$GIVEN could not be copied to $DROP"
else
  if [ -z "$WANTED" ]; then
    say "Looking up the latest release of Glass"
    WANTED=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | sed -n 's/.*"tag_name":[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)
    [ -n "$WANTED" ] || fail "the latest release could not be read from GitHub"
  fi
  WANTED=${WANTED#v}
  ZIP="glass-$WANTED.zip"
  URL="https://github.com/$REPO/releases/download/v$WANTED/$ZIP"
  say "Downloading $ZIP"
  curl -fL --progress-bar -o "$DROP/$ZIP" "$URL" || fail "$URL could not be downloaded"
  unpack_zip "$DROP/$ZIP" || { rm -f "$DROP/$ZIP"; fail "$ZIP is not a zip that miniunzip can open"; }
fi

# Held to the signature inside before anything else happens with it.
say "Verifying $ZIP"
VSTATUS=0
VERIFIED=$(verify_unpacked "$UNPACKED") || VSTATUS=$?
if [ "$VSTATUS" -eq 0 ]; then
  say "Verified: Glass ${VERIFIED% *}, ${VERIFIED#* } files held to the release's signature"
elif [ "$VSTATUS" -eq 2 ]; then
  # No signature at all: a release before 0.9.23. Only one asked for by
  # its version goes in, and said so; anything else is not as published.
  case "$ASKED" in
    0.[0-8].*|0.9.[0-9]|0.9.1[0-9]|0.9.2[0-2]) say "Warning: Glass $WANTED was published before releases were signed; nothing can verify it" ;;
    *) rm -f "$DROP/$ZIP"; fail "the zip carries no signature inside; every release from 0.9.23 on does, so it is not the release as published" ;;
  esac
else
  rm -f "$DROP/$ZIP"
  fail "nothing was installed"
fi
if [ "$CHECK" = yes ]; then
  rm -f "$DROP/$ZIP"
  say "Nothing installed (--check)."
  exit 0
fi

# The way back to the stable release, on a player that has Glass: the
# settings as they stand are backed up, and what they become is made by the
# stable release's own plan (its manager/stable.js, out of the unpacked
# zip): its defaults, with the screen's and the network's set-up kept. One
# Node process, one job, done before anything is touched.
PLANNED=
if [ "$STABLE" = yes ] && [ -n "$HERE" ]; then
  WORK=$UNPACKED
  [ -f "$WORK/manager/stable.js" ] || fail "Glass $WANTED has no way back of its own (it is older than 0.8.52); install it from its manager instead"
  "$NODE" - "$WORK" "$PLUGIN" "$SETTINGS" "$BACKUPS" "$HERE" <<'JS' || fail "the settings could not be planned; nothing was changed"
const fs = require('fs');
const path = require('path');
const [work, plugin, settings, backups, here] = process.argv.slice(2);
const ini = require(path.join(work, 'node_modules/ini'));
const stable = require(path.join(work, 'manager/stable.js'));
const text = (file) => { try { return fs.readFileSync(file, 'utf8'); } catch (e) { return null; } };
const parsed = (value, parse) => { try { return value === null ? {} : parse(value); } catch (e) { return {}; } };
// The files as they stand: under the data folder from 0.9.42, in the plugin folder before.
const data = path.join(path.dirname(backups), 'config');
const standing = (name) => text(path.join(data, name)) !== null ? path.join(data, name) : path.join(plugin, 'config', name);
const now = { config: text(settings), meter: text(standing('meter.txt')), spectrum: text(standing('spectrum.txt')) };
// Where the stable release reads them, for the planned files to go after its install.
const stableVersion = (() => { try { return JSON.parse(text(path.join(work, 'package.json'))).version; } catch (e) { return ''; } })();
const before = (v) => { const m = /^(\d+)\.(\d+)\.(\d+)/.exec(String(v || '')); if (!m) return false; const [a, b, c] = m.slice(1).map(Number); return a < 0 || (a === 0 && (b < 9 || (b === 9 && c < 42))); };
fs.writeFileSync(path.join(work, 'place'), before(stableVersion) ? path.join(plugin, 'config') : data);
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
# update. The events it sends tell how it goes. One Node process, one
# request over the manager's own socket, a time limit, then it exits.
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
  PLACE=$(cat "$WORK/place")
  mkdir -p "$PLACE" "$(dirname "$SETTINGS")"
  cp "$PLANNED/meter.txt" "$PLACE/meter.txt"
  cp "$PLANNED/spectrum.txt" "$PLACE/spectrum.txt"
  cp "$PLANNED/config.json" "$SETTINGS"
fi

# The plugin manager writes its registry a moment after it enables the
# plugin; a restart before that brings the player back with Glass installed
# and off. The restart waits until the registry on disk says enabled.
waited=0
until plugin_enabled glass; do
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
