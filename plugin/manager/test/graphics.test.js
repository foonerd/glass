'use strict';
// The graphics check: what the probe printed, what the kiosk's X server
// wrote of itself, whether Mesa's parts belong together, and when a take
// is held back. The log lines are as two players wrote them: one that
// works, and the one whose take failed with "Can't load EGL/GL library".
const test = require('node:test');
const assert = require('node:assert/strict');
const { parseProbe, xVerdict, mesaParts, summary, blocksTake, check, MESA_PARTS } = require('../graphics');

const WORKS = '{"applies":true,"ok":true,"failed":"","reason":"EGL works on the screen\'s device (EGL 1.5, Mesa Project, 1.5)","steps":[{"step":"card","ok":true,"detail":"/dev/dri/card1"},{"step":"egl-initialize","ok":true,"detail":"EGL 1.5, Mesa Project, 1.5"}]}';
const BROKEN = '{"applies":true,"ok":false,"failed":"egl-display","reason":"EGL gave no display for /dev/dri/card1 (EGL_SUCCESS); the EGL vendor library libEGL_mesa.so.0 would not load: libxshmfence.so.1: cannot open shared object file: No such file or directory","steps":[{"step":"egl-display","ok":false,"detail":"EGL gave no display for /dev/dri/card1 (EGL_SUCCESS)"}]}';
const NO_SCREEN = '{"applies":false,"ok":true,"failed":"","reason":"the kernel drives no screen here: nothing to draw on without an X server","steps":[]}';

test('the probe\'s line is read, whatever else the display wrote', () => {
  assert.equal(parseProbe(WORKS).ok, true);
  assert.equal(parseProbe('libEGL warning: something\n' + BROKEN + '\n').failed, 'egl-display');
  assert.equal(parseProbe(BROKEN).steps[0].detail, 'EGL gave no display for /dev/dri/card1 (EGL_SUCCESS)');
  assert.equal(parseProbe(NO_SCREEN).applies, false);
  // A display that does not know the switch prints its usage or nothing.
  assert.equal(parseProbe('glass: unknown argument --probe-graphics'), null);
  assert.equal(parseProbe(''), null);
  assert.equal(parseProbe(undefined), null);
  assert.equal(parseProbe('{"not":"a probe"}'), null);
  assert.equal(parseProbe('{broken'), null);
});

test('the kiosk\'s X server says in its log whether it draws with the GPU', () => {
  const good = [
    '[    23.822] (II) modeset(0): using drv /dev/dri/card1',
    '[    23.823] (II) Loading sub module "glamoregl"',
    '[    23.919] (II) modeset(0): glamor X acceleration enabled on V3D 7.1.10.2',
    '[    23.919] (II) modeset(0): glamor initialized'
  ].join('\n');
  assert.deepEqual(xVerdict(good), { state: 'accelerated', detail: 'V3D 7.1.10.2' });
  const bad = [
    '[    27.408] (II) Loading sub module "glamoregl"',
    '[    27.590] (EE) modeset(0): eglGetDisplay() failed',
    '[    27.590] (II) modeset(0): glamor initialization failed',
    '[    27.887] (II) IGLX: Loaded and initialized swrast'
  ].join('\n');
  assert.deepEqual(xVerdict(bad), { state: 'software', detail: 'eglGetDisplay() failed' });
  assert.deepEqual(xVerdict('[ 1.0] (II) modeset(0): glamor initialization failed'), { state: 'software', detail: 'glamor initialization failed' });
  // A log that speaks of neither, and no log.
  assert.deepEqual(xVerdict('[ 1.0] (II) FBDEV(0): using default device'), { state: 'unknown', detail: '' });
  assert.deepEqual(xVerdict(''), { state: 'unknown', detail: '' });
  assert.deepEqual(xVerdict(undefined), { state: 'unknown', detail: '' });
});

test('Mesa\'s parts are of one version, or it is said which are not', () => {
  const one = 'libegl-mesa0 24.2.8-1~bpo12+rpt5\nlibgbm1 24.2.8-1~bpo12+rpt5\nlibgl1-mesa-dri 24.2.8-1~bpo12+rpt5\nlibglapi-mesa 24.2.8-1~bpo12+rpt5\nlibglx-mesa0 24.2.8-1~bpo12+rpt5\n';
  const got = mesaParts(one);
  assert.equal(got.agree, true);
  assert.equal(got.version, '24.2.8-1~bpo12+rpt5');
  assert.equal(Object.keys(got.versions).length, MESA_PARTS.length);
  const mixed = mesaParts(one.replace('libglapi-mesa 24.2.8-1~bpo12+rpt5', 'libglapi-mesa 23.2.1-1'));
  assert.equal(mixed.agree, false);
  assert.equal(mixed.version, '');
  assert.equal(mixed.versions['libglapi-mesa'], '23.2.1-1');
  // What is not Mesa's is passed over; nothing installed agrees with itself.
  assert.deepEqual(mesaParts('libdrm2 2.4.123\n').versions, {});
  assert.equal(mesaParts('').agree, true);
});

test('a take is held back only where the screen would be drawn on itself and the probe failed', () => {
  const broken = summary({ probe: parseProbe(BROKEN) });
  const works = summary({ probe: parseProbe(WORKS) });
  assert.equal(blocksTake('kms', broken), true);
  assert.equal(blocksTake('kms', works), false);
  assert.equal(blocksTake('x', broken), false, 'on an X server brought up for the face the probe has no say');
  assert.equal(blocksTake(null, broken), false);
  assert.equal(blocksTake('kms', summary({ probe: parseProbe(NO_SCREEN) })), false);
  assert.equal(blocksTake('kms', summary({ probe: null })), false, 'a display too old to probe holds nothing back');
  assert.equal(blocksTake('kms', undefined), false);
  assert.equal(broken.reason.indexOf('libEGL_mesa.so.0 would not load') !== -1, true);
  assert.equal(summary(undefined).asked, false);
  assert.equal(summary({ probe: null }).ok, null);
});

test('the check runs the probe as the display\'s user and reads the log and the versions', async () => {
  const calls = [];
  const found = await check({
    bin: '/plugin/bin/arm/glass',
    uid: 1000,
    gid: 1000,
    xLog: '/var/log/Xorg.0.log',
    execFile: function (file, args, options, done) {
      calls.push([file, args.slice(), options.uid]);
      if (file === '/plugin/bin/arm/glass') return done(null, BROKEN + '\n', '');
      done(null, 'libegl-mesa0 24.2.8-1~bpo12+rpt4\nlibgbm1 24.2.8-1~bpo12+rpt4\n', '');
    },
    readFile: function (file, encoding, done) { done(null, '[ 27.590] (EE) modeset(0): eglGetDisplay() failed\n'); }
  });
  assert.deepEqual(calls[0], ['/plugin/bin/arm/glass', ['--probe-graphics'], 1000]);
  assert.equal(calls[1][0], '/usr/bin/dpkg-query');
  const told = summary(found);
  assert.equal(told.ok, false);
  assert.equal(told.x.state, 'software');
  assert.equal(told.mesa.version, '24.2.8-1~bpo12+rpt4');
  assert.ok(told.at > 0);
  // Nothing that answers: nothing known, and nothing thrown.
  const none = await check({
    bin: '/nowhere/glass',
    execFile: function (file, args, options, done) { done(new Error('ENOENT'), '', ''); },
    readFile: function (file, encoding, done) { done(new Error('ENOENT')); }
  });
  assert.equal(summary(none).asked, false);
  assert.equal(summary(none).x.state, 'unknown');
  assert.equal(await check({ execFile: function (f, a, o, done) { done(null, '', ''); }, readFile: function (f, e, done) { done(null, ''); } }).then(function (f) { return f.probe; }), null, 'no binary, no probe');
});
