#!/bin/sh
# Install Glass on a Volumio player from its latest release, from a
# release named on the command line, or from a release zip brought by
# hand (downloaded on another machine, for a player that cannot reach
# GitHub):
#
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- 0.7.7
#   curl -fsSL https://raw.githubusercontent.com/foonerd/glass/main/get-glass.sh | sh -s -- --zip glass-0.9.37.zip
#
# Every zip, downloaded or given, is held to the signature it carries
# inside before anything is installed: MANIFEST, each file's digest, and
# MANIFEST.sig, the manifest's Ed25519 signature made in the release
# workflow with the project's key, checked here with the public key below
# and every file against the manifest, with no network. A zip that is not
# the release as published is refused. A release before 0.9.23 carries no
# signature: asked for by its version, it goes in with a warning.
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
GIVEN=
TAKE_ZIP=no
for arg in "$@"; do
  if [ "$TAKE_ZIP" = yes ]; then
    GIVEN=$arg; TAKE_ZIP=no; continue
  fi
  case "$arg" in
    --stable) STABLE=yes ;;
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

say() { printf '%s\n' "$*"; }
fail() { printf 'get-glass: %s\n' "$*" >&2; exit 1; }

[ "$(id -un)" = volumio ] || fail "run this as the volumio user on the player"
[ -d /volumio ] && [ -x "$NODE" ] && [ -d "$SOCKET_CLIENT" ] || fail "this is not a Volumio player"
[ -n "$GIVEN" ] || command -v curl >/dev/null 2>&1 || fail "curl is needed"
[ -z "$GIVEN" ] || [ -f "$GIVEN" ] || fail "$GIVEN is not a file"
[ -z "$GIVEN" ] || [ -z "$WANTED" ] || fail "--zip installs the version the zip holds; it takes no version"

# The release zip is read by the player's own Node and nothing else: its
# directory walked, each file inflated and digested, the manifest's
# signature held to the key above. Written once, used for the checks and
# for the way back's files.
ZIPCHECK=$(mktemp)
trap 'rm -f "$ZIPCHECK"' EXIT
cat > "$ZIPCHECK" <<'JS'
// The release zip read by the player's own Node, nothing else: its central
// directory walked, each file inflated and digested, the manifest and its
// Ed25519 signature held to the project's key. Modes:
//   verify ZIP PEM            prints "<version> <files>"; exit 2 for a zip with no
//                             signature at all, 1 for one that is not as published
//   version ZIP               prints the version package.json holds
//   extract ZIP DIR NAME...   writes the named members under DIR
'use strict';
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const crypto = require('crypto');

function fail(message, code) { process.stderr.write('get-glass: ' + message + '\n'); process.exit(code || 1); }

// The entries of a zip from its central directory, read from the end.
function entries(fd, size) {
  const tail = Math.min(size, 65536 + 22);
  const buf = Buffer.alloc(tail);
  fs.readSync(fd, buf, 0, tail, size - tail);
  let at = -1;
  for (let i = tail - 22; i >= 0; i--) { if (buf.readUInt32LE(i) === 0x06054b50) { at = i; break; } }
  if (at < 0) fail('not a zip: no end record');
  const count = buf.readUInt16LE(at + 10);
  const cdSize = buf.readUInt32LE(at + 12);
  const cdOffset = buf.readUInt32LE(at + 16);
  if (count === 0xffff || cdSize === 0xffffffff || cdOffset === 0xffffffff) fail('the zip is larger than this reader takes');
  const cd = Buffer.alloc(cdSize);
  fs.readSync(fd, cd, 0, cdSize, cdOffset);
  const list = [];
  let p = 0;
  for (let i = 0; i < count; i++) {
    if (p + 46 > cdSize || cd.readUInt32LE(p) !== 0x02014b50) fail('the zip\'s directory is damaged');
    const method = cd.readUInt16LE(p + 10);
    const crc = cd.readUInt32LE(p + 16);
    const packed = cd.readUInt32LE(p + 20);
    const unpacked = cd.readUInt32LE(p + 24);
    const nameLen = cd.readUInt16LE(p + 28), extraLen = cd.readUInt16LE(p + 30), commentLen = cd.readUInt16LE(p + 32);
    const offset = cd.readUInt32LE(p + 42);
    const name = cd.toString('utf8', p + 46, p + 46 + nameLen);
    list.push({ name, method, crc, packed, unpacked, offset, isDir: name.endsWith('/') });
    p += 46 + nameLen + extraLen + commentLen;
  }
  return list;
}

function read(fd, entry) {
  const head = Buffer.alloc(30);
  fs.readSync(fd, head, 0, 30, entry.offset);
  if (head.readUInt32LE(0) !== 0x04034b50) fail('the zip\'s entry ' + entry.name + ' is damaged');
  const start = entry.offset + 30 + head.readUInt16LE(26) + head.readUInt16LE(28);
  const packed = Buffer.alloc(entry.packed);
  fs.readSync(fd, packed, 0, entry.packed, start);
  let data;
  if (entry.method === 0) data = packed;
  else if (entry.method === 8) data = zlib.inflateRawSync(packed, { maxOutputLength: 256 * 1024 * 1024 });
  else fail('the zip\'s entry ' + entry.name + ' uses an unsupported compression');
  if (data.length !== entry.unpacked) fail('the zip\'s entry ' + entry.name + ' inflates to the wrong size');
  return data;
}

function open(file) {
  let fd, size;
  try { fd = fs.openSync(file, 'r'); size = fs.fstatSync(fd).size; } catch (e) { fail('the zip could not be read: ' + e.message); }
  return { fd, list: entries(fd, size) };
}

function versionOf(zip) {
  const entry = zip.list.find((e) => e.name === 'package.json');
  if (!entry) return '';
  try { return String(JSON.parse(read(zip.fd, entry).toString('utf8')).version || ''); } catch (e) { return ''; }
}

const [mode, file, ...rest] = process.argv.slice(2);
if (mode === 'version') {
  process.stdout.write(versionOf(open(file)));
} else if (mode === 'extract') {
  const [dir, ...names] = rest;
  const zip = open(file);
  for (const entry of zip.list) {
    const parts = entry.name.split('/');
    if (entry.isDir || parts.includes('..') || !names.some((n) => n.endsWith('/') ? entry.name.startsWith(n) : entry.name === n)) continue;
    const out = path.join(dir, entry.name);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.writeFileSync(out, read(zip.fd, entry));
  }
} else if (mode === 'verify') {
  const pem = rest[0];
  const zip = open(file);
  const files = zip.list.filter((e) => !e.isDir);
  const manifest = files.find((e) => e.name === 'MANIFEST');
  const sig = files.find((e) => e.name === 'MANIFEST.sig');
  if (!manifest && !sig) process.exit(2);
  if (!manifest || !sig) fail('the zip carries half a signature; it is not the release as published');
  if (manifest.unpacked > 4 * 1024 * 1024 || sig.unpacked !== 64) fail('the zip\'s manifest or signature has an unusable size');
  const manifestBytes = read(zip.fd, manifest);
  if (!crypto.verify(null, manifestBytes, pem, read(zip.fd, sig))) fail('the zip\'s signature does not verify; it is not the release as published');
  const listed = new Map();
  for (const line of manifestBytes.toString('utf8').split(/\r?\n/)) {
    const m = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line);
    if (m) listed.set(m[2], m[1].toLowerCase());
  }
  if (!listed.size) fail('the zip\'s manifest names no file');
  const seen = new Set();
  for (const entry of files) {
    if (entry.name === 'MANIFEST' || entry.name === 'MANIFEST.sig') continue;
    const stated = listed.get(entry.name);
    if (!stated) fail('the zip holds ' + entry.name + ', which its signed manifest does not name');
    if (crypto.createHash('sha256').update(read(zip.fd, entry)).digest('hex') !== stated) fail(entry.name + ' does not match the zip\'s signed manifest');
    seen.add(entry.name);
  }
  for (const name of listed.keys()) if (!seen.has(name)) fail('the zip lacks ' + name + ', which its signed manifest names');
  process.stdout.write(versionOf(zip) + ' ' + seen.size);
} else {
  fail('zipcheck: verify|version|extract');
}
JS

# A zip held to the signature it carries inside. Prints the version and
# the file count; exit 2 for a zip with no signature at all (a release
# before 0.9.23), 1 for one that is not the release as published.
verify_zip() {
  "$NODE" "$ZIPCHECK" verify "$1" "$PUBKEY"
}

# The version a zip holds, by its package.json.
zip_version() {
  "$NODE" "$ZIPCHECK" version "$1"
}
HERE=
if [ -f "$PLUGIN/package.json" ]; then
  HERE=$("$NODE" -e 'try { process.stdout.write(String(require(process.argv[1]).version || "")); } catch (e) {}' "$PLUGIN/package.json")
fi
if [ "$STABLE" = yes ]; then
  [ -z "$WANTED" ] || fail "--stable goes to the latest stable release; it takes no version"
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

# The release: the zip given, the latest, or the one asked for.
ASKED=$WANTED
mkdir -p "$DROP"
if [ -n "$GIVEN" ]; then
  WANTED=$(zip_version "$GIVEN")
  [ -n "$WANTED" ] || fail "$GIVEN is not a Glass release zip (it holds no package.json with a version)"
  ZIP="glass-$WANTED.zip"
  say "Taking $GIVEN, Glass $WANTED"
  cp "$GIVEN" "$DROP/$ZIP" || fail "$GIVEN could not be copied to $DROP"
else
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
  say "Downloading $ZIP"
  curl -fL --progress-bar -o "$DROP/$ZIP" "$URL" || fail "$URL could not be downloaded"
fi

# Held to the signature inside before anything else happens with it.
say "Verifying $ZIP"
VSTATUS=0
VERIFIED=$(verify_zip "$DROP/$ZIP") || VSTATUS=$?
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

# The way back to the stable release, on a player that has Glass: the
# settings as they stand are backed up, and what they become is made by the
# stable release's own plan (its manager/stable.js, out of its zip): its
# defaults, with the screen's and the network's set-up kept.
PLANNED=
if [ "$STABLE" = yes ] && [ -n "$HERE" ]; then
  WORK=$(mktemp -d)
  "$NODE" "$ZIPCHECK" extract "$DROP/$ZIP" "$WORK" package.json config.json config/meter.txt.tmpl config/spectrum.txt.tmpl manager/stable.js node_modules/ini/ || fail "the release's zip could not be read"
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
