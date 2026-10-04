'use strict';
// Whether this player can draw on a screen of the display's own, asked
// before the screen is handed over and said on the status sheet.
//
// On a screen with no X server the display draws through GBM and EGL, the
// system's graphics libraries. Where they do not work the display dies as
// it starts with one sentence of SDL's ("Can't load EGL/GL library on
// window creation"), and a take leaves the screen black until the kiosk has
// it back. Three things say beforehand whether they work, and why not:
//
// - the probe, `glass --probe-graphics`: the display's own try at each
//   step, with the loader's words where a library will not load;
// - the kiosk's X server, which tries the same libraries when it starts and
//   writes into its log whether it draws with the GPU or fell back to
//   drawing in software;
// - the versions of Mesa's parts, which belong together.
//
// Pure but for `check`, which runs the probe and reads the log.

const { execFile } = require('child_process');
const fs = require('fs');

// Mesa's parts on a Debian system: one source, one version, or the
// libraries do not fit each other.
const MESA_PARTS = ['libegl-mesa0', 'libgbm1', 'libgl1-mesa-dri', 'libglapi-mesa', 'libglx-mesa0'];

// What the probe printed, as an object, or null when it printed nothing
// that reads as one (an older display that does not know the switch).
function parseProbe(stdout) {
  const line = String(stdout || '').split('\n').map(function (l) { return l.trim(); }).filter(function (l) { return l[0] === '{'; }).pop();
  if (!line) return null;
  try {
    const probe = JSON.parse(line);
    if (!probe || typeof probe !== 'object' || typeof probe.ok !== 'boolean') return null;
    return {
      applies: probe.applies === true,
      ok: probe.ok === true,
      failed: String(probe.failed || ''),
      reason: String(probe.reason || ''),
      driver: String(probe.driver || ''),
      connector: String(probe.connector || ''),
      renderer: String(probe.renderer || ''),
      software: probe.software === true,
      steps: Array.isArray(probe.steps) ? probe.steps.map(function (s) { return { step: String(s.step || ''), ok: s.ok === true, detail: String(s.detail || '') }; }) : []
    };
  } catch (e) {
    return null;
  }
}

// What the kiosk's X server says of itself in its log: `accelerated` with
// the renderer it names, `software` with the line that says why, or
// `unknown` where the log has neither (no log, another driver).
function xVerdict(logText) {
  const lines = String(logText || '').split('\n');
  const find = function (re) {
    for (let i = lines.length - 1; i >= 0; i--) { const m = re.exec(lines[i]); if (m) return m; }
    return null;
  };
  const accelerated = find(/glamor X acceleration enabled on (.+?)\s*$/);
  const failed = find(/(eglGetDisplay\(\) failed)/) || find(/(glamor initialization failed)/);
  if (failed && !accelerated) return { state: 'software', detail: failed[1] };
  if (accelerated) return { state: 'accelerated', detail: accelerated[1] };
  return { state: 'unknown', detail: '' };
}

// The versions of Mesa's parts from `dpkg-query -W -f='${Package} ${Version}\n'`:
// which are installed, and whether they are all of one version.
function mesaParts(dpkgText) {
  const versions = {};
  String(dpkgText || '').split('\n').forEach(function (line) {
    const m = /^(\S+)\s+(\S+)\s*$/.exec(line.trim());
    if (m && MESA_PARTS.indexOf(m[1]) !== -1) versions[m[1]] = m[2];
  });
  const found = Object.keys(versions);
  const distinct = found.map(function (p) { return versions[p]; }).filter(function (v, i, all) { return all.indexOf(v) === i; });
  return { versions: versions, agree: distinct.length <= 1, version: distinct.length === 1 ? distinct[0] : '' };
}

// What the page and the take are told, from what was found: whether the
// player can draw on the screen itself (`ok`, null where nothing could be
// asked), the reason where it cannot, and the X server's and Mesa's side.
function summary(found) {
  const f = found || {};
  const probe = f.probe || null;
  const x = f.x || { state: 'unknown', detail: '' };
  const mesa = f.mesa || { versions: {}, agree: true, version: '' };
  return {
    asked: !!probe,
    applies: probe ? probe.applies : false,
    ok: probe ? probe.ok : null,
    failed: probe ? probe.failed : '',
    reason: probe ? probe.reason : '',
    driver: probe ? probe.driver : '',
    connector: probe ? probe.connector : '',
    renderer: probe ? probe.renderer : '',
    software: probe ? probe.software : false,
    x: x,
    mesa: mesa,
    at: f.at || 0
  };
}

// Whether a take is held back: only where the screen would be drawn on
// itself (`kms`), the probe was run, applies and failed. Where glass-evo
// would draw on an X server brought up for it, the probe has no say; and a
// display too old to probe holds nothing back.
function blocksTake(mode, graphics) {
  const g = graphics || {};
  return mode === 'kms' && g.asked === true && g.applies === true && g.ok === false;
}

// Run the probe as the display's user and read the X log and Mesa's
// versions. Resolves with what was found; never rejects: what could not be
// asked is null or unknown.
function check(options) {
  const opts = options || {};
  const run = opts.execFile || execFile;
  const read = opts.readFile || fs.readFile;
  const probe = new Promise(function (resolve) {
    if (!opts.bin) return resolve(null);
    const o = { timeout: 10000, encoding: 'utf8' };
    if (opts.uid !== undefined) { o.uid = opts.uid; o.gid = opts.gid; }
    run(opts.bin, ['--probe-graphics'], o, function (error, stdout) { resolve(parseProbe(stdout)); });
  });
  const x = new Promise(function (resolve) {
    read(opts.xLog || '/var/log/Xorg.0.log', 'utf8', function (error, text) { resolve(error ? { state: 'unknown', detail: '' } : xVerdict(text)); });
  });
  const mesa = new Promise(function (resolve) {
    run('/usr/bin/dpkg-query', ['-W', '-f=${Package} ${Version}\\n'].concat(MESA_PARTS), { timeout: 5000, encoding: 'utf8' }, function (error, stdout) { resolve(mesaParts(stdout)); });
  });
  return Promise.all([probe, x, mesa]).then(function (all) {
    return { probe: all[0], x: all[1], mesa: all[2], at: Date.now() };
  });
}

module.exports = { MESA_PARTS: MESA_PARTS, parseProbe: parseProbe, xVerdict: xVerdict, mesaParts: mesaParts, summary: summary, blocksTake: blocksTake, check: check };
