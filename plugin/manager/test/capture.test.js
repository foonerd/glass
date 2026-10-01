'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { Capture, parseReply, describe, LIMIT_MS, RAISED } = require('../capture');

// A player's side of a capture, in memory: the marker's file, the log
// settings, a clock and timers the test turns by hand.
function rig(over) {
  const world = {
    now: 1000000, files: {}, settings: { level: 'warn', targets: ['display'] }, set: [], relaunched: 0, sent: [], timers: [],
    reply: '{"status":"OK","link":"http://logs.example.org/volumio/AbCdEfG.html"}'
  };
  const deps = Object.assign({
    file: '/data/capture.json',
    now: () => world.now,
    read: (p) => { if (!(p in world.files)) throw new Error('none'); return world.files[p]; },
    write: (p, text) => { world.files[p] = text; },
    remove: (p) => { delete world.files[p]; },
    logSettings: () => world.settings,
    setLogSettings: (s) => { world.settings = s; world.set.push(s); },
    relaunch: () => { world.relaunched += 1; },
    sendLog: (description) => { world.sent.push(description); return Promise.resolve(world.reply); },
    setTimer: (fn, ms) => { const t = { fn, at: world.now + ms, live: true }; world.timers.push(t); return t; },
    clearTimer: (t) => { t.live = false; }
  }, over || {});
  world.turn = function (ms) {
    world.now += ms;
    world.timers.filter((t) => t.live && t.at <= world.now).forEach((t) => { t.live = false; t.fn(); });
  };
  return { world, deps, capture: new Capture(deps) };
}

test('a capture raises the level, sends the log when it has happened, and puts the level back', async function () {
  const { world, capture } = rig();
  assert.deepStrictEqual(capture.view(), { state: 'idle' });
  const began = capture.start('touch', { found: false, findings: [] });
  assert.strictEqual(began.state, 'capturing');
  assert.strictEqual(began.until - began.startedAt, LIMIT_MS);
  assert.deepStrictEqual(world.settings, RAISED);
  assert.strictEqual(world.relaunched, 1, 'the display starts again so its own lines are full');
  assert.ok(world.files['/data/capture.json'], 'the marker is on disk');
  assert.throws(() => capture.start('touch'), /capturing/);
  world.turn(90 * 1000);
  const report = await capture.finish('0.7.94', '  tapped play, \n nothing happened  ');
  assert.deepStrictEqual(report.log, { link: 'http://logs.example.org/volumio/AbCdEfG.html' });
  assert.strictEqual(report.symptom, 'touch');
  assert.strictEqual(report.endedAt - report.startedAt, 90 * 1000);
  assert.strictEqual(report.text, 'tapped play, \n nothing happened');
  assert.deepStrictEqual(report.diagnosis, { found: false, findings: [] });
  assert.deepStrictEqual(world.sent, ['Glass 0.7.94 report: touch - tapped play, nothing happened']);
  assert.deepStrictEqual(world.settings, { level: 'warn', targets: ['display'] }, 'the level is back as it was');
  assert.strictEqual(world.files['/data/capture.json'], undefined, 'the marker is gone');
  assert.deepStrictEqual(capture.view(), { state: 'idle' });
  assert.ok(world.timers.every((t) => !t.live), 'no timer is left');
  await assert.rejects(capture.finish('0.7.94', 'again'), /not-capturing/);
});

test('the level goes back by itself: on a cancel, when the time runs out, whatever the sending comes to', async function () {
  let r = rig();
  r.capture.start('slow');
  r.capture.cancel();
  assert.deepStrictEqual(r.world.settings, { level: 'warn', targets: ['display'] });
  assert.deepStrictEqual(r.capture.view(), { state: 'idle' });

  r = rig();
  r.capture.start('slow');
  r.world.turn(LIMIT_MS - 1);
  assert.strictEqual(r.capture.view().state, 'capturing');
  r.world.turn(1);
  assert.deepStrictEqual(r.capture.view(), { state: 'idle' }, 'ten minutes and it is over');
  assert.deepStrictEqual(r.world.settings, { level: 'warn', targets: ['display'] });
  assert.strictEqual(r.world.files['/data/capture.json'], undefined);

  r = rig({ sendLog: () => Promise.reject(new Error('the submitter is not there')) });
  r.capture.start('other');
  let report = await r.capture.finish('0.7.94', 'x');
  assert.deepStrictEqual(report.log, { error: 'not-sent', said: 'the submitter is not there' });
  assert.deepStrictEqual(r.world.settings, { level: 'warn', targets: ['display'] });

  // No word from the submitter: the wait ends and the level goes back.
  r = rig({ sendLog: () => new Promise(() => {}) });
  r.capture.start('other');
  const pending = r.capture.finish('0.7.94', 'x');
  assert.strictEqual(r.capture.view().state, 'sending');
  assert.throws(() => r.capture.cancel(), /sending/);
  r.world.turn(LIMIT_MS + 1);
  assert.strictEqual(r.capture.view().state, 'sending', 'the limit does not cut a sending short');
  r.world.timers.filter((t) => t.live).forEach((t) => { t.live = false; t.fn(); });
  report = await pending;
  assert.deepStrictEqual(report.log, { error: 'no-reply', said: '' });
  assert.deepStrictEqual(r.world.settings, { level: 'warn', targets: ['display'] });
});

test('a backend that restarted in the middle picks the capture up, or ends one whose time has passed', function () {
  const first = rig();
  first.capture.start('restarts', null);
  const marker = first.world.files['/data/capture.json'];

  // The backend comes back two minutes on: the level stays raised, the timer runs for the rest.
  let again = rig();
  again.world.files['/data/capture.json'] = marker;
  again.world.settings = RAISED;
  again.world.now = first.world.now + 2 * 60 * 1000;
  assert.strictEqual(again.capture.resume().state, 'capturing');
  assert.deepStrictEqual(again.world.settings, RAISED);
  again.world.turn(LIMIT_MS - 2 * 60 * 1000);
  assert.deepStrictEqual(again.capture.view(), { state: 'idle' });
  assert.deepStrictEqual(again.world.settings, { level: 'warn', targets: ['display'] });

  // It comes back after the time: the level goes back at once.
  again = rig();
  again.world.files['/data/capture.json'] = marker;
  again.world.settings = RAISED;
  again.world.now = first.world.now + LIMIT_MS + 5;
  assert.deepStrictEqual(again.capture.resume(), { state: 'idle' });
  assert.deepStrictEqual(again.world.settings, { level: 'warn', targets: ['display'] });
  assert.strictEqual(again.world.files['/data/capture.json'], undefined);

  // No marker, or one that is not a marker: nothing to do, nothing set.
  again = rig();
  again.world.files['/data/capture.json'] = 'not json';
  assert.deepStrictEqual(again.capture.resume(), { state: 'idle' });
  assert.deepStrictEqual(again.world.set, []);
});

test('the submitter\'s words are read as a link or as a log that stayed on the player', function () {
  assert.deepStrictEqual(parseReply('{"status":"OK","link":"http://logs.volumio.org/volumio/AbCdEfG.html"}\n'), { link: 'http://logs.volumio.org/volumio/AbCdEfG.html' });
  assert.deepStrictEqual(parseReply('Cannot send bug report: Error: Command failed: /usr/bin/curl\nSaving as: /var/tmp/logondemand\n'),
    { error: 'not-sent', said: 'Cannot send bug report: Error: Command failed: /usr/bin/curl' });
  assert.deepStrictEqual(parseReply('{"status":"ERROR"}'), { error: 'not-sent', said: '{"status":"ERROR"}' });
  assert.deepStrictEqual(parseReply('{"status":"OK","link":"javascript:alert(1)"}').error, 'not-sent');
  assert.deepStrictEqual(parseReply(''), { error: 'no-reply', said: '' });
  assert.strictEqual(describe('0.7.94', 'no-meters', ''), 'Glass 0.7.94 report: no-meters');
  assert.strictEqual(describe('0.7.94', 'slow', 'a\nb   c'), 'Glass 0.7.94 report: slow - a b c');
  assert.strictEqual(describe('0.7.94', 'slow', 'x'.repeat(900)).length, 'Glass 0.7.94 report: slow - '.length + 300);
});
