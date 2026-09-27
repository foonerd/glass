'use strict';

const test = require('node:test');
const assert = require('node:assert');
const { classify, allowed, normalize, makeLogger } = require('../logging');

test('lines are classified by their prefix', () => {
  assert.deepStrictEqual(classify('channel: a display connected'), { target: 'channel', level: 'info' });
  assert.deepStrictEqual(classify('pushState: status=play'), { target: 'channel', level: 'verbose' });
  assert.deepStrictEqual(classify('glass-serve: 10.0.0.5:4000 subscribed (x, 0.7.9)'), { target: 'remotes', level: 'verbose' });
  assert.deepStrictEqual(classify('glass-serve: ring /dev/shm/x'), { target: 'remotes', level: 'info' });
  assert.deepStrictEqual(classify('upgrade: handing x to the plugin manager'), { target: 'manager', level: 'info' });
  assert.deepStrictEqual(classify('the display died 0 s after launch'), { target: 'display', level: 'info' });
  assert.deepStrictEqual(classify('meter=gold'), { target: 'display', level: 'info' });
  assert.deepStrictEqual(classify('frame.rate=30 size=1280x720'), { target: 'display', level: 'info' });
  assert.deepStrictEqual(classify('fonts loaded 5 of 5'), { target: 'display', level: 'verbose' });
  assert.deepStrictEqual(classify('frames 43/s, received 400, refused 0, channel up'), { target: 'remotes', level: 'trace' });
  assert.deepStrictEqual(classify('Soloist nudged: x'), { target: 'audio', level: 'info' });
  assert.deepStrictEqual(classify('something new'), { target: 'settings', level: 'info' });
});

test('the gate follows the level and, at verbose and trace, the targets', () => {
  assert.deepStrictEqual(normalize({}), { level: 'warn', targets: [] });
  assert.strictEqual(allowed({ level: 'warn' }, 'error', 'display'), true);
  assert.strictEqual(allowed({ level: 'warn' }, 'info', 'display'), false);
  assert.strictEqual(allowed({ level: 'info' }, 'info', 'display'), true);
  assert.strictEqual(allowed({ level: 'info' }, 'verbose', 'display'), false);
  assert.strictEqual(allowed({ level: 'verbose', targets: [] }, 'verbose', 'display'), true);
  assert.strictEqual(allowed({ level: 'verbose', targets: ['channel'] }, 'verbose', 'display'), false);
  assert.strictEqual(allowed({ level: 'verbose', targets: ['channel'] }, 'info', 'display'), true, 'targets gate only the fine levels');
  assert.strictEqual(allowed({ level: 'trace', targets: ['remotes'] }, 'trace', 'remotes'), true);
});

test('the logger in front of the player writes what the gate allows', () => {
  const written = [];
  const base = { info: (m) => written.push(['info', m]), warn: (m) => written.push(['warn', m]), error: (m) => written.push(['error', m]) };
  let settings = { level: 'warn', targets: [] };
  const log = makeLogger(base, 'glass: ', () => settings);
  log.info('glass: channel: a display connected');
  log.warn('glass: the display died');
  log.error('glass: broken');
  assert.deepStrictEqual(written, [['warn', 'glass: the display died'], ['error', 'glass: broken']]);
  settings = { level: 'info', targets: [] };
  log.info('glass: channel: a display connected');
  log.info('glass: pushState: status=play');
  assert.strictEqual(written.length, 3, 'a state push is verbose and stays out at info');
  settings = { level: 'verbose', targets: ['channel'] };
  log.info('glass: pushState: status=play');
  log.verbose('glass: upgrade: step');
  assert.strictEqual(written.length, 4, 'verbose lines of other targets stay out');
  assert.strictEqual(written[3][1], 'glass: pushState: status=play');
});
