'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const fsp = require('fs/promises');
const os = require('os');
const path = require('path');
const { parseSize, cut, pack } = require('../tailor');

// A launcher that behaves as the display's cutter and packager do on the
// outside: the folders written and the lines printed.
async function fakeLauncher(dir) {
  const script = path.join(dir, 'glass.sh');
  await fsp.writeFile(script, `#!/bin/sh
out=""; theme=""; mode=""; size=""
while [ $# -gt 0 ]; do
  case "$1" in
    --tailor) mode=tailor; size="$2"; shift ;;
    --package) mode=package ;;
    --theme) theme="$2"; shift ;;
    --out) out="$2"; shift ;;
  esac
  shift
done
name=$(basename "$theme")
if [ "$mode" = tailor ]; then
  new="\${size}_$(echo "$name" | cut -d_ -f2-)"
  mkdir -p "$out/templates/$new" "$out/templates_spectrum/$new"
  echo "cut" > "$out/templates/$new/meters.txt"
  echo "cut" > "$out/templates_spectrum/$new/spectrum.txt"
  echo "$out/templates/$new"
  echo "$out/templates_spectrum/$new"
  echo "glass: meters.txt: [m] odd.key: not a key the cutter knows; kept as it is" >&2
  echo "glass: $new cut to $size: 7 pictures resampled"
else
  mkdir -p "$out"
  echo "zipbytes" > "$out/$name.zip"
  echo "glass: snapshot $out/.package-1/$name/m.png"
  echo "$out/$name.zip"
  echo "glass: $name packaged for the catalogue"
fi
`);
  await fsp.chmod(script, 0o755);
  return script;
}

test('a size is read as WxH within reason', () => {
  assert.deepEqual(parseSize('1280x720'), { width: 1280, height: 720 });
  assert.deepEqual(parseSize(' 3840 X 2160 '), { width: 3840, height: 2160 });
  assert.equal(parseSize('1280'), null);
  assert.equal(parseSize('10x10'), null);
  assert.equal(parseSize('99999x1'), null);
});

test('the cutter\'s folders, name and warnings come back, and the packager\'s zip', async () => {
  const dir = await fsp.mkdtemp(path.join(os.tmpdir(), 'glass-tailor-'));
  const launcher = await fakeLauncher(dir);
  const theme = path.join(dir, 'templates', '1920x1080_x');
  fs.mkdirSync(theme, { recursive: true });
  const out = path.join(dir, 'staging');
  const result = await cut({ launcher, env: {}, themeDir: theme, width: 1280, height: 720, out });
  assert.equal(result.name, '1280x720_x');
  assert.equal(result.pictures, 7);
  assert.deepEqual(result.folders.map(f => [f.install, f.folder]), [['templates', '1280x720_x'], ['templates_spectrum', '1280x720_x']]);
  assert.equal(fs.readFileSync(path.join(out, 'templates', '1280x720_x', 'meters.txt'), 'utf8').trim(), 'cut');
  assert.deepEqual(result.warnings, ['meters.txt: [m] odd.key: not a key the cutter knows; kept as it is']);
  const packed = await pack({ launcher, env: {}, themeDir: theme, out: path.join(dir, 'pkg'), settle: 2 });
  assert.equal(packed.zip, path.join(dir, 'pkg', '1920x1080_x.zip'));
  assert.equal(fs.readFileSync(packed.zip, 'utf8').trim(), 'zipbytes');
  await assert.rejects(cut({ launcher: path.join(dir, 'missing.sh'), env: {}, themeDir: theme, width: 1280, height: 720, out }));
  await fsp.rm(dir, { recursive: true, force: true });
});
