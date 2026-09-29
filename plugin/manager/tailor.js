'use strict';
// The cutter and the packager as the Manager runs them: the display's own
// binary through the launcher, as the previews are rendered, into a staging
// folder, and what it printed read back for the job.

const { spawn } = require('child_process');
const path = require('path');

const CUT_TIMEOUT_MS = 5 * 60 * 1000;
const PACKAGE_TIMEOUT_MS = 30 * 60 * 1000;

// `WxH` as a size, or null.
function parseSize(text) {
  const m = /^\s*(\d{2,5})\s*x\s*(\d{2,5})\s*$/i.exec(String(text || ''));
  if (!m) return null;
  const width = parseInt(m[1], 10);
  const height = parseInt(m[2], 10);
  if (!(width >= 64 && width <= 7680 && height >= 64 && height <= 4320)) return null;
  return { width: width, height: height };
}

// One run of the display through the launcher; resolves with what it
// wrote, rejects with its last line when it left with an error.
function run(opts) {
  return new Promise(function (resolve, reject) {
    const env = Object.assign({}, opts.env || {});
    delete env.DISPLAY;
    const options = { env: env, stdio: ['ignore', 'pipe', 'pipe'] };
    if (opts.uid !== undefined) { options.uid = opts.uid; options.gid = opts.gid; }
    const child = spawn(opts.launcher, opts.args, options);
    let out = '';
    let err = '';
    let pending = '';
    child.stdout.on('data', function (d) {
      out += d;
      if (out.length > 262144) out = out.slice(-131072);
      if (opts.onLine) {
        pending += d;
        const lines = pending.split('\n');
        pending = lines.pop();
        lines.forEach(function (line) { try { opts.onLine(line.trim()); } catch (e) { /* the listener's own */ } });
      }
    });
    child.stderr.on('data', function (d) { err += d; if (err.length > 262144) err = err.slice(-131072); });
    const timer = setTimeout(function () { child.kill('SIGKILL'); }, opts.timeoutMs || CUT_TIMEOUT_MS);
    child.on('error', function (e) { clearTimeout(timer); reject(e); });
    child.on('close', function (code, signal) {
      clearTimeout(timer);
      if (signal) return reject(new Error('the display was stopped (' + signal + ')'));
      if (code !== 0) return reject(new Error('the display left with code ' + code + (err ? ': ' + err.trim().split('\n').pop() : '')));
      resolve({ out: out, err: err });
    });
  });
}

// The warnings the cutter printed: its own lines, without the prefix.
function warningsOf(err) {
  return String(err || '').split('\n')
    .map(function (l) { return l.trim(); })
    .filter(function (l) { return l.startsWith('glass: '); })
    .map(function (l) { return l.slice(7); });
}

// Cut a theme folder to a size into `out`: resolves with the new name, the
// folders written (install tree and path) and the cutter's warnings.
async function cut(opts) {
  const args = ['--tailor', opts.width + 'x' + opts.height, '--theme', opts.themeDir, '--out', opts.out];
  if (opts.stretch) args.push('--stretch');
  const result = await run({ launcher: opts.launcher, env: opts.env, uid: opts.uid, gid: opts.gid, args: args, timeoutMs: CUT_TIMEOUT_MS });
  const lines = result.out.split('\n').map(function (l) { return l.trim(); }).filter(Boolean);
  const done = lines.map(function (l) { return /^glass: (.+) cut to \d+x\d+: (\d+) pictures resampled$/.exec(l); }).find(Boolean);
  if (!done) throw new Error('the cutter did not say it was done' + (lines.length ? ': ' + lines[lines.length - 1] : ''));
  const folders = lines
    .filter(function (l) { return !l.startsWith('glass:'); })
    .map(function (p) {
      const install = path.basename(path.dirname(p));
      return { install: install, folder: path.basename(p), path: p };
    })
    .filter(function (f) { return f.install === 'templates' || f.install === 'templates_spectrum'; });
  if (!folders.length) throw new Error('the cutter wrote no folder');
  return { name: done[1], pictures: parseInt(done[2], 10), folders: folders, warnings: warningsOf(result.err) };
}

// Package a theme folder into `out`: resolves with the zip's path. Each
// meter's snapshot, as the display reports it, counts up `onProgress`.
async function pack(opts) {
  const args = ['--headless', '--package', '--theme', opts.themeDir, '--out', opts.out, '--settle', String(opts.settle || 3)];
  let shots = 0;
  const result = await run({
    launcher: opts.launcher, env: opts.env, uid: opts.uid, gid: opts.gid, args: args, timeoutMs: PACKAGE_TIMEOUT_MS,
    onLine: function (line) {
      if (/^glass: snapshot /.test(line)) {
        shots += 1;
        if (opts.onProgress) opts.onProgress(shots);
      }
    }
  });
  const lines = result.out.split('\n').map(function (l) { return l.trim(); }).filter(Boolean);
  const zip = lines.find(function (l) { return l.endsWith('.zip') && !l.startsWith('glass:'); });
  if (!zip) throw new Error('the packager wrote no zip' + (lines.length ? ': ' + lines[lines.length - 1] : ''));
  return { zip: zip, warnings: warningsOf(result.err) };
}

module.exports = { parseSize, run, cut, pack, CUT_TIMEOUT_MS, PACKAGE_TIMEOUT_MS };
